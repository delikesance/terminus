//! Background self-update, driven by [`terminus_update`].
//!
//! A worker thread checks for a new release shortly after start and then once
//! a day (`[updates] check`), and on demand from the command palette. What
//! happens next depends on how Terminus was installed:
//!
//! * portable tarball, Windows per-user or portable copy: checked when the
//!   app starts ([`launch_check`], at most [`LAUNCH_CHECK_TIMEOUT`]); a new
//!   release is downloaded, verified and swapped in behind an "Updating"
//!   screen, then the new version starts. Later checks swap it in silently
//!   and the user restarts when convenient;
//! * Windows installer: offered; "Install Update" downloads the verified
//!   installer, starts it and quits so it can replace the executable;
//! * `.deb` / `.rpm`: offered; "Install Update" downloads the verified package
//!   and shows the one command that installs it;
//! * Nix, development builds, unknown layouts: the user is told how to update.
//!
//! The UI thread only [`Updater::pump`]s state and acts on it; nothing here
//! blocks rendering.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use terminus_update::{Installer, Release, UpdatePlan};

/// First automatic check, after start-up settles.
const STARTUP_DELAY: Duration = Duration::from_secs(20);
/// Between automatic checks.
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
/// Longest the launch-time check may delay the window.
pub const LAUNCH_CHECK_TIMEOUT: Duration = Duration::from_secs(2);
/// The version this binary reports to the release check.
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Where an update stands, as the UI shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateState {
    Idle,
    Checking,
    UpToDate,
    /// A newer release exists and waits for the user (or cannot be
    /// installed from here: `plan` says why).
    Available {
        version: String,
        page_url: String,
        plan: UpdatePlan,
        can_install: bool,
    },
    Downloading {
        version: String,
    },
    /// The binary on disk is the new version; restart to use it.
    ReadyToRestart {
        version: String,
    },
    /// A verified installer is downloaded; the UI starts it and quits.
    InstallerReady {
        version: String,
        path: PathBuf,
        installer: Installer,
    },
    /// A verified package is downloaded; `command` installs it.
    PackageReady {
        version: String,
        command: String,
    },
    Failed(String),
}

/// What to do once a newer release has been found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NextStep {
    /// Download and install now, without asking.
    AutoInstall,
    /// Offer "Install Update".
    Offer,
    /// Tell the user; nothing can be installed from here.
    Inform,
}

/// Only in-place binary swaps happen unattended: an installer needs UAC and
/// closes the app, a package needs root.
pub(crate) fn next_step(
    plan: &UpdatePlan,
    can_install: bool,
    auto_install: bool,
) -> NextStep {
    match plan {
        UpdatePlan::Manual { .. } => NextStep::Inform,
        _ if !can_install => NextStep::Inform,
        UpdatePlan::ReplaceBinary { .. } if auto_install => NextStep::AutoInstall,
        _ => NextStep::Offer,
    }
}

/// One-line notice for the sidebar band. `manual`: the user asked, so even
/// "no update" deserves an answer.
pub(crate) fn notice_for(state: &UpdateState, manual: bool) -> Option<String> {
    match state {
        UpdateState::Idle | UpdateState::Checking => None,
        UpdateState::UpToDate => {
            manual.then(|| format!("Terminus {CURRENT_VERSION} is up to date"))
        }
        UpdateState::Available {
            version,
            plan,
            can_install,
            ..
        } => Some(match plan {
            UpdatePlan::Manual { hint } => format!("Terminus {version} is available. {hint}"),
            _ if !can_install => format!(
                "Terminus {version} is available. Run “Install Update” to open the release page"
            ),
            UpdatePlan::RunInstaller { .. } => format!(
                "Terminus {version} is available. Run “Install Update” (Terminus restarts)"
            ),
            _ => format!("Terminus {version} is available. Run “Install Update”"),
        }),
        UpdateState::Downloading { version } => Some(format!("Downloading Terminus {version}…")),
        UpdateState::ReadyToRestart { version } => Some(format!(
            "Terminus {version} is installed. Run “Restart to Update”"
        )),
        UpdateState::InstallerReady { version, .. } => {
            Some(format!("Starting the Terminus {version} installer…"))
        }
        UpdateState::PackageReady { version, .. } => Some(format!(
            "Terminus {version} is downloaded. Run “Install Update” to copy the install command"
        )),
        UpdateState::Failed(message) => Some(format!("Update failed: {message}")),
    }
}

/// What “Install Update” does in a given state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InstallAction {
    /// Ask the worker to download and apply (or retry) the update.
    Install,
    /// The package is verified on disk; installing it needs the user's shell.
    CopyCommand(String),
    /// This copy cannot update itself; send the user to the release.
    OpenPage(String),
}

pub(crate) fn install_action(state: &UpdateState) -> InstallAction {
    match state {
        UpdateState::PackageReady { command, .. } => {
            InstallAction::CopyCommand(command.clone())
        }
        UpdateState::Available {
            page_url,
            plan,
            can_install,
            ..
        } if !can_install || matches!(plan, UpdatePlan::Manual { .. }) => {
            InstallAction::OpenPage(page_url.clone())
        }
        _ => InstallAction::Install,
    }
}

/// Whether a release found at launch is installed before the app opens:
/// only with the user's consent (check + auto-install), a trusted key, and
/// an install that can replace its own binary (no installer, no admin).
pub(crate) fn launch_update_wanted(
    settings: UpdateSettings,
    can_install: bool,
    plan: &UpdatePlan,
) -> bool {
    settings.check
        && settings.auto_install
        && can_install
        && matches!(plan, UpdatePlan::ReplaceBinary { .. })
}

/// What the launch screen shows while an update found at start installs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StartupPhase {
    /// No launch update: the app runs normally.
    None,
    Downloading {
        version: String,
        done: u64,
        total: u64,
    },
    /// Installed: start the new binary and exit.
    Relaunch,
    /// Give up and open this version.
    Failed(String),
}

pub(crate) fn startup_phase(
    startup: bool,
    state: &UpdateState,
    (done, total): (u64, u64),
) -> StartupPhase {
    if !startup {
        return StartupPhase::None;
    }
    match state {
        UpdateState::Downloading { version } => StartupPhase::Downloading {
            version: version.clone(),
            done,
            total,
        },
        UpdateState::ReadyToRestart { .. } => StartupPhase::Relaunch,
        UpdateState::Failed(message) => StartupPhase::Failed(message.clone()),
        other => StartupPhase::Failed(format!("unexpected update state {other:?}")),
    }
}

/// A newer release found by [`launch_check`], for the first window.
struct LaunchUpdate {
    release: Release,
    plan: UpdatePlan,
}

static LAUNCH_UPDATE: Mutex<Option<LaunchUpdate>> = Mutex::new(None);

/// Before the window opens: is there a release to install right now?
/// Bounded by [`LAUNCH_CHECK_TIMEOUT`]; offline or slow networks just open
/// the app. Installers and packages are left to the background worker.
pub fn launch_check(settings: UpdateSettings) -> bool {
    if !settings.check || !settings.auto_install {
        return false;
    }
    let Some(exe) = std::env::current_exe().ok() else {
        return false;
    };
    let kind = terminus_update::detect_install(
        &exe,
        terminus_update::Os::current(),
        &terminus_update::FsProbe,
    );
    let plan = terminus_update::plan(kind);
    let Ok(client) = terminus_update::Client::official() else {
        return false;
    };
    if !launch_update_wanted(settings, client.can_install(), &plan) {
        return false;
    }
    match client
        .with_timeout(LAUNCH_CHECK_TIMEOUT)
        .check(CURRENT_VERSION)
    {
        Ok(Some(release)) => {
            tracing::info!(version = %release.version, "update found at launch");
            if let Ok(mut slot) = LAUNCH_UPDATE.lock() {
                *slot = Some(LaunchUpdate { release, plan });
            }
            true
        }
        Ok(None) => false,
        Err(err) => {
            tracing::info!(%err, "launch update check skipped");
            false
        }
    }
}

/// `[updates]` settings the worker honours.
#[derive(Debug, Clone, Copy)]
pub struct UpdateSettings {
    pub check: bool,
    pub auto_install: bool,
}

impl From<rio_backend::config::Updates> for UpdateSettings {
    fn from(updates: rio_backend::config::Updates) -> Self {
        // TERMINUS_NO_UPDATE_CHECK=1 turns automatic checks off (packagers,
        // CI, air-gapped machines); the palette still works.
        let disabled = std::env::var_os("TERMINUS_NO_UPDATE_CHECK")
            .is_some_and(|v| !v.is_empty() && v != "0");
        Self {
            check: updates.check && !disabled,
            auto_install: updates.auto_install,
        }
    }
}

enum Command {
    Check,
    Install,
    /// `[updates]` changed (Settings > Updates writes the config file).
    Settings(UpdateSettings),
}

/// UI-side handle to the update worker.
pub struct Updater {
    commands: Sender<Command>,
    events: Receiver<(UpdateState, bool)>,
    state: UpdateState,
    notice: Option<String>,
    /// Binary path captured at start: after an in-place swap,
    /// `current_exe()` can report the unlinked old file on Linux.
    exe: Option<PathBuf>,
    /// Runs in `run_exit_action` once the user has confirmed the quit.
    on_exit: Option<ExitAction>,
    /// This window is installing an update found at launch.
    startup: bool,
    progress: Arc<[AtomicU64; 2]>,
}

/// Work left for the moment the process exits, so a declined
/// “want to quit?” never leaves two copies of Terminus running.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ExitAction {
    Relaunch(PathBuf),
    RunInstaller(PathBuf, Installer),
}

pub(crate) fn exit_action_for(
    state: &UpdateState,
    exe: Option<&Path>,
) -> Option<ExitAction> {
    match state {
        UpdateState::ReadyToRestart { .. } => {
            exe.map(|e| ExitAction::Relaunch(e.to_path_buf()))
        }
        UpdateState::InstallerReady {
            path, installer, ..
        } => Some(ExitAction::RunInstaller(path.clone(), *installer)),
        _ => None,
    }
}

impl Updater {
    pub fn spawn(
        settings: UpdateSettings,
        wake: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> Self {
        let (command_tx, command_rx) = channel();
        let (event_tx, event_rx) = channel();
        let exe = std::env::current_exe().ok();
        let worker_exe = exe.clone();
        let launch = LAUNCH_UPDATE.lock().ok().and_then(|mut slot| slot.take());
        let startup_version = launch.as_ref().map(|l| l.release.version.to_string());
        let progress: Arc<[AtomicU64; 2]> =
            Arc::new([AtomicU64::new(0), AtomicU64::new(0)]);
        let worker_progress = Arc::clone(&progress);
        let spawned = std::thread::Builder::new()
            .name("terminus-updater".into())
            .spawn(move || {
                let install_first = launch.is_some();
                let mut worker = Worker {
                    settings,
                    exe: worker_exe,
                    events: event_tx,
                    wake,
                    found: launch.map(|l| (l.release, l.plan, true)),
                    install_first,
                    progress: worker_progress,
                };
                worker.run(command_rx);
            });
        if let Err(err) = spawned {
            tracing::warn!(%err, "could not start the update worker");
        }
        let startup = startup_version.is_some();
        Self {
            commands: command_tx,
            events: event_rx,
            state: match startup_version {
                Some(version) => UpdateState::Downloading { version },
                None => UpdateState::Idle,
            },
            notice: None,
            exe,
            on_exit: None,
            startup,
            progress,
        }
    }

    /// Where the launch-time update stands ([`StartupPhase::None`] once it
    /// is over or when there was none).
    pub(crate) fn startup_phase(&self) -> StartupPhase {
        let progress = (
            self.progress[0].load(Ordering::Relaxed),
            self.progress[1].load(Ordering::Relaxed),
        );
        startup_phase(self.startup, &self.state, progress)
    }

    /// The launch update failed: carry on with this version.
    pub(crate) fn end_startup(&mut self) {
        self.startup = false;
    }

    /// Start the installed binary now (no quit confirmation: nothing has
    /// run in this window yet).
    pub(crate) fn relaunch_now(&mut self) -> Result<(), String> {
        self.arm_exit_action()?;
        self.run_exit_action();
        Ok(())
    }

    /// Check now (palette "Check for Updates").
    pub fn check_now(&self) {
        let _ = self.commands.send(Command::Check);
    }

    /// The `[updates]` config changed: the worker follows it live.
    pub fn apply_settings(&self, settings: UpdateSettings) {
        let _ = self.commands.send(Command::Settings(settings));
    }

    /// Install the found update (palette "Install Update").
    pub fn install(&self) {
        let _ = self.commands.send(Command::Install);
    }

    /// Apply worker events; true when the UI should redraw.
    pub fn pump(&mut self) -> bool {
        let mut changed = false;
        while let Ok((state, manual)) = self.events.try_recv() {
            if let Some(notice) = notice_for(&state, manual) {
                self.notice = Some(notice);
            }
            self.state = state;
            changed = true;
        }
        changed
    }

    pub fn state(&self) -> &UpdateState {
        &self.state
    }

    /// Notice to show once (sidebar band).
    pub fn take_notice(&mut self) -> Option<String> {
        self.notice.take()
    }

    /// Arm the relaunch/installer for the next confirmed quit.
    pub fn arm_exit_action(&mut self) -> Result<(), String> {
        let action =
            exit_action_for(&self.state, self.exe.as_deref()).ok_or_else(|| {
                "No installed update is waiting. Run “Check for Updates” first"
                    .to_string()
            })?;
        self.on_exit = Some(action);
        Ok(())
    }

    pub fn exit_action_armed(&self) -> bool {
        self.on_exit.is_some()
    }

    /// The user answered “no” to quitting: stay on this version.
    pub fn disarm_exit_action(&mut self) {
        self.on_exit = None;
    }

    /// Called right before the process exits.
    pub fn run_exit_action(&mut self) {
        let result = match self.on_exit.take() {
            None => return,
            Some(ExitAction::Relaunch(exe)) => std::process::Command::new(&exe)
                .args(std::env::args_os().skip(1))
                .spawn()
                .map(|_| ())
                .map_err(|e| format!("could not restart {}: {e}", exe.display())),
            Some(ExitAction::RunInstaller(path, installer)) => {
                terminus_update::launch_installer(&path, installer)
                    .map_err(|e| e.to_string())
            }
        };
        if let Err(err) = result {
            tracing::error!("update: {err}");
        }
    }
}

struct Worker {
    settings: UpdateSettings,
    exe: Option<PathBuf>,
    events: Sender<(UpdateState, bool)>,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
    found: Option<(Release, UpdatePlan, bool)>,
    /// Install `found` right away (an update found at launch).
    install_first: bool,
    /// Bytes received / expected by the running download.
    progress: Arc<[AtomicU64; 2]>,
}

impl Worker {
    fn run(&mut self, commands: Receiver<Command>) {
        if let Some(exe) = &self.exe {
            terminus_update::cleanup_after_update(exe);
        }
        let mut next_check = self.settings.check.then(|| Instant::now() + STARTUP_DELAY);
        if self.install_first {
            self.install_first = false;
            self.install();
            next_check = self.settings.check.then(|| Instant::now() + CHECK_INTERVAL);
        }
        loop {
            let wait = next_check
                .map(|at| at.saturating_duration_since(Instant::now()))
                .unwrap_or(CHECK_INTERVAL);
            match commands.recv_timeout(wait) {
                Ok(Command::Check) => self.check(true),
                Ok(Command::Install) => self.install(),
                Ok(Command::Settings(settings)) => {
                    self.apply_settings(settings, &mut next_check, Instant::now())
                }
                Err(RecvTimeoutError::Timeout) => {
                    if next_check.is_some_and(|at| Instant::now() >= at) {
                        self.check(false);
                        next_check = Some(Instant::now() + CHECK_INTERVAL);
                    }
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    }

    /// Follow a settings change: auto-install decides what the next found
    /// update does; turning checks off drops the scheduled one, turning
    /// them on schedules it again.
    fn apply_settings(
        &mut self,
        settings: UpdateSettings,
        next_check: &mut Option<Instant>,
        now: Instant,
    ) {
        if !settings.check {
            *next_check = None;
        } else if next_check.is_none() {
            *next_check = Some(now + CHECK_INTERVAL);
        }
        self.settings = settings;
    }

    fn emit(&self, state: UpdateState, manual: bool) {
        if self.events.send((state, manual)).is_ok() {
            if let Some(wake) = &self.wake {
                wake();
            }
        }
    }

    fn client(&self) -> Result<terminus_update::Client, String> {
        let progress = Arc::clone(&self.progress);
        let wake = self.wake.clone();
        let last_percent = Arc::new(AtomicU64::new(u64::MAX));
        terminus_update::Client::official()
            .map(|client| {
                client.with_progress(Arc::new(move |done, total| {
                    progress[0].store(done, Ordering::Relaxed);
                    progress[1].store(total, Ordering::Relaxed);
                    // Repaint once per percent, not per chunk.
                    let percent = (done * 100).checked_div(total).unwrap_or(0);
                    if last_percent.swap(percent, Ordering::Relaxed) != percent {
                        if let Some(wake) = &wake {
                            wake();
                        }
                    }
                }))
            })
            .map_err(|e| e.to_string())
    }

    fn check(&mut self, manual: bool) {
        // Never re-check over an update that is already staged or running.
        if matches!(
            self.found.as_ref().map(|f| &f.1),
            Some(UpdatePlan::ReplaceBinary { .. })
        ) && !manual
        {
            return;
        }
        if manual {
            self.emit(UpdateState::Checking, manual);
        }
        let client = match self.client() {
            Ok(client) => client,
            Err(err) => return self.emit(UpdateState::Failed(err), manual),
        };
        let release = match client.check(CURRENT_VERSION) {
            Ok(Some(release)) => release,
            Ok(None) => return self.emit(UpdateState::UpToDate, manual),
            Err(err) => {
                // Automatic checks fail quietly (offline laptops); say so
                // only when the user asked.
                tracing::info!(%err, "update check failed");
                if manual {
                    self.emit(UpdateState::Failed(err.to_string()), manual);
                }
                return;
            }
        };
        let kind = match &self.exe {
            Some(exe) => terminus_update::detect_install(
                exe,
                terminus_update::Os::current(),
                &terminus_update::FsProbe,
            ),
            None => terminus_update::InstallKind::Unknown {
                reason: "cannot locate the Terminus executable".into(),
            },
        };
        let plan = terminus_update::plan(kind);
        let can_install = client.can_install();
        let step = next_step(&plan, can_install, self.settings.auto_install);
        self.found = Some((release.clone(), plan.clone(), can_install));
        match step {
            NextStep::AutoInstall => self.install(),
            NextStep::Offer | NextStep::Inform => self.emit(
                UpdateState::Available {
                    version: release.version.to_string(),
                    page_url: release.page_url.clone(),
                    plan,
                    can_install,
                },
                manual,
            ),
        }
    }

    fn install(&mut self) {
        if self.found.is_none() {
            self.check(true);
        }
        let Some((release, plan, can_install)) = self.found.clone() else {
            return;
        };
        let version = release.version.to_string();
        let client = match self.client() {
            Ok(client) => client,
            Err(err) => return self.emit(UpdateState::Failed(err), true),
        };
        if !can_install {
            return self.emit(
                UpdateState::Available {
                    version,
                    page_url: release.page_url.clone(),
                    plan,
                    can_install,
                },
                true,
            );
        }
        match plan.clone() {
            UpdatePlan::ReplaceBinary { exe, asset, entry } => {
                self.emit(
                    UpdateState::Downloading {
                        version: version.clone(),
                    },
                    true,
                );
                let Some(dir) = exe.parent() else {
                    return self
                        .emit(UpdateState::Failed("no install folder".into()), true);
                };
                let result = client
                    .stage_binary(&release, &asset, &entry, dir)
                    .and_then(|staged| terminus_update::install(&staged, &exe));
                match result {
                    Ok(()) => self.emit(UpdateState::ReadyToRestart { version }, true),
                    Err(err) => self.emit(UpdateState::Failed(err.to_string()), true),
                }
            }
            UpdatePlan::RunInstaller { asset, installer } => {
                self.emit(
                    UpdateState::Downloading {
                        version: version.clone(),
                    },
                    true,
                );
                let dir = std::env::temp_dir().join("terminus-update");
                let result = std::fs::create_dir_all(&dir)
                    .map_err(|e| e.to_string())
                    .and_then(|()| {
                        client
                            .download_verified(&release, &asset, &dir)
                            .map_err(|e| e.to_string())
                    });
                match result {
                    Ok(path) => self.emit(
                        UpdateState::InstallerReady {
                            version,
                            path,
                            installer,
                        },
                        true,
                    ),
                    Err(err) => self.emit(UpdateState::Failed(err), true),
                }
            }
            UpdatePlan::PackageFile { asset, command } => {
                self.emit(
                    UpdateState::Downloading {
                        version: version.clone(),
                    },
                    true,
                );
                let dir = dirs::download_dir().unwrap_or_else(std::env::temp_dir);
                match client.download_verified(&release, &asset, &dir) {
                    Ok(path) => self.emit(
                        UpdateState::PackageReady {
                            version,
                            command: command.replace("{}", &path.display().to_string()),
                        },
                        true,
                    ),
                    Err(err) => self.emit(UpdateState::Failed(err.to_string()), true),
                }
            }
            UpdatePlan::Manual { .. } => self.emit(
                UpdateState::Available {
                    version,
                    page_url: release.page_url.clone(),
                    plan,
                    can_install,
                },
                true,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tarball() -> UpdatePlan {
        UpdatePlan::ReplaceBinary {
            exe: PathBuf::from("/opt/terminus/terminus"),
            asset: terminus_update::LINUX_TARBALL.into(),
            entry: terminus_update::LINUX_BINARY.into(),
        }
    }

    #[test]
    fn launch_updates_need_consent_a_key_and_a_self_replacing_install() {
        let on = UpdateSettings {
            check: true,
            auto_install: true,
        };
        assert!(launch_update_wanted(on, true, &tarball()));
        assert!(!launch_update_wanted(on, false, &tarball()), "unsigned: no");
        let off = UpdateSettings {
            check: true,
            auto_install: false,
        };
        assert!(!launch_update_wanted(off, true, &tarball()));
        let no_check = UpdateSettings {
            check: false,
            auto_install: true,
        };
        assert!(!launch_update_wanted(no_check, true, &tarball()));
        let installer = UpdatePlan::RunInstaller {
            asset: terminus_update::WINDOWS_MSI.into(),
            installer: Installer::Msi,
        };
        assert!(
            !launch_update_wanted(on, true, &installer),
            "needs admin: later"
        );
    }

    #[test]
    fn the_launch_screen_follows_the_worker() {
        let downloading = UpdateState::Downloading {
            version: "0.6.1".into(),
        };
        assert_eq!(
            startup_phase(true, &downloading, (5, 10)),
            StartupPhase::Downloading {
                version: "0.6.1".into(),
                done: 5,
                total: 10
            }
        );
        let ready = UpdateState::ReadyToRestart {
            version: "0.6.1".into(),
        };
        assert_eq!(
            startup_phase(true, &ready, (10, 10)),
            StartupPhase::Relaunch
        );
        assert_eq!(
            startup_phase(true, &UpdateState::Failed("offline".into()), (0, 0)),
            StartupPhase::Failed("offline".into())
        );
        // Not a launch update: the app runs normally.
        assert_eq!(
            startup_phase(false, &downloading, (5, 10)),
            StartupPhase::None
        );
    }

    #[test]
    fn settings_changed_in_the_app_reach_the_worker() {
        let (events, _rx) = channel();
        let mut w = Worker {
            settings: UpdateSettings {
                check: true,
                auto_install: true,
            },
            exe: None,
            events,
            wake: None,
            found: None,
            install_first: false,
            progress: Arc::new([AtomicU64::new(0), AtomicU64::new(0)]),
        };
        let now = Instant::now();
        let mut next = Some(now);
        // Settings > Updates: both switches off.
        w.apply_settings(
            UpdateSettings {
                check: false,
                auto_install: false,
            },
            &mut next,
            now,
        );
        assert!(!w.settings.auto_install, "a found update must not install");
        assert_eq!(next, None, "no more background checks");
        // Checks back on: the next one is scheduled again.
        w.apply_settings(
            UpdateSettings {
                check: true,
                auto_install: false,
            },
            &mut next,
            now,
        );
        assert_eq!(next, Some(now + CHECK_INTERVAL));
    }

    #[test]
    fn only_portable_installs_update_unattended() {
        assert_eq!(next_step(&tarball(), true, true), NextStep::AutoInstall);
        assert_eq!(next_step(&tarball(), true, false), NextStep::Offer);
        let installer = UpdatePlan::RunInstaller {
            asset: terminus_update::WINDOWS_MSI.into(),
            installer: Installer::Msi,
        };
        assert_eq!(next_step(&installer, true, true), NextStep::Offer);
        let package = UpdatePlan::PackageFile {
            asset: terminus_update::DEB_ASSET.into(),
            command: "sudo apt install {}".into(),
        };
        assert_eq!(next_step(&package, true, true), NextStep::Offer);
    }

    #[test]
    fn without_a_signing_key_or_a_way_to_install_we_only_inform() {
        assert_eq!(next_step(&tarball(), false, true), NextStep::Inform);
        let manual = UpdatePlan::Manual {
            hint: "Installed with Nix".into(),
        };
        assert_eq!(next_step(&manual, true, true), NextStep::Inform);
    }

    #[test]
    fn notices_tell_the_user_what_to_do_next() {
        assert_eq!(notice_for(&UpdateState::UpToDate, false), None);
        assert!(notice_for(&UpdateState::UpToDate, true)
            .unwrap()
            .contains("up to date"));
        let offered = UpdateState::Available {
            version: "0.6.0".into(),
            page_url: "https://example/r".into(),
            plan: tarball(),
            can_install: true,
        };
        assert!(notice_for(&offered, false)
            .unwrap()
            .contains("Install Update"));
        let unsigned = UpdateState::Available {
            version: "0.6.0".into(),
            page_url: "https://example/r".into(),
            plan: tarball(),
            can_install: false,
        };
        assert!(notice_for(&unsigned, false)
            .unwrap()
            .contains("open the release page"));
        let ready = UpdateState::ReadyToRestart {
            version: "0.6.0".into(),
        };
        assert!(notice_for(&ready, false)
            .unwrap()
            .contains("Restart to Update"));
        let package = UpdateState::PackageReady {
            version: "0.6.0".into(),
            command: "sudo apt install /tmp/t.deb".into(),
        };
        assert!(notice_for(&package, false)
            .unwrap()
            .contains("copy the install command"));
    }

    #[test]
    fn notices_fit_the_sidebar_card() {
        let cap = terminus_ui::sidebar::NOTICE_LINE_CHARS
            * terminus_ui::sidebar::NOTICE_MAX_LINES;
        let v = "10.20.30".to_string();
        let states = [
            UpdateState::Available {
                version: v.clone(),
                page_url: "https://github.com/delikesance/terminus/releases/tag/v10.20.30"
                    .into(),
                plan: tarball(),
                can_install: false,
            },
            UpdateState::Available {
                version: v.clone(),
                page_url: String::new(),
                plan: terminus_update::plan(terminus_update::InstallKind::Nix),
                can_install: true,
            },
            UpdateState::Available {
                version: v.clone(),
                page_url: String::new(),
                plan: UpdatePlan::RunInstaller {
                    asset: terminus_update::WINDOWS_MSI.into(),
                    installer: Installer::Msi,
                },
                can_install: true,
            },
            UpdateState::ReadyToRestart { version: v.clone() },
            UpdateState::PackageReady {
                version: v.clone(),
                command: "sudo dnf install /home/someone/Downloads/terminus-10.20.30-1.x86_64.rpm"
                    .into(),
            },
        ];
        for state in states {
            let notice = notice_for(&state, false).unwrap();
            assert!(notice.chars().count() <= cap, "{notice}");
        }
    }

    #[test]
    fn exit_actions_only_follow_a_finished_update() {
        let exe = Some(PathBuf::from("/opt/terminus/terminus"));
        let ready = UpdateState::ReadyToRestart {
            version: "0.6.0".into(),
        };
        assert_eq!(
            exit_action_for(&ready, exe.as_deref()),
            Some(ExitAction::Relaunch(PathBuf::from(
                "/opt/terminus/terminus"
            )))
        );
        assert_eq!(exit_action_for(&ready, None), None, "nothing to relaunch");
        let installer = UpdateState::InstallerReady {
            version: "0.6.0".into(),
            path: PathBuf::from("/tmp/setup.exe"),
            installer: Installer::Nsis,
        };
        assert_eq!(
            exit_action_for(&installer, exe.as_deref()),
            Some(ExitAction::RunInstaller(
                PathBuf::from("/tmp/setup.exe"),
                Installer::Nsis
            ))
        );
        assert_eq!(
            exit_action_for(&UpdateState::UpToDate, exe.as_deref()),
            None
        );
    }

    #[test]
    fn a_declined_quit_disarms_the_exit_action() {
        let mut updater = Updater::spawn(
            UpdateSettings {
                check: false,
                auto_install: false,
            },
            None,
        );
        updater.exe = Some(PathBuf::from("/opt/terminus/terminus"));
        assert!(updater.arm_exit_action().is_err(), "nothing installed yet");
        updater.state = UpdateState::ReadyToRestart {
            version: "0.6.0".into(),
        };
        assert!(updater.arm_exit_action().is_ok());
        assert!(updater.exit_action_armed());
        updater.disarm_exit_action();
        assert!(!updater.exit_action_armed());
    }

    #[test]
    fn install_update_does_the_next_useful_thing() {
        let page = "https://example/r".to_string();
        let package = UpdateState::PackageReady {
            version: "0.6.0".into(),
            command: "sudo apt install /tmp/t.deb".into(),
        };
        assert_eq!(
            install_action(&package),
            InstallAction::CopyCommand("sudo apt install /tmp/t.deb".into())
        );
        let unsigned = UpdateState::Available {
            version: "0.6.0".into(),
            page_url: page.clone(),
            plan: tarball(),
            can_install: false,
        };
        assert_eq!(
            install_action(&unsigned),
            InstallAction::OpenPage(page.clone())
        );
        let nix = UpdateState::Available {
            version: "0.6.0".into(),
            page_url: page.clone(),
            plan: terminus_update::plan(terminus_update::InstallKind::Nix),
            can_install: true,
        };
        assert_eq!(install_action(&nix), InstallAction::OpenPage(page.clone()));
        let offered = UpdateState::Available {
            version: "0.6.0".into(),
            page_url: page.clone(),
            plan: tarball(),
            can_install: true,
        };
        assert_eq!(install_action(&offered), InstallAction::Install);
        assert_eq!(
            install_action(&UpdateState::Failed("x".into())),
            InstallAction::Install,
            "retry after a failure"
        );
    }
}
