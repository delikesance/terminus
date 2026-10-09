//! Pasting a clipboard image into a terminal tab.
//!
//! A terminal only carries text, so an image paste becomes the path of a PNG
//! holding it: CLI tools that take image paths (Claude Code, `cat`-to-viewer
//! scripts…) then read the file. Where the file lives depends on the tab:
//! the local temp dir for a local shell, the same file seen through
//! `/mnt/<drive>` for a WSL distro started from Windows, and `/tmp` on the
//! remote for an SSH host (uploaded with the tab's own `ssh` command line).

use super::Screen;
use crate::crosswords::Mode;
use crate::event::Msg;
use crate::hosts;
use crate::ssh_secrets::{private_temp_dir, write_private_file};
use rio_backend::clipboard::{Clipboard, ClipboardType};
use rio_window::keyboard::{Key, ModifiersState};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

/// Local pasted images older than this are deleted on the next paste.
const KEEP_PASTES_FOR: Duration = Duration::from_secs(24 * 60 * 60);

/// Prefix shared by every pasted image file, local or remote.
const PASTE_PREFIX: &str = "terminus-paste-";

/// An image read from the system clipboard, as straight RGBA rows.
pub(super) struct ClipboardImage {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

/// Where the pasted file has to be for the tab's program to read it.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum PasteTarget {
    Local,
    Wsl,
    Ssh(String),
}

impl Screen<'_> {
    /// The clipboard paste binding (Ctrl+Shift+V, Cmd+V, palette).
    ///
    /// Text wins, so a normal copy behaves exactly as before. Only when the
    /// clipboard has no text and holds an image is the image saved and its
    /// path pasted instead.
    pub fn paste_clipboard_or_image(&mut self, clipboard: &mut Clipboard) {
        let content = clipboard.get(ClipboardType::Clipboard);
        if !self.paste_image_unless_text(&content) {
            self.paste_from_clipboard(&content);
        }
    }

    /// Bare Ctrl+V: the key CLI tools (Claude Code…) listen to for an image
    /// paste, but they read the clipboard of the machine they run on, which
    /// over SSH is not ours. Returns whether the key was consumed.
    pub fn paste_clipboard_image(&mut self, clipboard: &mut Clipboard) -> bool {
        let content = clipboard.get(ClipboardType::Clipboard);
        self.paste_image_unless_text(&content)
    }

    fn paste_image_unless_text(&mut self, clipboard_text: &str) -> bool {
        if !is_empty_or_image_markup(clipboard_text)
            || self.view_takes_keys()
            || self.search_active()
        {
            return false;
        }
        let Some(image) = read_clipboard_image() else {
            return false;
        };
        self.paste_image(image);
        true
    }

    fn paste_target(&self) -> PasteTarget {
        let Some(id) = self.context_manager.current().host_id.as_deref() else {
            return PasteTarget::Local;
        };
        if id == hosts::LOCAL_ID {
            PasteTarget::Local
        } else if self.host_store.platform().distro_named(id).is_some() {
            PasteTarget::Wsl
        } else if self.host_store.hosts().iter().any(|host| host.id == id) {
            PasteTarget::Ssh(id.to_string())
        } else {
            PasteTarget::Local
        }
    }

    fn paste_image(&mut self, image: ClipboardImage) {
        let png = match encode_png(&image) {
            Ok(png) => png,
            Err(err) => {
                self.report_paste_failure(&format!("Image paste failed: {err}"));
                return;
            }
        };
        let name = paste_file_name(chrono::Local::now());
        match self.paste_target() {
            PasteTarget::Ssh(id) => {
                if let Err(err) = self.upload_pasted_image(&id, &name, png) {
                    self.report_paste_failure(&format!("Image paste failed: {err}"));
                }
            }
            target => match save_local_paste(&png, &name) {
                Ok(path) => {
                    let text = local_path_text(&path, target == PasteTarget::Wsl);
                    self.paste(&text, true);
                }
                Err(err) => {
                    self.report_paste_failure(&format!("Image paste failed: {err}"))
                }
            },
        }
    }

    fn report_paste_failure(&mut self, message: &str) {
        tracing::warn!("{message}");
        self.chrome.panel.error = Some(message.to_string());
    }

    /// Copy `png` to `/tmp/<name>` on the tab's host, then paste that path
    /// into the tab. Runs on its own thread: the UI must not wait on the
    /// network, and the tab's input channel is all it needs afterwards.
    fn upload_pasted_image(
        &mut self,
        id: &str,
        name: &str,
        png: Vec<u8>,
    ) -> Result<(), String> {
        let (shell, env) = self.plain_shell_for_row(id)?;
        let shell = shell.ok_or("not an SSH host")?;
        let program = shell.program.unwrap_or_else(|| "ssh".to_string());
        let remote_path = remote_paste_path(name);
        let args = upload_args(shell.args, &remote_path);
        let env = env.unwrap_or_default();
        let bracketed = self.get_mode().contains(Mode::BRACKETED_PASTE);
        let input = self.context_manager.current().messenger.channel.clone();
        let report = self.paste_errors.reporter();

        std::thread::Builder::new()
            .name("image-paste-upload".into())
            .spawn(move || match run_upload(&program, &args, &env, &png) {
                Ok(()) => {
                    let bytes =
                        super::clipboard::paste_bytes(&remote_path, true, bracketed);
                    let _ = input.send(Msg::Input(bytes.into()));
                }
                Err(err) => report(format!("Image upload failed: {err}")),
            })
            .map(|_| ())
            .map_err(|err| format!("could not start the upload: {err}"))
    }
}

/// The clipboard's image, if it holds one. `arboard` is opened per paste:
/// it is only read here, and copypasta keeps owning the text clipboard.
fn read_clipboard_image() -> Option<ClipboardImage> {
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|err| tracing::debug!("image clipboard unavailable: {err}"))
        .ok()?;
    let image = clipboard.get_image().ok()?;
    Some(ClipboardImage {
        width: image.width,
        height: image.height,
        rgba: image.bytes.into_owned(),
    })
}

pub(super) fn encode_png(image: &ClipboardImage) -> Result<Vec<u8>, String> {
    let width = u32::try_from(image.width).map_err(|_| "image too wide")?;
    let height = u32::try_from(image.height).map_err(|_| "image too tall")?;
    let buffer = image_rs::RgbaImage::from_raw(width, height, image.rgba.clone())
        .ok_or("clipboard image size does not match its pixels")?;
    let mut png = Vec::new();
    buffer
        .write_to(
            &mut std::io::Cursor::new(&mut png),
            image_rs::ImageFormat::Png,
        )
        .map_err(|err| format!("could not encode the image: {err}"))?;
    Ok(png)
}

/// `terminus-paste-20261008-200600-123.png`: sortable, and unique enough
/// for pastes made by hand.
pub(super) fn paste_file_name<Tz: chrono::TimeZone>(now: chrono::DateTime<Tz>) -> String
where
    Tz::Offset: std::fmt::Display,
{
    format!("{PASTE_PREFIX}{}.png", now.format("%Y%m%d-%H%M%S-%3f"))
}

fn save_local_paste(png: &[u8], name: &str) -> Result<PathBuf, String> {
    let dir = private_temp_dir("paste")?;
    sweep_old_pastes(&dir, KEEP_PASTES_FOR);
    let path = dir.join(name);
    write_private_file(&path, png)?;
    Ok(path)
}

/// Delete pasted images in `dir` last written more than `max_age` ago.
pub(super) fn sweep_old_pastes(dir: &Path, max_age: Duration) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let is_paste = entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with(PASTE_PREFIX));
        let expired = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age > max_age);
        if is_paste && expired {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// The text typed into a local or WSL tab for the saved file at `path`.
fn local_path_text(path: &Path, wsl: bool) -> String {
    let path = path.to_string_lossy();
    if cfg!(windows) {
        if wsl {
            if let Some(unix) = wsl_mount_path(&path) {
                return quote_for_shell(&unix, false);
            }
        }
        return quote_for_shell(&path, true);
    }
    quote_for_shell(&path, false)
}

/// Where the upload thread reports a failure: the UI drains it into the
/// panel's error line, since a silent failure looks like "nothing pasted".
pub struct PasteErrors {
    sender: Sender<String>,
    receiver: Receiver<String>,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl PasteErrors {
    pub fn new(wake: Option<Arc<dyn Fn() + Send + Sync>>) -> Self {
        let (sender, receiver) = channel();
        Self {
            sender,
            receiver,
            wake,
        }
    }

    fn reporter(&self) -> impl Fn(String) + Send + 'static {
        let sender = self.sender.clone();
        let wake = self.wake.clone();
        move |message| {
            let _ = sender.send(message);
            if let Some(wake) = &wake {
                wake();
            }
        }
    }

    pub fn take(&self) -> Option<String> {
        self.receiver.try_iter().last()
    }
}

/// "Copy image" in a browser or chat app puts the image's `<img>` markup on
/// the text clipboard next to the bitmap; that text is no real text.
fn is_empty_or_image_markup(text: &str) -> bool {
    let text = text.trim();
    text.is_empty() || (text.starts_with('<') && text.contains("<img"))
}

/// Bare Ctrl+V, the chord CLI tools listen to for an image paste.
pub(super) fn is_image_paste_chord(key: &Key, mods: ModifiersState) -> bool {
    mods == ModifiersState::CONTROL
        && matches!(key, Key::Character(c) if c.eq_ignore_ascii_case("v"))
}

/// Quote `path` for the tab's shell only when it needs it (a temp dir under
/// a user name with a space). POSIX shells get single quotes, Windows ones
/// double quotes, which `cmd`, PowerShell and Claude Code all strip.
pub(super) fn quote_for_shell(path: &str, windows: bool) -> String {
    let plain = |c: char| c.is_ascii_alphanumeric() || "/\\:._-+~,@%".contains(c);
    if path.chars().all(plain) {
        return path.to_string();
    }
    if windows {
        format!("\"{path}\"")
    } else {
        format!("'{}'", path.replace('\'', r"'\''"))
    }
}

/// `C:\Users\me\x.png` → `/mnt/c/Users/me/x.png`: the same file as a WSL
/// distro sees it through its default automount.
pub(super) fn wsl_mount_path(windows_path: &str) -> Option<String> {
    let mut chars = windows_path.chars();
    let drive = chars.next().filter(|c| c.is_ascii_alphabetic())?;
    if chars.next() != Some(':') {
        return None;
    }
    let rest = chars.as_str().replace('\\', "/");
    if !rest.starts_with('/') {
        return None;
    }
    Some(format!("/mnt/{}{rest}", drive.to_ascii_lowercase()))
}

pub(super) fn remote_paste_path(name: &str) -> String {
    format!("/tmp/{name}")
}

/// The tab's own `ssh` arguments turned into a one-shot upload: no pty
/// (stdin is the file), a bounded connect, and a remote command writing
/// stdin to `remote_path` readable by the user only. The host is the last
/// tab argument, so the command goes right after it.
pub(super) fn upload_args(tab_args: Vec<String>, remote_path: &str) -> Vec<String> {
    let mut args = vec![
        "-T".to_string(),
        "-o".to_string(),
        "ConnectTimeout=10".to_string(),
    ];
    args.extend(tab_args);
    args.push(format!(
        "umask 077 && cat > {}",
        quote_for_shell(remote_path, false)
    ));
    args
}

fn run_upload(
    program: &str,
    args: &[String],
    env: &[(String, String)],
    png: &[u8],
) -> Result<(), String> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    // Same cleanup as a tab: the askpass / key files go once ssh is done.
    let _secrets =
        crate::ssh_secrets::SecretFiles::new(crate::ssh_secrets::launch_secrets(
            args.iter().map(String::as_str),
            env.iter().map(|(k, v)| (k.as_str(), v.as_str())),
        ));
    let mut cmd = Command::new(program);
    cmd.args(args)
        .envs(env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.spawn().map_err(|err| format!("{program}: {err}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(png)
            .map_err(|err| format!("sending the image: {err}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|err| format!("{program}: {err}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upload_failure_reaches_the_ui_and_wakes_it() {
        let woken = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = woken.clone();
        let errors = PasteErrors::new(Some(Arc::new(move || {
            flag.store(true, std::sync::atomic::Ordering::SeqCst)
        })));
        assert_eq!(errors.take(), None);
        let report = errors.reporter();
        std::thread::spawn(move || report("boom".into()))
            .join()
            .unwrap();
        assert_eq!(errors.take().as_deref(), Some("boom"));
        assert!(woken.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn browser_image_copies_count_as_no_text() {
        let copied = r#"<meta http-equiv="content-type" content="text/html"><img src="https://x/y.png">"#;
        assert!(is_empty_or_image_markup(copied));
        assert!(is_empty_or_image_markup("  \n"));
        assert!(!is_empty_or_image_markup("hello <img> world"));
        assert!(!is_empty_or_image_markup("<div>real html</div>"));
    }

    #[test]
    fn only_bare_ctrl_v_is_the_image_paste_chord() {
        let v = Key::Character("v".into());
        assert!(is_image_paste_chord(&v, ModifiersState::CONTROL));
        assert!(is_image_paste_chord(
            &Key::Character("V".into()),
            ModifiersState::CONTROL
        ));
        assert!(!is_image_paste_chord(&v, ModifiersState::empty()));
        assert!(!is_image_paste_chord(
            &v,
            ModifiersState::CONTROL | ModifiersState::SHIFT
        ));
        assert!(!is_image_paste_chord(
            &Key::Character("c".into()),
            ModifiersState::CONTROL
        ));
    }

    #[test]
    fn encodes_clipboard_rgba_as_png() {
        let image = ClipboardImage {
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 255, 0, 0, 255, 128],
        };
        let png = encode_png(&image).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let decoded = image_rs::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!((decoded.width(), decoded.height()), (2, 1));
        assert_eq!(decoded.into_raw(), image.rgba);
    }

    #[test]
    fn rejects_rgba_that_does_not_match_the_size() {
        let image = ClipboardImage {
            width: 3,
            height: 3,
            rgba: vec![0; 4],
        };
        assert!(encode_png(&image).is_err());
    }

    #[test]
    fn file_name_is_timestamped_png() {
        use chrono::TimeZone;
        let now = chrono::Utc
            .with_ymd_and_hms(2026, 10, 8, 20, 6, 0)
            .unwrap()
            .checked_add_signed(chrono::TimeDelta::milliseconds(42))
            .unwrap();
        assert_eq!(
            paste_file_name(now),
            "terminus-paste-20261008-200600-042.png"
        );
    }

    #[test]
    fn quotes_only_paths_that_need_it() {
        assert_eq!(
            quote_for_shell("/run/user/1000/terminus-paste-1000/a.png", false),
            "/run/user/1000/terminus-paste-1000/a.png"
        );
        assert_eq!(
            quote_for_shell(r"C:\Users\Jo\AppData\Local\Temp\a.png", true),
            r"C:\Users\Jo\AppData\Local\Temp\a.png"
        );
        assert_eq!(
            quote_for_shell("/tmp/my dir/a.png", false),
            "'/tmp/my dir/a.png'"
        );
        assert_eq!(
            quote_for_shell("/tmp/it's/a.png", false),
            r"'/tmp/it'\''s/a.png'"
        );
        assert_eq!(
            quote_for_shell(r"C:\Users\Jo Doe\a.png", true),
            r#""C:\Users\Jo Doe\a.png""#
        );
    }

    #[test]
    fn maps_windows_paths_into_wsl_mounts() {
        assert_eq!(
            wsl_mount_path(r"C:\Users\Jo\AppData\Local\Temp\terminus-paste\a.png"),
            Some("/mnt/c/Users/Jo/AppData/Local/Temp/terminus-paste/a.png".into())
        );
        assert_eq!(wsl_mount_path(r"D:\x.png"), Some("/mnt/d/x.png".into()));
        assert_eq!(wsl_mount_path(r"\\server\share\x.png"), None);
        assert_eq!(wsl_mount_path("/tmp/x.png"), None);
        assert_eq!(wsl_mount_path("C:x.png"), None);
    }

    #[test]
    fn upload_reuses_tab_args_and_writes_stdin_to_tmp() {
        let tab = vec!["-p".to_string(), "2222".to_string(), "me@box".to_string()];
        let path = remote_paste_path("terminus-paste-1.png");
        assert_eq!(path, "/tmp/terminus-paste-1.png");
        assert_eq!(
            upload_args(tab, &path),
            vec![
                "-T",
                "-o",
                "ConnectTimeout=10",
                "-p",
                "2222",
                "me@box",
                "umask 077 && cat > /tmp/terminus-paste-1.png",
            ]
        );
    }

    #[test]
    fn sweep_removes_only_old_pastes() {
        let dir = std::env::temp_dir()
            .join(format!("terminus-paste-sweep-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let old = dir.join("terminus-paste-old.png");
        let other = dir.join("notes.txt");
        std::fs::write(&old, b"x").unwrap();
        std::fs::write(&other, b"x").unwrap();

        sweep_old_pastes(&dir, Duration::from_secs(3600));
        assert!(old.exists(), "a fresh paste is kept");

        sweep_old_pastes(&dir, Duration::ZERO);
        assert!(!old.exists(), "an expired paste is removed");
        assert!(other.exists(), "files that are not pastes are left alone");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
