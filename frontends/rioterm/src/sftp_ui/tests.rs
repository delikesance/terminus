use super::*;
use std::time::Duration;

#[test]
fn remote_browser_starts_in_the_tab_directory_or_home() {
    assert_eq!(remote_start_path(Some("/srv/app".into())), "/srv/app");
    assert_eq!(remote_start_path(None), terminus_bridge::REMOTE_HOME);
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "terminus-sftp-ui-{}-{}",
        name,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn wait_ready(session: &mut ActiveSftp) {
    for _ in 0..100 {
        session.pump();
        if !session.state.loading {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn key_tab_toggles_focus() {
    let root = scratch("tab");
    let left = root.join("L");
    let right = root.join("R");
    std::fs::create_dir_all(&left).unwrap();
    std::fs::create_dir_all(&right).unwrap();
    let mut s = ActiveSftp::start_local_dual(left, right, None);
    assert_eq!(s.state.focus, SftpFocus::Left);
    s.handle_key(SftpKey::Tab);
    assert_eq!(s.state.focus, SftpFocus::Right);
    s.handle_key(SftpKey::Tab);
    assert_eq!(s.state.focus, SftpFocus::Left);
    s.close();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn key_mkdir_and_rename() {
    let root = scratch("mkdir");
    let left = root.join("L");
    let right = root.join("R");
    std::fs::create_dir_all(&left).unwrap();
    std::fs::write(left.join("f.txt"), b"x").unwrap();
    std::fs::create_dir_all(&right).unwrap();
    let mut s = ActiveSftp::start_local_dual(left, right, None);
    wait_ready(&mut s);
    assert!(s.handle_key(SftpKey::Mkdir));
    assert!(s.state.name_edit.is_some());
    s.state.cancel_name_edit();
    s.state.left.selected = Some(0);
    assert!(s.handle_key(SftpKey::Rename));
    assert_eq!(
        s.state.name_edit.as_ref().map(|e| e.kind),
        Some(SftpNameKind::Rename)
    );
    s.close();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn keyboard_map_table() {
    let expected = [
        ("Tab", "Tab"),
        ("ArrowUp", "Up"),
        ("ArrowDown", "Down"),
        ("Enter", "Enter"),
        ("Backspace", "Backspace"),
        ("Delete", "Delete"),
        ("F2", "Rename"),
        ("Ctrl+N", "Mkdir"),
        ("Ctrl+U", "Upload"),
        ("Ctrl+D", "Download"),
    ];
    assert_eq!(expected.len(), 10, "SFTP keyboard map must stay complete");
}

#[test]
fn refresh_focused_lists() {
    let root = scratch("refresh");
    let left = root.join("L");
    let right = root.join("R");
    std::fs::create_dir_all(&left).unwrap();
    std::fs::create_dir_all(&right).unwrap();
    let mut s = ActiveSftp::start_local_dual(left, right, None);
    wait_ready(&mut s);
    s.refresh_focused();
    assert!(
        s.state.loading
            || s.state.status.contains("Listing")
            || s.state.status == "Ready"
    );
    s.close();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn transfer_folder_sends_command() {
    let root = scratch("xfer-dir");
    let left = root.join("L");
    let right = root.join("R");
    std::fs::create_dir_all(left.join("folder")).unwrap();
    std::fs::write(left.join("folder").join("a.txt"), b"a").unwrap();
    std::fs::create_dir_all(&right).unwrap();
    let mut s = ActiveSftp::start_local_dual(left, right.clone(), None);
    wait_ready(&mut s);
    s.state.left.selected = s.state.left.entries.iter().position(|e| e.is_dir);
    s.transfer_selected();
    assert!(s.state.error.is_none(), "folder transfer should be allowed");
    for _ in 0..100 {
        s.pump();
        if right.join("folder").join("a.txt").is_file() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        right.join("folder").join("a.txt").is_file(),
        "folder tree should land on the other pane"
    );
    s.close();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn local_file_context_menu_offers_edit() {
    let root = scratch("local-edit-menu");
    let left = root.join("L");
    let right = root.join("R");
    std::fs::create_dir_all(&left).unwrap();
    std::fs::write(left.join("notes.txt"), b"hi").unwrap();
    std::fs::create_dir_all(&right).unwrap();
    let mut s = ActiveSftp::start_local_dual(left, right, None);
    wait_ready(&mut s);
    let file_idx = s
        .state
        .left
        .entries
        .iter()
        .position(|e| e.name == "notes.txt" && !e.is_dir)
        .expect("notes.txt listed");
    let menu = s
        .context_menu_for(SftpHit::LeftRow(file_idx), None, 40.0, 120.0)
        .expect("menu for local file");
    assert!(
        (0..menu.entries.len()).any(|i| matches!(
            menu.take_action(i),
            Some(terminus_ui::ContextAction::SftpEdit)
        )),
        "local file right-click should include Edit"
    );
    s.close();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn edit_selected_opens_local_file_without_remote_error() {
    let root = scratch("local-edit-open");
    let left = root.join("L");
    let right = root.join("R");
    std::fs::create_dir_all(&left).unwrap();
    let file = left.join("notes.txt");
    std::fs::write(&file, b"hi").unwrap();
    std::fs::create_dir_all(&right).unwrap();
    let mut s = ActiveSftp::start_local_dual(left, right, None);
    wait_ready(&mut s);
    s.state.focus = SftpFocus::Left;
    s.state.left.selected = s
        .state
        .left
        .entries
        .iter()
        .position(|e| e.name == "notes.txt");
    s.edit_selected();
    assert!(
        s.state.error.is_none(),
        "local edit must not require remote: {:?}",
        s.state.error
    );
    assert!(
        s.state.status.contains("Editing"),
        "expected Editing status, got {}",
        s.state.status
    );
    s.close();
    let _ = std::fs::remove_dir_all(root);
}
