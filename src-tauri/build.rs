fn main() {
    // Opt-in E2E test hooks: set TERMINUS_E2E=1 at compile time to enable
    // `test_set_host_connection` (and related) in release CI. Absent by default
    // so normal release builds do not ship the Tauri test command.
    println!("cargo:rerun-if-env-changed=TERMINUS_E2E");
    println!("cargo:rustc-check-cfg=cfg(terminus_e2e)");
    if std::env::var("TERMINUS_E2E").ok().as_deref() == Some("1") {
        println!("cargo:rustc-cfg=terminus_e2e");
    }
    // Linux GSSAPI/krb5 is vendored next to the binary / in /usr/lib/terminus.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/../lib/terminus");
        println!("cargo:rustc-link-arg=-Wl,-z,origin");
        println!("cargo:rustc-link-arg=-Wl,--disable-new-dtags");
    }
    tauri_build::build()
}
