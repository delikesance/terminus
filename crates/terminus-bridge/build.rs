use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

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

    let musl_bin = musl_candidates
        .iter()
        .find(|p| p.is_file())
        .cloned()
        .unwrap_or_else(|| musl_candidates[0].clone());

    if !musl_bin.is_file() {
        // Best-effort auto-build (works with rustup; may fail inside pure nix without musl target).
        let _ = Command::new("cargo")
            .args([
                "build",
                "-p",
                "terminus-walk",
                "--release",
                "--target",
                triple,
            ])
            .current_dir(&workspace)
            .status();
    }

    let musl_bin = musl_candidates
        .iter()
        .find(|p| p.is_file())
        .cloned()
        .unwrap_or(musl_bin);

    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let embedded = out.join("terminus-walk.embedded");
    if musl_bin.is_file() {
        fs::copy(&musl_bin, &embedded).expect("copy musl terminus-walk into OUT_DIR");
        println!(
            "cargo:warning=embedded terminus-walk from {} ({} bytes)",
            musl_bin.display(),
            fs::metadata(&musl_bin).map(|m| m.len()).unwrap_or(0)
        );
    } else {
        // Placeholder so include_bytes! still compiles; runtime will error clearly.
        fs::write(&embedded, []).expect("write empty embed placeholder");
        println!(
            "cargo:warning=terminus-walk musl binary missing at {}; run: cargo build -p terminus-walk --release --target {triple}",
            musl_bin.display()
        );
    }
}
