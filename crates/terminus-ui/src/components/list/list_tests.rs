use super::*;
use crate::geom::Rect;
use crate::theme::ChromeTheme;

fn card_spec<'a>(widths: &'a [f32]) -> CardSpec<'a> {
    CardSpec {
        has_dot: true,
        meta_width: 40.0,
        action_widths: widths,
    }
}

#[test]
#[allow(clippy::assertions_on_constants)]
fn card_height_covers_padding_and_text_block() {
    assert_eq!(CARD_HEIGHT, 2.0 * 16.0 + CARD_CONTENT_HEIGHT);
    assert!(CARD_CONTENT_HEIGHT >= 36.0, "fits a medium button");
}

#[test]
fn card_dot_is_8px_inside_left_padding_and_centered() {
    let l = card_layout(
        Rect::new(10.0, 20.0, 600.0, CARD_HEIGHT),
        &card_spec(&[60.0]),
    );
    let dot = l.dot.expect("dot");
    assert_eq!((dot.width, dot.height), (8.0, 8.0));
    assert_eq!(dot.x, 10.0 + 20.0);
    assert_eq!(dot.y + 4.0, 20.0 + CARD_HEIGHT / 2.0);
    // text starts after dot + 18 gap
    assert_eq!(l.text.x, dot.right() + 18.0);
}

#[test]
fn card_without_dot_starts_text_at_padding() {
    let mut s = card_spec(&[]);
    s.has_dot = false;
    let l = card_layout(Rect::new(0.0, 0.0, 600.0, CARD_HEIGHT), &s);
    assert!(l.dot.is_none());
    assert_eq!(l.text.x, 20.0);
}

#[test]
fn card_actions_lay_out_right_to_left_with_10px_gaps() {
    let r = Rect::new(0.0, 0.0, 600.0, CARD_HEIGHT);
    let l = card_layout(r, &card_spec(&[80.0, 36.0]));
    let a0 = l.actions[0].unwrap();
    let a1 = l.actions[1].unwrap();
    // slot 0 is the leftmost (first listed), slot 1 hugs the right padding
    assert_eq!(a1.right(), 600.0 - 20.0);
    assert_eq!(a0.right() + 10.0, a1.x);
    assert_eq!((a0.width, a1.width), (80.0, 36.0));
    assert_eq!(a0.height, 36.0);
    assert_eq!(a0.y + 18.0, CARD_HEIGHT / 2.0);
}

#[test]
fn card_meta_sits_left_of_actions_and_text_fills_the_rest() {
    let r = Rect::new(0.0, 0.0, 600.0, CARD_HEIGHT);
    let l = card_layout(r, &card_spec(&[80.0]));
    let a = l.actions[0].unwrap();
    assert_eq!(l.meta.right() + 18.0, a.x);
    assert_eq!(l.meta.width, 40.0);
    assert_eq!(l.text.right() + 18.0, l.meta.x);
    assert!(l.text.width > 0.0);
}

#[test]
fn card_without_actions_meta_hugs_right_padding() {
    let l = card_layout(Rect::new(0.0, 0.0, 600.0, CARD_HEIGHT), &card_spec(&[]));
    assert!(l.actions.iter().all(|a| a.is_none()));
    assert_eq!(l.meta.right(), 580.0);
}

#[test]
fn card_hit_test_prefers_actions_then_body() {
    let r = Rect::new(0.0, 0.0, 600.0, CARD_HEIGHT);
    let spec = card_spec(&[80.0, 36.0]);
    let l = card_layout(r, &spec);
    let a1 = l.actions[1].unwrap();
    assert_eq!(
        card_hit(r, &spec, a1.x + 1.0, a1.y + 1.0),
        Some(CardHit::Action(1))
    );
    let a0 = l.actions[0].unwrap();
    assert_eq!(
        card_hit(r, &spec, a0.x + 1.0, a0.y + 1.0),
        Some(CardHit::Action(0))
    );
    assert_eq!(card_hit(r, &spec, 5.0, 5.0), Some(CardHit::Body));
    // the gap between actions is body, outside is nothing
    assert_eq!(
        card_hit(r, &spec, a0.right() + 5.0, a0.y + 1.0),
        Some(CardHit::Body)
    );
    assert_eq!(card_hit(r, &spec, 700.0, 5.0), None);
}

#[test]
fn card_background_follows_state() {
    let t = ChromeTheme::default();
    assert_eq!(CardState::Default.background(&t), t.frame);
    assert_eq!(CardState::Hover.background(&t), t.surface);
}

#[test]
fn file_row_columns_have_board_widths() {
    let r = Rect::new(5.0, 7.0, 500.0, 40.0);
    let l = file_row_layout(r);
    assert_eq!(l.size.width, 70.0);
    assert_eq!(l.date.width, 80.0);
    assert_eq!(l.date.right(), 500.0 + 5.0 - 10.0);
    assert_eq!(l.size.right() + 12.0, l.date.x);
    assert_eq!(l.icon.x, 5.0 + 10.0);
    assert_eq!(l.icon.width, 15.0);
    assert_eq!(l.name.x, l.icon.right() + 12.0);
    assert_eq!(l.name.right() + 12.0, l.size.x);
}

#[test]
fn file_row_rename_field_is_28px_centered_over_name() {
    let r = Rect::new(0.0, 100.0, 500.0, 40.0);
    let l = file_row_layout(r);
    assert_eq!(l.rename_field.height, 28.0);
    assert_eq!(l.rename_field.y, 106.0);
    assert_eq!(l.rename_field.x, l.name.x);
    assert_eq!(l.rename_field.width, l.name.width);
}

#[test]
fn file_row_hit_test_reports_columns() {
    let r = Rect::new(0.0, 0.0, 500.0, 40.0);
    let l = file_row_layout(r);
    assert_eq!(
        file_row_hit(r, l.icon.x + 1.0, 20.0),
        Some(FileColumn::Icon)
    );
    assert_eq!(
        file_row_hit(r, l.name.x + 1.0, 20.0),
        Some(FileColumn::Name)
    );
    assert_eq!(
        file_row_hit(r, l.size.x + 1.0, 20.0),
        Some(FileColumn::Size)
    );
    assert_eq!(
        file_row_hit(r, l.date.x + 1.0, 20.0),
        Some(FileColumn::Date)
    );
    assert_eq!(file_row_hit(r, 2.0, 20.0), Some(FileColumn::Row));
    assert_eq!(file_row_hit(r, 2.0, 41.0), None);
}

#[test]
fn file_row_state_visuals() {
    let t = ChromeTheme::default();
    assert_eq!(FileRowState::Default.background(&t), None);
    assert_eq!(FileRowState::Renaming.background(&t), None);
    assert_eq!(FileRowState::Hover.background(&t), Some(t.surface));
    assert_eq!(FileRowState::Selected.background(&t), Some(t.selected));
    let drop = FileRowState::DropTarget.background(&t).unwrap();
    assert_eq!(&drop[..3], &t.accent[..3]);
    assert!((drop[3] - 0.1).abs() < 1e-6);
    assert_eq!(FileRowState::DropTarget.inset_stroke(), Some(1.5));
    assert_eq!(FileRowState::Selected.inset_stroke(), None);
    assert!(FileRowState::Renaming.shows_rename_field());
    assert!(!FileRowState::Hover.shows_rename_field());
}

#[test]
fn history_row_is_52_high_with_bottom_divider() {
    let r = Rect::new(0.0, 10.0, 600.0, HISTORY_ROW_HEIGHT);
    let l = history_row_layout(r, 50.0, 100.0);
    assert_eq!(HISTORY_ROW_HEIGHT, 52.0);
    assert_eq!(l.divider, Rect::new(0.0, 10.0 + 51.0, 600.0, 1.0));
}

#[test]
fn history_row_columns_flow_right_to_left() {
    let r = Rect::new(0.0, 0.0, 600.0, HISTORY_ROW_HEIGHT);
    let l = history_row_layout(r, 50.0, 100.0);
    let a = l.action.unwrap();
    assert_eq!(a.right(), 600.0);
    assert_eq!(a.width, 100.0);
    assert_eq!(a.height, 36.0);
    assert_eq!(l.time.width, 90.0);
    assert_eq!(l.time.right() + 18.0, a.x);
    assert_eq!(l.cwd.width, 50.0);
    assert_eq!(l.cwd.right() + 18.0, l.time.x);
    assert_eq!(l.command.x, 0.0);
    assert_eq!(l.command.right() + 18.0, l.cwd.x);
}

#[test]
fn history_row_without_action_slot() {
    let r = Rect::new(0.0, 0.0, 600.0, HISTORY_ROW_HEIGHT);
    let l = history_row_layout(r, 50.0, 0.0);
    assert!(l.action.is_none());
    assert_eq!(l.time.right(), 600.0);
}

#[test]
fn a_long_cwd_never_runs_over_the_command() {
    // 1100 px window: the row is ~770 px, the cwd a deep worktree path.
    let r = Rect::new(288.0, 0.0, 770.0, HISTORY_ROW_HEIGHT);
    let l = history_row_layout(r, 600.0, 90.0);
    assert!(l.cwd.x >= l.command.right() + HISTORY_GAP - 0.01);
    assert!(l.command.width >= l.cwd.width, "the command keeps half");
    assert!(l.cwd.right() <= l.time.x);
    // A short cwd keeps its measured width.
    let short = history_row_layout(r, 60.0, 90.0);
    assert_eq!(short.cwd.width, 60.0);
}

#[test]
fn elision_keeps_the_meaningful_end() {
    assert_eq!(elide_start("/home/me/dev/app", 9), "\u{2026}/dev/app");
    assert_eq!(elide_start("~/app", 9), "~/app");
    assert_eq!(elide_end("docker compose up -d", 10), "docker co\u{2026}");
    assert_eq!(elide_end("git pull", 10), "git pull");
    assert_eq!(elide_end("abc", 0), "");
}

#[test]
fn history_hit_test() {
    let r = Rect::new(0.0, 0.0, 600.0, HISTORY_ROW_HEIGHT);
    let l = history_row_layout(r, 50.0, 100.0);
    let a = l.action.unwrap();
    assert_eq!(
        history_hit(r, 50.0, 100.0, a.x + 2.0, 26.0),
        Some(HistoryHit::Action)
    );
    assert_eq!(
        history_hit(r, 50.0, 100.0, 3.0, 26.0),
        Some(HistoryHit::Row)
    );
    assert_eq!(history_hit(r, 50.0, 100.0, 3.0, 60.0), None);
}
