use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace = manifest
        .parent()
        .and_then(|p| p.parent())
        .expect("crates/terminus-bridge → workspace")
        .to_path_buf();
    let triple = match env::var("CARGO_CFG_TARGET_ARCH")
        .unwrap_or_else(|_| "x86_64".into())
        .as_str()
    {
        "aarch64" => "aarch64-unknown-linux-musl",
        _ => "x86_64-unknown-linux-musl",
    };
    // Prefer CARGO_TARGET_DIR when set (sandbox / custom layouts), else workspace target/.
    let target_root = env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join("target"));
    let musl_candidates = [
        target_root.join(format!("{triple}/release/terminus-walk")),
        workspace.join(format!("target/{triple}/release/terminus-walk")),
    ];
    for c in &musl_candidates {
        println!("cargo:rerun-if-changed={}", c.display());
    }
    println!(
        "cargo:rerun-if-changed={}",
        workspace.join("crates/terminus-walk/src/main.rs").display()
    );

    // Releases must ship the helper: scripts/release.sh builds it first and
    // sets TERMINUS_REQUIRE_WALK=1. Development builds embed an empty
    // placeholder instead, and the SFTP folder diff reports it clearly at
    // runtime (see walk_remote.rs). We never spawn a nested `cargo` here:
    // the parent build holds the package lock (deadlock, also under nix).
    println!("cargo:rerun-if-env-changed=TERMINUS_REQUIRE_WALK");
    let required = env::var_os("TERMINUS_REQUIRE_WALK").is_some_and(|v| v != "0");

    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let embedded = out.join("terminus-walk.embedded");
    match musl_candidates.iter().find(|p| p.is_file()) {
        Some(musl_bin) => {
            fs::copy(musl_bin, &embedded).expect("copy musl terminus-walk into OUT_DIR");
        }
        None if required => panic!(
            "terminus-walk is required but missing at {}; run: cargo build -p terminus-walk --release --target {triple}",
            musl_candidates[0].display()
        ),
        None => fs::write(&embedded, []).expect("write empty embed placeholder"),
    }
}
