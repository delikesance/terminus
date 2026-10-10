use super::*;
use crate::geom::Rect;
use crate::settings::FIELD_CARD_HEIGHT;

#[test]
fn a_failed_connection_stays_visible_on_its_own_pane() {
    let mut state = SftpPaneState::new_local_remote("/home/user", "h1", "demo");
    state.set_connect_error(SftpFocus::Right, "Nothing answers on that port.".into());
    // The local listing that lands right after must not hide it.
    state.set_listed(SftpFocus::Left, "/home/user".into(), Vec::new());
    assert_eq!(
        state.right.connect_error.as_deref(),
        Some("Nothing answers on that port.")
    );
    assert!(state.footer_text().contains("Nothing answers"));
    assert!(!state.loading);
    // A later listing on that pane (reconnected) clears it.
    state.set_listed(SftpFocus::Right, "/".into(), Vec::new());
    assert_eq!(state.right.connect_error, None);
    assert_eq!(state.footer_text(), state.status);
}

#[test]
fn transfer_fraction_and_caption() {
    let t = SftpTransfer {
        label: "Uploading build.tar.gz".into(),
        done: 11 * 1024 * 1024,
        total: 18 * 1024 * 1024,
    };
    assert!((t.fraction() - 11.0 / 18.0).abs() < 1e-6);
    assert_eq!(t.caption(), "61 % \u{b7} 11 of 18 MB");
    let unknown = SftpTransfer {
        label: "x".into(),
        done: 0,
        total: 0,
    };
    assert_eq!(unknown.fraction(), 0.0);
    assert_eq!(unknown.caption(), "");
    let kb = SftpTransfer {
        label: "x".into(),
        done: 512,
        total: 2048,
    };
    assert_eq!(kb.caption(), "25 % \u{b7} 0.5 of 2 KB");
    let over = SftpTransfer {
        label: "x".into(),
        done: 99,
        total: 10,
    };
    assert_eq!(over.fraction(), 1.0);
}

#[test]
fn format_bytes_units() {
    assert_eq!(format_bytes(0), "0 B");
    assert_eq!(format_bytes(900), "900 B");
    assert_eq!(format_bytes(4096), "4 KB");
    assert_eq!(format_bytes(1536), "1.5 KB");
    assert_eq!(format_bytes(18 * 1024 * 1024), "18 MB");
}

#[test]
fn set_and_clear_transfer() {
    let mut st = SftpPaneState::new_local_local("/a", "/b");
    assert!(st.transfer.is_none());
    st.set_transfer("Copy a", 1, 2);
    assert_eq!(st.transfer.as_ref().unwrap().total, 2);
    st.clear_transfer();
    assert!(st.transfer.is_none());
}

fn sample_state() -> SftpPaneState {
    let mut state = SftpPaneState::new_local_remote("/home/user", "h1", "demo");
    state.set_listed(
        SftpFocus::Left,
        "/home/user".into(),
        vec![
            SftpRow {
                name: "docs".into(),
                path: "/home/user/docs".into(),
                is_dir: true,
                size: 0,
                modified: None,
            },
            SftpRow {
                name: "a.txt".into(),
                path: "/home/user/a.txt".into(),
                is_dir: false,
                size: 12,
                modified: None,
            },
        ],
    );
    state.set_listed(
        SftpFocus::Right,
        "/var".into(),
        vec![SftpRow {
            name: "log".into(),
            path: "/var/log".into(),
            is_dir: true,
            size: 0,
            modified: None,
        }],
    );
    state
}

#[test]
fn layout_name_edit_uses_field_card_height() {
    let layout = SftpPaneLayout::with_name_edit(Rect::new(0.0, 0.0, 640.0, 400.0), true);
    assert!(
        (layout.name_field.height - FIELD_CARD_HEIGHT).abs() < 0.01,
        "name field should be FIELD_CARD_HEIGHT, got {}",
        layout.name_field.height
    );
    assert!(layout.toolbar.height > TOOLBAR_HEIGHT);
    assert!(layout.left.y >= layout.toolbar.bottom() - 0.01);
    assert!(layout.name_field.x < layout.btn_close.x);
}

#[test]
fn name_edit_field_paint_uses_shared_model() {
    let mut state = sample_state();
    state.begin_mkdir();
    let edit = state.name_edit.as_ref().unwrap();
    let paint = edit.field_paint();
    assert_eq!(edit.field_label(), "Folder name");
    assert!(paint.placeholder || paint.text.is_empty());
    assert!(paint.show_caret);
    assert_eq!(edit.side, SftpFocus::Left);
}

#[test]
fn hit_test_close_only_in_toolbar() {
    let state = sample_state();
    let layout = SftpPaneLayout::from_bounds(Rect::new(0.0, 0.0, 500.0, 360.0));
    assert_eq!(
        layout.hit_test(&state, layout.btn_close.x + 4.0, layout.btn_close.y + 4.0),
        SftpHit::Close
    );
    // Slim toolbar: no mkdir/upload/download action hits.
    assert_eq!(
        layout.hit_test(&state, layout.toolbar.x + 20.0, layout.toolbar.y + 10.0),
        SftpHit::Consume
    );
}

#[test]
fn hit_test_rows_and_parent() {
    let state = sample_state();
    let layout = SftpPaneLayout::from_bounds(Rect::new(0.0, 0.0, 400.0, 300.0));

    let row0 = layout.left_row_rect(0, 0.0);
    assert_eq!(
        layout.hit_test(&state, row0.x + 4.0, row0.y + 4.0),
        SftpHit::LeftRow(0)
    );

    assert_eq!(
        layout.hit_test(
            &state,
            layout.left_parent_btn.x + 4.0,
            layout.left_parent_btn.y + 4.0
        ),
        SftpHit::LeftParent
    );
    assert_eq!(layout.hit_test(&state, -10.0, -10.0), SftpHit::Miss);
}

#[test]
fn conflict_prompt_hits_replace_keep_and_apply_all() {
    let mut state = sample_state();
    state.begin_conflict(7, SftpConflictKind::File, "readme.txt");
    let layout = SftpPaneLayout::from_bounds(Rect::new(0.0, 0.0, 640.0, 400.0));
    let prompt = state.conflict.clone().unwrap();
    let d = layout.conflict_layout(&prompt).dialog;
    let (ow, keep, apply) = (d.confirm, d.cancel, d.option.unwrap());
    assert_eq!(
        layout.hit_test(&state, ow.x + 4.0, ow.y + 4.0),
        SftpHit::ConflictOverwrite
    );
    assert_eq!(
        layout.hit_test(&state, keep.x + 4.0, keep.y + 4.0),
        SftpHit::ConflictKeep
    );
    assert_eq!(
        layout.hit_test(&state, apply.x + 4.0, apply.y + 4.0),
        SftpHit::ConflictApplyAll
    );
    // Everywhere else in the pane (scrim, dialog body) is swallowed.
    assert_eq!(layout.hit_test(&state, 5.0, 5.0), SftpHit::Consume);
    assert_eq!(
        layout.hit_test(&state, d.title.x + 2.0, d.title.y + 2.0),
        SftpHit::Consume
    );
    assert!(state.toggle_conflict_apply_all());
    assert!(state.conflict.as_ref().unwrap().apply_to_all);
}

#[test]
fn conflict_dialog_copy_names_the_file_and_offers_replace_or_keep() {
    let mut state = sample_state();
    state.begin_conflict(1, SftpConflictKind::File, "app/build.tar.gz");
    let spec = state.conflict.as_ref().unwrap().spec();
    assert_eq!(spec.title, "build.tar.gz already exists");
    assert_eq!(spec.confirm, "Replace");
    assert_eq!(spec.cancel, "Keep existing");
    assert_eq!(spec.option.as_deref(), Some("Do this for every conflict"));
    state.begin_conflict(2, SftpConflictKind::Directory, "photos");
    assert_eq!(
        state.conflict.as_ref().unwrap().spec().title,
        "photos already exists"
    );
}

#[test]
fn conflict_keys_follow_the_dialog_rules() {
    use crate::components::overlay::DialogKey;
    let mut state = sample_state();
    state.begin_conflict(1, SftpConflictKind::File, "a.txt");
    let c = state.conflict.as_mut().unwrap();
    assert_eq!(c.key(DialogKey::Tab), SftpConflictKey::Changed);
    assert_eq!(c.key(DialogKey::Enter), SftpConflictKey::Keep);
    assert_eq!(c.key(DialogKey::Tab), SftpConflictKey::Changed);
    assert_eq!(c.key(DialogKey::Enter), SftpConflictKey::Overwrite);
    assert_eq!(c.key(DialogKey::Escape), SftpConflictKey::Keep);
}

#[test]
fn side_titles() {
    let state = sample_state();
    assert_eq!(state.left.title(), "Local");
    assert_eq!(state.right.title(), "demo");
    assert!(state.left.is_local());
    assert!(!state.right.is_local());
}

#[test]
fn parent_and_join_paths() {
    assert_eq!(parent_path("/home/alice/docs"), "/home/alice");
    assert_eq!(parent_path("/"), "/");
    assert_eq!(join_remote("/", "tmp"), "/tmp");
    assert_eq!(join_remote("/home", "alice"), "/home/alice");
}

#[test]
fn move_selection_clamps() {
    let mut state = sample_state();
    state.focus = SftpFocus::Left;
    state.move_selection(1);
    assert_eq!(state.left.selected, Some(1));
    state.move_selection(10);
    assert_eq!(state.left.selected, Some(1));
    state.move_selection(-10);
    assert_eq!(state.left.selected, Some(0));
}

#[test]
fn hit_test_covers_all_primary_targets() {
    let mut state = sample_state();
    state.begin_mkdir();
    let layout = SftpPaneLayout::from_state(Rect::new(0.0, 0.0, 640.0, 400.0), &state);

    assert_eq!(
        layout.hit_test(
            &state,
            layout.left_header.x + 4.0,
            layout.left_header.y + 4.0
        ),
        SftpHit::LeftCrumb
    );
    assert_eq!(
        layout.hit_test(
            &state,
            layout.right_header.x + 4.0,
            layout.right_header.y + 4.0
        ),
        SftpHit::RightCrumb
    );
    assert_eq!(
        layout.hit_test(
            &state,
            layout.right_parent_btn.x + 2.0,
            layout.right_parent_btn.y + 2.0
        ),
        SftpHit::RightParent
    );
    assert_eq!(
        layout.hit_test(&state, layout.name_field.x + 4.0, layout.name_field.y + 4.0),
        SftpHit::NameField
    );
    assert_eq!(
        layout.hit_test(
            &state,
            layout.name_confirm.x + 2.0,
            layout.name_confirm.y + 2.0
        ),
        SftpHit::NameConfirm
    );
    assert_eq!(
        layout.hit_test(
            &state,
            layout.name_cancel.x + 2.0,
            layout.name_cancel.y + 2.0
        ),
        SftpHit::NameCancel
    );
    assert_eq!(
        layout.hit_test(&state, layout.footer.x + 8.0, layout.footer.y + 4.0),
        SftpHit::Footer
    );
    let right_row = layout.right_row_rect(0, 0.0);
    assert_eq!(
        layout.hit_test(&state, right_row.x + 4.0, right_row.y + 4.0),
        SftpHit::RightRow(0)
    );
}

#[test]
fn crumb_segments_and_navigate() {
    let state = sample_state();
    let segs = state.crumb_segments(SftpFocus::Left);
    assert!(
        segs.len() >= 2,
        "expected root + home/user segments, got {segs:?}"
    );
    assert_eq!(segs.last().map(|(_, p)| p.as_str()), Some("/home/user"));
    let root = state.navigate_crumb_path(SftpFocus::Left, 0).unwrap();
    assert_eq!(root, "/");
    let mid = state.navigate_crumb_path(SftpFocus::Left, segs.len() - 1);
    assert_eq!(mid.as_deref(), Some("/home/user"));
}

#[test]
fn name_edit_rename_label() {
    let mut state = sample_state();
    state.focus = SftpFocus::Left;
    state.left.selected = Some(1);
    assert!(state.begin_rename());
    let edit = state.name_edit.as_ref().unwrap();
    assert_eq!(edit.kind, SftpNameKind::Rename);
    assert_eq!(edit.field_label(), "New name");
    assert_eq!(edit.draft.value, "a.txt");
}

#[test]
fn local_local_constructor() {
    let state = SftpPaneState::new_local_local("/tmp/a", "/tmp/b");
    assert!(state.left.is_local());
    assert!(state.right.is_local());
    assert_eq!(state.left.cwd, "/tmp/a");
    assert_eq!(state.right.cwd, "/tmp/b");
}

#[test]
fn drag_struct_carries_dir_flag() {
    let drag = SftpDrag {
        from: SftpFocus::Left,
        row_index: 0,
        name: "docs".into(),
        path: "/home/user/docs".into(),
        is_dir: true,
        pointer_x: 10.0,
        pointer_y: 20.0,
    };
    assert!(drag.is_dir);
}
