fn main() {
    // Mirror src-tauri: compile `SessionManager::test_set_connection` when
    // TERMINUS_E2E=1 so release+E2E CI can call it from the Tauri command.
    println!("cargo:rerun-if-env-changed=TERMINUS_E2E");
    println!("cargo:rustc-check-cfg=cfg(terminus_e2e)");
    if std::env::var("TERMINUS_E2E").ok().as_deref() == Some("1") {
        println!("cargo:rustc-cfg=terminus_e2e");
    }
}
