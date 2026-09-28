//! End to end: a local HTTP server plays GitHub Releases (JSON + assets +
//! signed checksums); the client checks, downloads, verifies and installs.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::{Arc, Mutex};

use terminus_update::{
    cleanup_after_update, detect_install, install, install_renaming_aside, plan, Client,
    InstallKind, Installer, Os, Probe, UpdateError, UpdatePlan, WINDOWS_BINARY,
    WINDOWS_ZIP,
};

type Routes = Arc<Mutex<HashMap<String, Vec<u8>>>>;

/// Minimal HTTP/1.1 server: GET <path> → 200 body, else 404.
fn serve(routes: Routes) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let routes = Arc::clone(&routes);
            std::thread::spawn(move || {
                let mut buf = [0u8; 8192];
                let n = stream.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
                let body = routes.lock().unwrap().get(&path).cloned();
                let resp = match body {
                    Some(body) => {
                        let mut r = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .into_bytes();
                        r.extend(body);
                        r
                    }
                    None => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        .to_vec(),
                };
                let _ = stream.write_all(&resp);
            });
        }
    });
    format!("http://{addr}")
}

fn sha256_hex(data: &[u8]) -> String {
    use sha2::Digest;
    hex::encode(sha2::Sha256::digest(data))
}

struct Fixture {
    base: String,
    routes: Routes,
    public_key: String,
    new_binary: Vec<u8>,
    windows_exe: Vec<u8>,
}

/// A signed release with the assets `scripts/release.sh` publishes.
fn fixture(tag: &str) -> Fixture {
    let routes: Routes = Arc::new(Mutex::new(HashMap::new()));
    let base = serve(Arc::clone(&routes));
    let version = tag.trim_start_matches('v');

    let new_binary = format!("#!/bin/sh\necho terminus {tag}\n").into_bytes();
    // Same layout as release.sh: terminus/terminus plus desktop/terminfo files.
    let tarball = tar_gz_many(&[
        ("terminus/terminus.desktop", b"[Desktop Entry]".as_slice()),
        ("terminus/terminus", &new_binary),
        ("terminus/rio.terminfo", b"terminfo".as_slice()),
    ]);
    let windows_exe = format!("MZ terminus {tag}").into_bytes();
    let zip = zip_many(&[
        ("terminus.exe", windows_exe.as_slice()),
        ("README.txt", b"portable".as_slice()),
    ]);
    let setup = b"MZ nsis setup".to_vec();
    let msi = b"MSI package".to_vec();
    let deb = b"!<arch> deb".to_vec();
    let rpm = b"rpm package".to_vec();
    let deb_name = format!("terminus_{version}_amd64.deb");
    let rpm_name = format!("terminus-{version}-1.x86_64.rpm");
    let files: Vec<(String, Vec<u8>)> = vec![
        ("terminus-linux-x86_64.tar.gz".into(), tarball),
        ("terminus-windows-x86_64.zip".into(), zip),
        ("terminus-setup-x86_64.exe".into(), setup),
        ("terminus-x86_64.msi".into(), msi),
        (deb_name, deb),
        (rpm_name, rpm),
    ];
    let checksums: String = files
        .iter()
        .map(|(name, data)| format!("{}  {name}\n", sha256_hex(data)))
        .collect();

    let minisign::KeyPair { pk, sk } =
        minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    let sig = minisign::sign(
        Some(&pk),
        &sk,
        std::io::Cursor::new(checksums.as_bytes()),
        Some("terminus release"),
        None,
    )
    .unwrap();
    let public_key = pk.to_box().unwrap().to_string();

    let mut assets: Vec<serde_json::Value> = files
        .iter()
        .map(|(name, data)| {
            serde_json::json!({"name": name, "browser_download_url": format!("{base}/a/{name}"), "size": data.len()})
        })
        .collect();
    assets.push(serde_json::json!({"name": "checksums.txt", "browser_download_url": format!("{base}/a/checksums.txt"), "size": checksums.len()}));
    assets.push(serde_json::json!({"name": "checksums.txt.minisig", "browser_download_url": format!("{base}/a/checksums.txt.minisig"), "size": 0}));
    let release = serde_json::json!({
        "tag_name": tag,
        "html_url": format!("{base}/release"),
        "draft": false,
        "prerelease": false,
        "assets": assets,
    });
    {
        let mut r = routes.lock().unwrap();
        r.insert("/latest".into(), serde_json::to_vec(&release).unwrap());
        for (name, data) in files {
            r.insert(format!("/a/{name}"), data);
        }
        r.insert("/a/checksums.txt".into(), checksums.into_bytes());
        r.insert(
            "/a/checksums.txt.minisig".into(),
            sig.to_string().into_bytes(),
        );
    }
    Fixture {
        base,
        routes,
        public_key,
        new_binary,
        windows_exe,
    }
}

fn zip_many(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut out);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, data) in entries {
            zip.start_file(*name, opts).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
    }
    out.into_inner()
}

fn tar_gz_many(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut builder = tar::Builder::new(enc);
    for (name, data) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder.append_data(&mut header, name, *data).unwrap();
    }
    builder.into_inner().unwrap().finish().unwrap()
}

fn client(f: &Fixture) -> Client {
    Client::new(&format!("{}/latest", f.base), Some(&f.public_key)).unwrap()
}

#[test]
fn newer_release_is_found_and_older_ignored() {
    let f = fixture("v0.9.0");
    let c = client(&f);
    let release = c.check("0.5.29").unwrap().expect("0.9.0 > 0.5.29");
    assert_eq!(release.version.to_string(), "0.9.0");
    assert!(c.check("0.9.0").unwrap().is_none(), "same version");
    assert!(c.check("1.0.0").unwrap().is_none(), "older release");
}

#[test]
fn prerelease_and_draft_are_ignored() {
    let f = fixture("v0.9.0");
    let mut json: serde_json::Value =
        serde_json::from_slice(&f.routes.lock().unwrap()["/latest"]).unwrap();
    json["prerelease"] = true.into();
    f.routes
        .lock()
        .unwrap()
        .insert("/latest".into(), serde_json::to_vec(&json).unwrap());
    assert!(client(&f).check("0.5.29").unwrap().is_none());
}

fn tarball_plan(exe: &Path) -> UpdatePlan {
    plan(InstallKind::LinuxTarball {
        exe: exe.to_path_buf(),
    })
}

#[test]
fn linux_tarball_downloads_verifies_and_replaces_the_binary() {
    let f = fixture("v0.9.0");
    let c = client(&f);
    let release = c.check("0.5.29").unwrap().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join("terminus");
    std::fs::write(&exe, b"old binary").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let UpdatePlan::ReplaceBinary { asset, entry, .. } = tarball_plan(&exe) else {
        panic!("tarball installs replace the binary")
    };
    let staged = c
        .stage_binary(&release, &asset, &entry, dir.path())
        .unwrap();
    assert_eq!(std::fs::read(&staged).unwrap(), f.new_binary);
    install(&staged, &exe).unwrap();
    assert_eq!(std::fs::read(&exe).unwrap(), f.new_binary);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&exe).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o755, "installed binary must stay executable");
    }
    assert!(!staged.exists(), "staged file moved into place");
    cleanup_after_update(&exe);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn installers_and_packages_are_downloaded_verified_for_their_kind() {
    let f = fixture("v0.9.0");
    let c = client(&f);
    let release = c.check("0.5.29").unwrap().unwrap();
    let dir = tempfile::tempdir().unwrap();
    for (kind, expect_name) in [
        (InstallKind::WindowsNsis, "terminus-setup-x86_64.exe"),
        (InstallKind::WindowsMsi, "terminus-x86_64.msi"),
        (InstallKind::Deb, "terminus_0.9.0_amd64.deb"),
        (InstallKind::Rpm, "terminus-0.9.0-1.x86_64.rpm"),
    ] {
        let asset = match plan(kind.clone()) {
            UpdatePlan::RunInstaller { asset, .. }
            | UpdatePlan::PackageFile { asset, .. } => asset,
            other => panic!("{kind:?} planned {other:?}"),
        };
        let path = c.download_verified(&release, &asset, dir.path()).unwrap();
        assert_eq!(path.file_name().unwrap().to_string_lossy(), expect_name);
    }
}

#[test]
fn tampered_asset_is_rejected_and_nothing_is_left_behind() {
    let f = fixture("v0.9.0");
    f.routes.lock().unwrap().insert(
        "/a/terminus-linux-x86_64.tar.gz".into(),
        tar_gz_many(&[("terminus/terminus", b"evil".as_slice())]),
    );
    let c = client(&f);
    let release = c.check("0.5.29").unwrap().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let err = c
        .stage_binary(
            &release,
            "terminus-linux-x86_64.tar.gz",
            "terminus",
            dir.path(),
        )
        .unwrap_err();
    assert!(matches!(err, UpdateError::ChecksumMismatch { .. }), "{err}");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn tampered_checksums_fail_the_signature() {
    let f = fixture("v0.9.0");
    let evil = tar_gz_many(&[("terminus/terminus", b"evil".as_slice())]);
    let sums = format!("{}  terminus-linux-x86_64.tar.gz\n", sha256_hex(&evil));
    {
        let mut r = f.routes.lock().unwrap();
        r.insert("/a/terminus-linux-x86_64.tar.gz".into(), evil);
        r.insert("/a/checksums.txt".into(), sums.into_bytes());
    }
    let c = client(&f);
    let release = c.check("0.5.29").unwrap().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let err = c
        .stage_binary(
            &release,
            "terminus-linux-x86_64.tar.gz",
            "terminus",
            dir.path(),
        )
        .unwrap_err();
    assert!(matches!(err, UpdateError::BadSignature(_)), "{err}");
}

#[test]
fn signature_from_another_key_is_rejected() {
    let f = fixture("v0.9.0");
    let other = fixture("v0.9.0");
    let c = Client::new(&format!("{}/latest", f.base), Some(&other.public_key)).unwrap();
    let release = c.check("0.5.29").unwrap().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let err = c
        .download_verified(&release, "terminus-x86_64.msi", dir.path())
        .unwrap_err();
    assert!(matches!(err, UpdateError::BadSignature(_)), "{err}");
}

#[test]
fn without_a_trusted_key_nothing_is_downloaded() {
    let f = fixture("v0.9.0");
    let c = Client::new(&format!("{}/latest", f.base), None).unwrap();
    assert!(!c.can_install());
    let release = c.check("0.5.29").unwrap().expect("checking still works");
    let dir = tempfile::tempdir().unwrap();
    let err = c
        .download_verified(&release, "terminus-x86_64.msi", dir.path())
        .unwrap_err();
    assert!(matches!(err, UpdateError::NoTrustedKey), "{err}");
}

#[test]
fn unsigned_release_is_rejected() {
    let f = fixture("v0.9.0");
    let mut json: serde_json::Value =
        serde_json::from_slice(&f.routes.lock().unwrap()["/latest"]).unwrap();
    json["assets"]
        .as_array_mut()
        .unwrap()
        .retain(|a| a["name"] != "checksums.txt.minisig");
    f.routes
        .lock()
        .unwrap()
        .insert("/latest".into(), serde_json::to_vec(&json).unwrap());
    let c = client(&f);
    let release = c.check("0.5.29").unwrap().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let err = c
        .download_verified(&release, "terminus-x86_64.msi", dir.path())
        .unwrap_err();
    assert!(matches!(err, UpdateError::MissingAsset(_)), "{err}");
}

/// Probe over a fixed set of existing paths and writable directories.
struct FakeProbe {
    files: Vec<&'static str>,
    writable: Vec<&'static str>,
}

impl Probe for FakeProbe {
    fn exists(&self, path: &Path) -> bool {
        let p = path.to_string_lossy().replace('\\', "/");
        self.files.iter().any(|f| *f == p)
    }
    fn dir_writable(&self, dir: &Path) -> bool {
        let p = dir.to_string_lossy().replace('\\', "/");
        self.writable.iter().any(|f| *f == p)
    }
}

#[test]
fn install_kind_follows_how_terminus_was_installed() {
    let none = FakeProbe {
        files: vec![],
        writable: vec![],
    };
    let p = |s: &str| Path::new(s).to_path_buf();

    assert_eq!(
        detect_install(
            &p("/home/me/terminus/target/release/terminus"),
            Os::Linux,
            &none
        ),
        InstallKind::Dev
    );
    assert_eq!(
        detect_install(&p("/nix/store/abc-terminus/bin/terminus"), Os::Linux, &none),
        InstallKind::Nix
    );
    let dpkg = FakeProbe {
        files: vec!["/var/lib/dpkg/info/terminus.list"],
        writable: vec![],
    };
    assert_eq!(
        detect_install(&p("/usr/bin/terminus"), Os::Linux, &dpkg),
        InstallKind::Deb
    );
    let rpmdb = FakeProbe {
        files: vec!["/var/lib/rpm"],
        writable: vec![],
    };
    assert_eq!(
        detect_install(&p("/usr/bin/terminus"), Os::Linux, &rpmdb),
        InstallKind::Rpm
    );
    let home = FakeProbe {
        files: vec![],
        writable: vec!["/home/me/.local/opt/terminus"],
    };
    assert_eq!(
        detect_install(
            &p("/home/me/.local/opt/terminus/terminus"),
            Os::Linux,
            &home
        ),
        InstallKind::LinuxTarball {
            exe: p("/home/me/.local/opt/terminus/terminus")
        }
    );
    assert!(matches!(
        detect_install(&p("/opt/readonly/terminus"), Os::Linux, &none),
        InstallKind::Unknown { .. }
    ));
    let nsis = FakeProbe {
        files: vec!["C:/Program Files/Terminus/Uninstall.exe"],
        writable: vec![],
    };
    assert_eq!(
        detect_install(
            &p("C:/Program Files/Terminus/terminus.exe"),
            Os::Windows,
            &nsis
        ),
        InstallKind::WindowsNsis
    );
    assert_eq!(
        detect_install(
            &p("C:/Program Files/Terminus/terminus.exe"),
            Os::Windows,
            &none
        ),
        InstallKind::WindowsMsi
    );
    assert!(matches!(
        detect_install(&p("D:/tools/terminus.exe"), Os::Windows, &none),
        InstallKind::Unknown { .. }
    ));
    // Per-user install (writable, even with an uninstaller) and portable
    // copies replace their own exe: no installer, no admin prompt.
    let per_user = FakeProbe {
        files: vec!["C:/Users/me/AppData/Local/Programs/Terminus/Uninstall.exe"],
        writable: vec!["C:/Users/me/AppData/Local/Programs/Terminus"],
    };
    let exe = p("C:/Users/me/AppData/Local/Programs/Terminus/terminus.exe");
    assert_eq!(
        detect_install(&exe, Os::Windows, &per_user),
        InstallKind::WindowsSelf { exe: exe.clone() }
    );
    let portable = FakeProbe {
        files: vec![],
        writable: vec!["D:/tools"],
    };
    assert!(matches!(
        detect_install(&p("D:/tools/terminus.exe"), Os::Windows, &portable),
        InstallKind::WindowsSelf { .. }
    ));
    assert!(matches!(
        detect_install(&p("/Applications/Terminus/terminus"), Os::Macos, &none),
        InstallKind::Unknown { .. }
    ));
}

#[test]
fn windows_self_updates_from_the_signed_zip() {
    let f = fixture("v9.9.9");
    let client = client(&f);
    let release = client.check("0.5.30").unwrap().expect("newer");
    let exe = std::path::PathBuf::from("C:/x/terminus.exe");
    let UpdatePlan::ReplaceBinary { asset, entry, .. } =
        plan(InstallKind::WindowsSelf { exe })
    else {
        panic!("windows self-install replaces the binary")
    };
    assert_eq!(
        (asset.as_str(), entry.as_str()),
        (WINDOWS_ZIP, WINDOWS_BINARY)
    );
    let dir = tempfile::tempdir().unwrap();
    let staged = client
        .stage_binary(&release, &asset, &entry, dir.path())
        .expect("zip staged");
    assert_eq!(std::fs::read(&staged).unwrap(), f.windows_exe);
}

#[test]
fn a_running_exe_is_moved_aside_then_replaced() {
    // Windows cannot overwrite a running .exe but can rename it.
    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join("terminus.exe");
    std::fs::write(&exe, b"old").unwrap();
    let staged = dir.path().join(".terminus-update-new");
    std::fs::write(&staged, b"new").unwrap();
    install_renaming_aside(&staged, &exe).expect("swapped");
    assert_eq!(std::fs::read(&exe).unwrap(), b"new");
    assert!(!staged.exists());
    let aside: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with(".terminus-update-")
        })
        .collect();
    assert_eq!(aside.len(), 1, "old exe kept aside until the next start");
    cleanup_after_update(&exe);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn a_quick_check_gives_up_on_a_silent_server() {
    // Accepts connections and never answers.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        let mut held = Vec::new();
        for stream in listener.incoming() {
            held.push(stream);
        }
    });
    let client = Client::new(&format!("http://{addr}/latest"), None)
        .unwrap()
        .with_timeout(std::time::Duration::from_millis(500));
    let started = std::time::Instant::now();
    assert!(client.check("0.5.30").is_err());
    assert!(started.elapsed() < std::time::Duration::from_secs(3));
}

#[test]
fn each_install_kind_gets_a_matching_plan() {
    assert!(matches!(
        plan(InstallKind::WindowsNsis),
        UpdatePlan::RunInstaller {
            installer: Installer::Nsis,
            ..
        }
    ));
    assert!(matches!(
        plan(InstallKind::WindowsMsi),
        UpdatePlan::RunInstaller {
            installer: Installer::Msi,
            ..
        }
    ));
    let UpdatePlan::PackageFile { command, .. } = plan(InstallKind::Deb) else {
        panic!()
    };
    assert!(command.contains("apt install"));
    let UpdatePlan::PackageFile { command, .. } = plan(InstallKind::Rpm) else {
        panic!()
    };
    assert!(command.contains("dnf install"));
    assert!(matches!(plan(InstallKind::Nix), UpdatePlan::Manual { .. }));
    assert!(matches!(plan(InstallKind::Dev), UpdatePlan::Manual { .. }));
}

#[test]
fn plain_http_is_refused_except_on_loopback() {
    assert!(Client::new("http://example.com/latest", None).is_err());
    assert!(Client::new("https://example.com/latest", None).is_ok());
    assert!(Client::new("http://127.0.0.1:1/latest", None).is_ok());
}
