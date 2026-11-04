//! Multi-selection, batch actions and the click rules of the Files browser.

use super::*;
use std::time::Duration;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "terminus-sftp-multi-{}-{}",
        name,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Pump until `done` holds (listings and transfers arrive asynchronously).
fn pump_until(s: &mut ActiveSftp, mut done: impl FnMut(&ActiveSftp) -> bool) -> bool {
    for _ in 0..150 {
        s.pump();
        if done(s) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

fn browser(name: &str, files: &[&str]) -> (ActiveSftp, PathBuf, PathBuf, PathBuf) {
    let root = scratch(name);
    let (left, right) = (root.join("L"), root.join("R"));
    std::fs::create_dir_all(&left).unwrap();
    std::fs::create_dir_all(&right).unwrap();
    for f in files {
        std::fs::write(left.join(f), f.as_bytes()).unwrap();
    }
    let mut s = ActiveSftp::start_local_dual(left.clone(), right.clone(), None);
    let n = files.len();
    assert!(pump_until(&mut s, |s| s.state.left.entries.len() == n
        && !s.state.loading));
    (s, root, left, right)
}

#[test]
fn select_all_then_transfer_copies_every_file() {
    let (mut s, root, _left, right) = browser("copy", &["a.txt", "b.txt", "c.txt"]);
    assert!(s.handle_key(SftpKey::SelectAll));
    assert_eq!(s.state.left.selection_count(), 3);
    s.transfer_selected();
    assert!(pump_until(&mut s, |_| {
        ["a.txt", "b.txt", "c.txt"]
            .iter()
            .all(|f| right.join(f).is_file())
    }));
    s.close();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn deleting_a_selection_asks_once_and_removes_every_row() {
    let (mut s, root, left, _right) = browser("delete", &["a.txt", "b.txt", "keep.txt"]);
    s.state.left.select_only(0);
    s.select_mods = SelectMods {
        toggle: false,
        range: true,
    };
    s.handle_hit(SftpHit::LeftRow(1), false);
    assert_eq!(s.state.left.selection_count(), 2);
    let prompt = s.delete_prompt().expect("a prompt for the selection");
    assert_eq!(prompt.spec.title, "Delete 2 items?");
    s.remove_focused();
    assert!(pump_until(&mut s, |_| !left.join("a.txt").exists()
        && !left.join("b.txt").exists()));
    assert!(left.join("keep.txt").exists());
    s.close();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_single_row_still_gets_its_own_name_in_the_prompt() {
    let (mut s, root, _left, _right) = browser("single", &["only.txt"]);
    s.state.left.select_only(0);
    assert_eq!(s.delete_prompt().unwrap().spec.title, "Delete only.txt?");
    s.close();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn double_clicking_a_file_selects_it_without_transferring() {
    let (mut s, root, _left, right) = browser("dbl", &["a.txt"]);
    s.handle_hit(SftpHit::LeftRow(0), true);
    s.handle_hit(SftpHit::LeftRow(0), true);
    std::thread::sleep(Duration::from_millis(300));
    s.pump();
    assert_eq!(s.state.left.selected, Some(0));
    assert!(!right.join("a.txt").exists());
    s.close();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn double_clicking_a_folder_opens_it_and_going_up_lands_on_it() {
    let (mut s, root, left, _right) = browser("dir", &[]);
    std::fs::create_dir_all(left.join("sub")).unwrap();
    std::fs::write(left.join("sub").join("x"), b"x").unwrap();
    s.refresh_focused();
    assert!(pump_until(&mut s, |s| s.state.left.entries.len() == 1));
    s.handle_hit(SftpHit::LeftRow(0), true);
    assert!(pump_until(&mut s, |s| s.state.left.cwd.ends_with("sub")));
    s.handle_key(SftpKey::Backspace);
    assert!(pump_until(&mut s, |s| s
        .state
        .left
        .selected_row()
        .is_some()));
    assert_eq!(s.state.left.selected_row().unwrap().name, "sub");
    s.close();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_renamed_file_stays_selected_after_the_relist() {
    let (mut s, root, _left, _right) = browser("rename", &["a.txt", "b.txt"]);
    s.state.left.select_only(0);
    assert!(s.handle_key(SftpKey::Rename));
    s.state.name_edit.as_mut().unwrap().draft = terminus_ui::TextDraft::new("z.txt");
    s.commit_name_edit();
    assert!(pump_until(&mut s, |s| s
        .state
        .left
        .selected_row()
        .is_some_and(|r| r.name == "z.txt")));
    s.close();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn cancel_without_a_transfer_does_nothing() {
    let (mut s, root, _left, _right) = browser("idle", &["a.txt"]);
    s.cancel_transfer();
    assert_ne!(s.state.status, "Cancelling…");
    s.close();
    let _ = std::fs::remove_dir_all(root);
}
