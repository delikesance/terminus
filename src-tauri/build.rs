fn main() {
    // Opt-in E2E test hooks: set TERMINUS_E2E=1 at compile time to enable
    // `test_set_host_connection` (and related) in release CI. Absent by default
    // so normal release builds do not ship the Tauri test command.
    println!("cargo:rerun-if-env-changed=TERMINUS_E2E");
    println!("cargo:rustc-check-cfg=cfg(terminus_e2e)");
    if std::env::var("TERMINUS_E2E").ok().as_deref() == Some("1") {
        println!("cargo:rustc-cfg=terminus_e2e");
    }
    // Linux Kerberos/GSSAPI must use distro libs (KCM ticket caches, krb5.conf).
    // Do not force an app-local rpath that shadows the system MIT Kerberos stack.
    tauri_build::build()
}
