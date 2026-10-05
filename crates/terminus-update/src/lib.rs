//! Signed self-update for Terminus.
//!
//! Flow: [`Client::check`] asks GitHub Releases for the latest release and
//! returns it when it is newer than the running build. [`detect_install`]
//! works out how this copy was installed and [`plan`] turns that into an
//! [`UpdatePlan`]:
//!
//! * portable Linux tarball → [`Client::stage_binary`] + [`install`] swap the
//!   binary in place;
//! * Windows NSIS / MSI → [`Client::download_verified`] the matching installer
//!   and [`launch_installer`] it (it upgrades in place, elevated via UAC);
//! * `.deb` / `.rpm` → download the verified package and hand the user the one
//!   command that installs it (system packages need root);
//! * Nix, development builds, unknown layouts → notify only.
//!
//! Every download is checked against `checksums.txt`, whose minisign signature
//! must verify with the key compiled into this binary ([`PUBLIC_KEY`]). A build
//! without a trusted key can still *check* and tell the user a release exists,
//! but never downloads anything to install.

use std::fmt;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use sha2::Digest;

/// GitHub Releases "latest" endpoint for Terminus.
pub const DEFAULT_ENDPOINT: &str =
    "https://api.github.com/repos/delikesance/terminus/releases/latest";

/// minisign public key trusted for release signatures (the `.pub` file's
/// contents or just its base64 line). Empty: updates are notify-only.
pub const PUBLIC_KEY: &str = include_str!("../update-public-key.txt");

/// Signed list of release artifact hashes (`sha256sum` format).
pub const CHECKSUMS_ASSET: &str = "checksums.txt";
/// minisign signature of [`CHECKSUMS_ASSET`].
pub const SIGNATURE_ASSET: &str = "checksums.txt.minisig";

/// Linux tarball published by `scripts/release.sh`.
pub const LINUX_TARBALL: &str = "terminus-linux-x86_64.tar.gz";
/// Binary name inside [`LINUX_TARBALL`] (`terminus/terminus`).
pub const LINUX_BINARY: &str = "terminus";
/// Windows zip the app updates itself from (`terminus.exe` inside).
pub const WINDOWS_ZIP: &str = "terminus-windows-x86_64.zip";
/// Entry name inside [`WINDOWS_ZIP`]. A label only: the update replaces the
/// running executable under whatever name it has on disk (`tmnx.exe` for
/// installs since 0.7.1, `terminus.exe` before), and published clients look
/// for this entry, so it must not change.
pub const WINDOWS_BINARY: &str = "terminus.exe";
/// NSIS setup wizard.
pub const WINDOWS_SETUP: &str = "terminus-setup-x86_64.exe";
/// MSI package.
pub const WINDOWS_MSI: &str = "terminus-x86_64.msi";
/// `.deb` asset name pattern.
pub const DEB_ASSET: &str = "terminus_{version}_amd64.deb";
/// `.rpm` asset name pattern.
pub const RPM_ASSET: &str = "terminus-{version}-1.x86_64.rpm";

/// Largest download the updater accepts.
const MAX_DOWNLOAD_BYTES: u64 = 512 * 1024 * 1024;
/// Largest checksums / signature / JSON body accepted.
const MAX_SMALL_BYTES: u64 = 4 * 1024 * 1024;
/// Prefix of staged files next to the running binary (swept on startup).
const STAGE_PREFIX: &str = ".terminus-update-";

/// Why an update step failed.
#[derive(Debug)]
pub enum UpdateError {
    /// Endpoint / asset URL is not HTTPS (plain HTTP only on loopback).
    InsecureUrl(String),
    /// Network or HTTP failure.
    Http(String),
    /// The release JSON could not be understood.
    BadRelease(String),
    /// A required release asset is missing.
    MissingAsset(String),
    /// No trusted public key is compiled in: updates are notify-only.
    NoTrustedKey,
    /// `checksums.txt` is not signed by the trusted key.
    BadSignature(String),
    /// The downloaded file does not match the signed hash.
    ChecksumMismatch { expected: String, actual: String },
    /// Archive / filesystem / process error.
    Io(String),
}

impl fmt::Display for UpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InsecureUrl(url) => write!(f, "refusing insecure update URL {url}"),
            Self::Http(msg) => write!(f, "update download failed: {msg}"),
            Self::BadRelease(msg) => write!(f, "unexpected release data: {msg}"),
            Self::MissingAsset(name) => write!(f, "release has no {name}"),
            Self::NoTrustedKey => write!(
                f,
                "this build has no update signing key; install updates manually"
            ),
            Self::BadSignature(msg) => write!(f, "release signature is not valid: {msg}"),
            Self::ChecksumMismatch { expected, actual } => write!(
                f,
                "downloaded file does not match the signed checksum (expected {expected}, got {actual})"
            ),
            Self::Io(msg) => write!(f, "update failed: {msg}"),
        }
    }
}

impl std::error::Error for UpdateError {}

impl From<std::io::Error> for UpdateError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, UpdateError>;

/// Operating system family (injectable for tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Linux,
    Windows,
    Macos,
    Other,
}

impl Os {
    pub fn current() -> Self {
        if cfg!(target_os = "linux") {
            Self::Linux
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::Macos
        } else {
            Self::Other
        }
    }
}

/// Filesystem questions [`detect_install`] asks (faked in tests).
pub trait Probe {
    fn exists(&self, path: &Path) -> bool;
    fn dir_writable(&self, dir: &Path) -> bool;
}

/// The real filesystem.
pub struct FsProbe;

impl Probe for FsProbe {
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn dir_writable(&self, dir: &Path) -> bool {
        let probe = dir.join(format!("{STAGE_PREFIX}probe-{}", uuid::Uuid::new_v4()));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&probe)
        {
            Ok(_) => {
                let _ = std::fs::remove_file(&probe);
                true
            }
            Err(_) => false,
        }
    }
}

/// How this copy of Terminus was installed (see `scripts/release.sh`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallKind {
    /// `terminus-linux-x86_64.tar.gz` unpacked into a folder we can write.
    LinuxTarball { exe: PathBuf },
    /// Windows copy in a folder we can write (per-user install or
    /// portable): replaces its own exe from [`WINDOWS_ZIP`].
    WindowsSelf { exe: PathBuf },
    /// `.deb` (dpkg owns the binary).
    Deb,
    /// `.rpm` (rpm owns the binary).
    Rpm,
    /// Nix store (flake / `terminus.nix`).
    Nix,
    /// NSIS setup wizard (`Uninstall.exe` next to the binary).
    WindowsNsis,
    /// MSI package (Program Files, no NSIS uninstaller).
    WindowsMsi,
    /// Running from a cargo `target/` directory.
    Dev,
    /// Anything else: say why, never touch it.
    Unknown { reason: String },
}

/// Which Windows installer to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Installer {
    Nsis,
    Msi,
}

/// What updating this installation means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdatePlan {
    /// Swap the running binary for `entry` from the verified `asset` archive.
    ReplaceBinary {
        exe: PathBuf,
        asset: String,
        entry: String,
    },
    /// Download the verified installer `asset` and run it.
    RunInstaller { asset: String, installer: Installer },
    /// Download the verified package `asset` (a `{version}` pattern);
    /// `command` installs it, `{}` standing for the downloaded path.
    PackageFile { asset: String, command: String },
    /// Nothing to download; `hint` says how to update.
    Manual { hint: String },
}

/// Work out how the binary at `exe` was installed.
pub fn detect_install(exe: &Path, os: Os, probe: &dyn Probe) -> InstallKind {
    let text = exe.to_string_lossy().replace('\\', "/");
    let parts: Vec<&str> = text.split('/').collect();
    let dev = parts.iter().enumerate().any(|(i, part)| {
        *part == "target"
            && parts[i + 1..]
                .iter()
                .take(2)
                .any(|p| *p == "debug" || *p == "release")
    });
    if dev {
        return InstallKind::Dev;
    }
    if text.starts_with("/nix/store/") {
        return InstallKind::Nix;
    }
    let dir = exe.parent().unwrap_or(Path::new("/"));
    match os {
        Os::Linux => {
            if text.starts_with("/usr/") {
                if probe.exists(Path::new("/var/lib/dpkg/info/terminus.list")) {
                    InstallKind::Deb
                } else if probe.exists(Path::new("/var/lib/rpm"))
                    || probe.exists(Path::new("/usr/lib/sysimage/rpm"))
                {
                    InstallKind::Rpm
                } else {
                    InstallKind::Unknown {
                        reason: format!("{text} is not managed by dpkg or rpm"),
                    }
                }
            } else if probe.dir_writable(dir) {
                InstallKind::LinuxTarball {
                    exe: exe.to_path_buf(),
                }
            } else {
                InstallKind::Unknown {
                    reason: format!("no write access to {}", dir.display()),
                }
            }
        }
        Os::Windows => {
            if probe.dir_writable(dir) {
                InstallKind::WindowsSelf {
                    exe: exe.to_path_buf(),
                }
            } else if probe.exists(&dir.join("Uninstall.exe")) {
                InstallKind::WindowsNsis
            } else if text.to_ascii_lowercase().contains("/program files/") {
                InstallKind::WindowsMsi
            } else {
                InstallKind::Unknown {
                    reason: "portable copy: download the installer from the release page"
                        .into(),
                }
            }
        }
        Os::Macos | Os::Other => InstallKind::Unknown {
            reason: "no Terminus release is published for this platform yet".into(),
        },
    }
}

/// The update procedure for an installation of `kind`.
pub fn plan(kind: InstallKind) -> UpdatePlan {
    match kind {
        InstallKind::LinuxTarball { exe } => UpdatePlan::ReplaceBinary {
            exe,
            asset: LINUX_TARBALL.into(),
            entry: LINUX_BINARY.into(),
        },
        InstallKind::WindowsSelf { exe } => UpdatePlan::ReplaceBinary {
            exe,
            asset: WINDOWS_ZIP.into(),
            entry: WINDOWS_BINARY.into(),
        },
        InstallKind::WindowsNsis => UpdatePlan::RunInstaller {
            asset: WINDOWS_SETUP.into(),
            installer: Installer::Nsis,
        },
        InstallKind::WindowsMsi => UpdatePlan::RunInstaller {
            asset: WINDOWS_MSI.into(),
            installer: Installer::Msi,
        },
        InstallKind::Deb => UpdatePlan::PackageFile {
            asset: DEB_ASSET.into(),
            command: "sudo apt install {}".into(),
        },
        InstallKind::Rpm => UpdatePlan::PackageFile {
            asset: RPM_ASSET.into(),
            command: "sudo dnf install {}".into(),
        },
        InstallKind::Nix => UpdatePlan::Manual {
            hint: "Installed with Nix: update your flake input (or terminus.nix) and rebuild"
                .into(),
        },
        InstallKind::Dev => UpdatePlan::Manual {
            hint: "Development build: pull and rebuild".into(),
        },
        InstallKind::Unknown { reason } => UpdatePlan::Manual { hint: reason },
    }
}

/// One downloadable file of a release.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    #[serde(rename = "browser_download_url")]
    pub url: String,
    #[serde(default)]
    pub size: u64,
}

/// A published release newer than the running build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: semver::Version,
    pub tag: String,
    /// Release page (notes, manual downloads).
    pub page_url: String,
    pub assets: Vec<Asset>,
}

impl Release {
    /// The asset named `name`.
    pub fn asset(&self, name: &str) -> Result<&Asset> {
        self.assets
            .iter()
            .find(|a| a.name == name)
            .ok_or_else(|| UpdateError::MissingAsset(name.to_string()))
    }

    /// Resolve `{version}` in an asset name pattern.
    pub fn asset_name(&self, pattern: &str) -> String {
        pattern.replace("{version}", &self.version.to_string())
    }
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

/// Parse `v1.2.3` / `1.2.3` into a version.
pub fn parse_version(raw: &str) -> Option<semver::Version> {
    semver::Version::parse(raw.trim().trim_start_matches('v')).ok()
}

/// Update client: endpoint + trusted key + HTTP agent.
/// Called with (bytes received, expected size) while a download runs.
pub type Progress = std::sync::Arc<dyn Fn(u64, u64) + Send + Sync>;

pub struct Client {
    endpoint: String,
    public_key: Option<minisign_verify::PublicKey>,
    agent: ureq::Agent,
    progress: Option<Progress>,
}

impl Client {
    /// Client for `endpoint` trusting `public_key` (minisign `.pub` contents
    /// or bare base64). `None` or blank: check-only.
    pub fn new(endpoint: &str, public_key: Option<&str>) -> Result<Self> {
        require_secure(endpoint)?;
        let public_key = match public_key.map(str::trim).filter(|k| !k.is_empty()) {
            None => None,
            Some(key) => Some(parse_public_key(key)?),
        };
        // OS trust store (works behind TLS-inspecting corporate proxies).
        // TLS is not what makes an update trustworthy: the minisign signature
        // over the checksums is, so a proxy can at worst block an update.
        let agent = build_agent(None);
        Ok(Self {
            endpoint: endpoint.to_string(),
            public_key,
            agent,
            progress: None,
        })
    }

    /// Give every request at most `limit` in total (the launch-time check
    /// must not hold up the app on a slow or silent network).
    pub fn with_timeout(mut self, limit: Duration) -> Self {
        self.agent = build_agent(Some(limit));
        self
    }

    /// Report download progress (for a progress bar).
    pub fn with_progress(mut self, progress: Progress) -> Self {
        self.progress = Some(progress);
        self
    }

    /// Client for the official releases and the compiled-in key.
    /// `TERMINUS_UPDATE_URL` overrides the endpoint (testing, mirrors); the
    /// signature check still applies, so a mirror cannot change what installs.
    pub fn official() -> Result<Self> {
        let endpoint = std::env::var("TERMINUS_UPDATE_URL")
            .ok()
            .filter(|u| !u.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_ENDPOINT.to_string());
        Self::new(&endpoint, Some(&trusted_key()))
    }

    /// Whether downloads can be verified (a trusted key is configured).
    pub fn can_install(&self) -> bool {
        self.public_key.is_some()
    }

    /// Latest published release when it is newer than `current_version`.
    /// Drafts and prereleases are ignored.
    pub fn check(&self, current_version: &str) -> Result<Option<Release>> {
        let current = parse_version(current_version).ok_or_else(|| {
            UpdateError::BadRelease(format!("bad running version {current_version:?}"))
        })?;
        let body = self.get_small(&self.endpoint, "application/vnd.github+json")?;
        let release: GithubRelease = serde_json::from_slice(&body)
            .map_err(|e| UpdateError::BadRelease(e.to_string()))?;
        if release.draft || release.prerelease {
            return Ok(None);
        }
        let Some(version) = parse_version(&release.tag_name) else {
            return Err(UpdateError::BadRelease(format!(
                "tag {:?} is not a version",
                release.tag_name
            )));
        };
        if version <= current || !version.pre.is_empty() {
            return Ok(None);
        }
        Ok(Some(Release {
            version,
            tag: release.tag_name,
            page_url: release.html_url,
            assets: release.assets,
        }))
    }

    /// Download release asset `name` (a `{version}` pattern is resolved),
    /// verify it against the signed checksums and store it as
    /// `dir/<asset name>`. Nothing is left in `dir` on failure.
    pub fn download_verified(
        &self,
        release: &Release,
        name: &str,
        dir: &Path,
    ) -> Result<PathBuf> {
        let name = release.asset_name(name);
        let expected = self.signed_hash(release, &name)?;
        let asset = release.asset(&name)?;
        let temp = dir.join(format!("{STAGE_PREFIX}{}.download", uuid::Uuid::new_v4()));
        let result = (|| {
            let actual = self.download_hashed(asset, &temp)?;
            if !actual.eq_ignore_ascii_case(&expected) {
                return Err(UpdateError::ChecksumMismatch { expected, actual });
            }
            let dest = dir.join(&name);
            std::fs::rename(&temp, &dest)?;
            Ok(dest)
        })();
        let _ = std::fs::remove_file(&temp);
        result
    }

    /// Download the verified archive `archive`, extract the file named
    /// `entry` into `stage_dir` (the running binary's folder, so [`install`]
    /// can rename it into place) and return the staged path.
    pub fn stage_binary(
        &self,
        release: &Release,
        archive: &str,
        entry: &str,
        stage_dir: &Path,
    ) -> Result<PathBuf> {
        let expected = self.signed_hash(release, archive)?;
        let asset = release.asset(archive)?;
        let archive_path =
            stage_dir.join(format!("{STAGE_PREFIX}{}.download", uuid::Uuid::new_v4()));
        let result = (|| {
            let actual = self.download_hashed(asset, &archive_path)?;
            if !actual.eq_ignore_ascii_case(&expected) {
                return Err(UpdateError::ChecksumMismatch { expected, actual });
            }
            let binary =
                stage_dir.join(format!("{STAGE_PREFIX}{}", uuid::Uuid::new_v4()));
            if let Err(err) = extract_entry(&archive_path, entry, &binary) {
                let _ = std::fs::remove_file(&binary);
                return Err(err);
            }
            Ok(binary)
        })();
        let _ = std::fs::remove_file(&archive_path);
        result
    }

    /// The SHA-256 the signed `checksums.txt` records for `name`.
    fn signed_hash(&self, release: &Release, name: &str) -> Result<String> {
        let public_key = self.public_key.as_ref().ok_or(UpdateError::NoTrustedKey)?;
        let checksums = release.asset(CHECKSUMS_ASSET)?;
        let signature = release.asset(SIGNATURE_ASSET)?;
        let sums = self.get_small(&checksums.url, "application/octet-stream")?;
        let sig = self.get_small(&signature.url, "application/octet-stream")?;
        let sig = String::from_utf8(sig)
            .map_err(|e| UpdateError::BadSignature(e.to_string()))?;
        let sig = minisign_verify::Signature::decode(&sig)
            .map_err(|e| UpdateError::BadSignature(e.to_string()))?;
        public_key
            .verify(&sums, &sig, false)
            .map_err(|e| UpdateError::BadSignature(e.to_string()))?;
        expected_hash(&String::from_utf8_lossy(&sums), name)
            .ok_or_else(|| UpdateError::MissingAsset(format!("checksum of {name}")))
    }

    fn get(&self, url: &str, accept: &str) -> Result<ureq::Body> {
        require_secure(url)?;
        let call = || self.agent.get(url).header("Accept", accept).call();
        let response = match call() {
            // A pooled connection the server (or a proxy) already closed
            // fails at once; these GETs are idempotent, so try a fresh one.
            Err(err)
                if !matches!(
                    err,
                    ureq::Error::StatusCode(_) | ureq::Error::Timeout(_)
                ) =>
            {
                tracing::debug!(%err, %url, "retrying on a new connection");
                call()
            }
            other => other,
        }
        .map_err(|e| UpdateError::Http(format!("{url}: {e}")))?;
        Ok(response.into_body())
    }

    fn get_small(&self, url: &str, accept: &str) -> Result<Vec<u8>> {
        let mut body = self.get(url, accept)?;
        let mut out = Vec::new();
        body.as_reader()
            .take(MAX_SMALL_BYTES + 1)
            .read_to_end(&mut out)
            .map_err(|e| UpdateError::Http(e.to_string()))?;
        if out.len() as u64 > MAX_SMALL_BYTES {
            return Err(UpdateError::Http(format!("{url}: response too large")));
        }
        Ok(out)
    }

    /// Stream `url` into `dest`, returning its SHA-256 (hex).
    fn download_hashed(&self, asset: &Asset, dest: &Path) -> Result<String> {
        let url = asset.url.as_str();
        let mut body = self.get(url, "application/octet-stream")?;
        let mut reader = body.as_reader().take(MAX_DOWNLOAD_BYTES + 1);
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dest)?;
        let mut hasher = sha2::Sha256::new();
        let mut buf = vec![0u8; 256 * 1024];
        let mut total = 0u64;
        loop {
            let n = reader
                .read(&mut buf)
                .map_err(|e| UpdateError::Http(e.to_string()))?;
            if n == 0 {
                break;
            }
            total += n as u64;
            if total > MAX_DOWNLOAD_BYTES {
                return Err(UpdateError::Http(format!("{url}: download too large")));
            }
            hasher.update(&buf[..n]);
            file.write_all(&buf[..n])?;
            if let Some(progress) = &self.progress {
                progress(total, asset.size.max(total));
            }
        }
        file.sync_all()?;
        Ok(hex::encode(hasher.finalize()))
    }
}

/// The trusted signing key: [`PUBLIC_KEY`]. Debug builds (never shipped)
/// also accept `TERMINUS_UPDATE_PUBKEY`, so the update flow can be exercised
/// end to end against a locally signed test release.
fn trusted_key() -> String {
    #[cfg(debug_assertions)]
    if let Some(key) = std::env::var("TERMINUS_UPDATE_PUBKEY")
        .ok()
        .filter(|k| !k.trim().is_empty())
    {
        return key;
    }
    PUBLIC_KEY.to_string()
}

fn parse_public_key(key: &str) -> Result<minisign_verify::PublicKey> {
    // Accept the whole `.pub` file (comment line + base64) or the base64 line.
    let decoded = if key.lines().count() > 1 {
        minisign_verify::PublicKey::decode(key)
    } else {
        minisign_verify::PublicKey::from_base64(key)
    };
    decoded.map_err(|e| UpdateError::BadSignature(format!("bad trusted key: {e}")))
}

/// HTTPS only, except plain HTTP to loopback (tests, local mirrors).
fn require_secure(url: &str) -> Result<()> {
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("https://") {
        return Ok(());
    }
    if let Some(rest) = lower.strip_prefix("http://") {
        let host = rest.split(['/', ':']).next().unwrap_or_default();
        if host == "127.0.0.1" || host == "localhost" {
            return Ok(());
        }
    }
    Err(UpdateError::InsecureUrl(url.to_string()))
}

/// The hash `sha256sum` recorded for `name`.
fn expected_hash(checksums: &str, name: &str) -> Option<String> {
    checksums.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let file = parts.next()?.trim_start_matches('*');
        (file == name && hash.len() == 64).then(|| hash.to_ascii_lowercase())
    })
}

/// Extract the file whose name is `entry_name` (at any depth) from a
/// verified `.tar.gz` into `dest` (mode 0755).
fn build_agent(total: Option<Duration>) -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder()
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    ureq::Agent::config_builder()
        .tls_config(tls)
        .timeout_global(total)
        .timeout_connect(Some(total.unwrap_or(Duration::from_secs(15))))
        .timeout_recv_body(Some(Duration::from_secs(60)))
        .user_agent(format!("terminus-updater/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .new_agent()
}

/// Copy the file named `entry_name` out of a zip archive into `dest`.
fn extract_zip_entry(archive: &Path, entry_name: &str, dest: &Path) -> Result<bool> {
    let file = std::fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|e| UpdateError::BadRelease(format!("bad zip: {e}")))?;
    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| UpdateError::BadRelease(format!("bad zip: {e}")))?;
        let name = entry.name().replace('\\', "/");
        if entry.is_file() && name.rsplit('/').next() == Some(entry_name) {
            let mut out = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(dest)?;
            std::io::copy(&mut entry, &mut out)?;
            out.sync_all()?;
            return Ok(true);
        }
    }
    Ok(false)
}

fn extract_entry(archive: &Path, entry_name: &str, dest: &Path) -> Result<()> {
    let mut magic = [0u8; 4];
    let is_zip = std::fs::File::open(archive)
        .and_then(|mut f| f.read_exact(&mut magic))
        .is_ok()
        && magic == *b"PK\x03\x04";
    if is_zip {
        if !extract_zip_entry(archive, entry_name, dest)? {
            return Err(UpdateError::MissingAsset(format!(
                "{entry_name} inside the archive"
            )));
        }
        return Ok(());
    }
    let file = std::fs::File::open(archive)?;
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
    let mut found = false;
    for entry in tar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_string_lossy().into_owned();
        if entry.header().entry_type().is_file()
            && path.rsplit('/').next() == Some(entry_name)
        {
            let mut out = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(dest)?;
            std::io::copy(&mut entry, &mut out)?;
            out.sync_all()?;
            found = true;
            break;
        }
    }
    if !found {
        return Err(UpdateError::MissingAsset(format!(
            "{entry_name} inside the archive"
        )));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

/// Swap the staged binary `new_binary` in for `exe`: one `rename` over the
/// running binary (atomic; the running process keeps its open inode).
/// The mode of the old binary is kept.
pub fn install(new_binary: &Path, exe: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        install_renaming_aside(new_binary, exe)
    }
    #[cfg(not(windows))]
    {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(exe)
                .map(|m| m.permissions().mode() & 0o7777)
                .unwrap_or(0o755)
                | 0o100;
            std::fs::set_permissions(new_binary, std::fs::Permissions::from_mode(mode))?;
        }
        std::fs::rename(new_binary, exe)?;
        Ok(())
    }
}

/// Windows swap: a running .exe cannot be replaced or deleted, but it can
/// be renamed. Move it aside (swept by [`cleanup_after_update`] on the next
/// start), then move the new binary into its place; put the old one back if
/// that fails.
pub fn install_renaming_aside(new_binary: &Path, exe: &Path) -> Result<()> {
    let dir = exe.parent().unwrap_or(Path::new("."));
    let aside = dir.join(format!("{STAGE_PREFIX}old-{}.exe", uuid::Uuid::new_v4()));
    std::fs::rename(exe, &aside)?;
    if let Err(err) = std::fs::rename(new_binary, exe) {
        let _ = std::fs::rename(&aside, exe);
        return Err(err.into());
    }
    Ok(())
}

/// Start a verified Windows installer; the caller should quit right after so
/// the installer can replace the running executable.
///
/// NSIS: opened through the shell so its manifest's UAC elevation applies.
/// MSI: `msiexec /i … /passive` (msiexec elevates per-machine installs).
pub fn launch_installer(path: &Path, installer: Installer) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut cmd = match installer {
            Installer::Nsis => {
                let mut c = std::process::Command::new("cmd");
                c.arg("/C").arg("start").arg("").arg(path);
                c
            }
            Installer::Msi => {
                let mut c = std::process::Command::new("msiexec");
                c.arg("/i").arg(path).arg("/passive");
                c
            }
        };
        cmd.creation_flags(CREATE_NO_WINDOW).spawn()?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (path, installer);
        Err(UpdateError::Io("installers only run on Windows".into()))
    }
}

/// Remove files a previous update attempt left next to `exe` (staged or
/// half-downloaded). Run once at startup.
pub fn cleanup_after_update(exe: &Path) {
    let Some(dir) = exe.parent() else { return };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with(STAGE_PREFIX)
        {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_semantically() {
        assert!(parse_version("v0.5.30").unwrap() > parse_version("0.5.29").unwrap());
        assert!(parse_version("v0.10.0").unwrap() > parse_version("0.9.9").unwrap());
        assert!(parse_version("nightly").is_none());
    }

    #[test]
    fn checksum_lines_are_matched_by_exact_name() {
        let sums = format!(
            "{}  terminus-linux-x86_64.tar.gz\n{}  terminus-linux-x86_64.tar.gz.bak\n",
            "a".repeat(64),
            "b".repeat(64)
        );
        assert_eq!(expected_hash(&sums, LINUX_TARBALL), Some("a".repeat(64)));
        assert_eq!(expected_hash(&sums, WINDOWS_MSI), None);
    }

    #[test]
    fn plain_http_only_on_loopback() {
        assert!(require_secure("https://api.github.com/x").is_ok());
        assert!(require_secure("http://127.0.0.1:8080/x").is_ok());
        assert!(require_secure("http://evil.example/x").is_err());
        assert!(require_secure("http://127.0.0.1.evil.example/x").is_err());
    }
}
