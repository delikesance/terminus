use super::*;
use crate::geom::Rect;

fn fake_measure(s: &str) -> f32 {
    s.chars().count() as f32 * 7.0
}

#[test]
fn action_row_right_aligns_confirm_and_gaps_cancel() {
    let dialog = Rect::new(100.0, 50.0, 400.0, 300.0);
    let (cancel, confirm) = action_row(&dialog, 30.0, 44.0, 10.0, 70.0, 90.0);
    assert_eq!(confirm, Rect::new(380.0, 276.0, 90.0, 44.0));
    assert_eq!(cancel, Rect::new(300.0, 276.0, 70.0, 44.0));
}

#[test]
fn dialog_is_centred_in_window() {
    let l = dialog_layout((1000.0, 700.0), DialogKind::Confirm, 2, 90.0, 80.0);
    assert_eq!(l.dialog.width, DIALOG_WIDTH);
    let cx = l.dialog.x + l.dialog.width / 2.0;
    let cy = l.dialog.y + l.dialog.height / 2.0;
    assert!((cx - 500.0).abs() < 0.5 && (cy - 350.0).abs() < 0.5);
    assert_eq!(l.scrim, Rect::new(0.0, 0.0, 1000.0, 700.0));
}

#[test]
fn dialog_actions_are_right_aligned_inside_padding() {
    let l = dialog_layout((1000.0, 700.0), DialogKind::Confirm, 1, 90.0, 80.0);
    assert!((l.confirm.right() - (l.dialog.right() - DIALOG_PAD)).abs() < 0.01);
    assert!((l.cancel.right() + ACTION_GAP - l.confirm.x).abs() < 0.01);
    assert_eq!(l.confirm.height, ACTION_HEIGHT);
    assert!((l.confirm.bottom() - (l.dialog.bottom() - DIALOG_PAD)).abs() < 0.01);
    assert!(l.cancel.x < l.confirm.x);
}

#[test]
fn dialog_grows_with_body_lines_and_option_row() {
    let a = dialog_layout((1000.0, 700.0), DialogKind::Confirm, 1, 90.0, 80.0);
    let b = dialog_layout((1000.0, 700.0), DialogKind::Confirm, 3, 90.0, 80.0);
    assert!((b.dialog.height - a.dialog.height - 2.0 * BODY_LINE).abs() < 0.01);
    let c = dialog_layout((1000.0, 700.0), DialogKind::WithOption, 1, 90.0, 80.0);
    assert!(c.option.is_some() && a.option.is_none());
    assert!(c.dialog.height > a.dialog.height);
    let o = c.option.unwrap();
    assert!(o.bottom() <= c.confirm.y && o.y >= c.body.bottom());
}

#[test]
fn dialog_hit_distinguishes_buttons_option_inside_and_scrim() {
    let l = dialog_layout((1000.0, 700.0), DialogKind::WithOption, 1, 90.0, 80.0);
    let mid = |r: Rect| (r.x + r.width / 2.0, r.y + r.height / 2.0);
    let (x, y) = mid(l.confirm);
    assert_eq!(l.hit_test(x, y), DialogHit::Confirm);
    let (x, y) = mid(l.cancel);
    assert_eq!(l.hit_test(x, y), DialogHit::Cancel);
    let (x, y) = mid(l.option.unwrap());
    assert_eq!(l.hit_test(x, y), DialogHit::Option);
    assert_eq!(
        l.hit_test(l.dialog.x + 2.0, l.dialog.y + 2.0),
        DialogHit::Inside
    );
    assert_eq!(l.hit_test(1.0, 1.0), DialogHit::Scrim);
}

#[test]
fn escape_always_cancels() {
    for kind in [
        DialogKind::Confirm,
        DialogKind::Destructive,
        DialogKind::WithOption,
    ] {
        for focus in [DialogFocus::Cancel, DialogFocus::Confirm] {
            assert_eq!(
                dialog_key(DialogKey::Escape, focus),
                DialogOutcome::Cancel,
                "{kind:?}"
            );
        }
    }
}

#[test]
fn enter_activates_the_focused_button_and_destructive_defaults_to_cancel() {
    assert_eq!(
        dialog_key(DialogKey::Enter, DialogFocus::Confirm),
        DialogOutcome::Confirm
    );
    assert_eq!(
        dialog_key(DialogKey::Enter, DialogFocus::Cancel),
        DialogOutcome::Cancel
    );
    assert_eq!(DialogKind::Confirm.default_focus(), DialogFocus::Confirm);
    assert_eq!(DialogKind::WithOption.default_focus(), DialogFocus::Confirm);
    assert_eq!(DialogKind::Destructive.default_focus(), DialogFocus::Cancel);
    assert_eq!(
        dialog_key(DialogKey::Tab, DialogFocus::Cancel),
        DialogOutcome::Focus(DialogFocus::Confirm)
    );
    assert_eq!(
        dialog_key(DialogKey::Tab, DialogFocus::Confirm),
        DialogOutcome::Focus(DialogFocus::Cancel)
    );
}

#[test]
fn wrap_breaks_on_words_and_respects_width() {
    let lines = wrap_text("alpha beta gamma delta", 7.0 * 11.0, fake_measure);
    assert_eq!(lines, vec!["alpha beta", "gamma delta"]);
    assert_eq!(wrap_text("", 100.0, fake_measure), vec![String::new()]);
    // An overlong word stays on its own line rather than looping.
    assert_eq!(
        wrap_text("supercalifragilistic x", 20.0, fake_measure).len(),
        2
    );
}

// ---- stepper ----
#[test]
fn stepper_fills_up_to_current_step_and_marks_current_label() {
    let segs = stepper_segments(Rect::new(10.0, 20.0, 380.0, 30.0), 2);
    assert_eq!(segs.len(), 3);
    assert_eq!(
        segs.iter().map(|s| s.filled).collect::<Vec<_>>(),
        [true, true, false]
    );
    assert_eq!(
        segs.iter().map(|s| s.current).collect::<Vec<_>>(),
        [false, true, false]
    );
    assert_eq!(
        segs.iter().map(|s| s.label).collect::<Vec<_>>(),
        ["Address", "Sign in", "Organise"]
    );
}

#[test]
fn stepper_bars_are_equal_3px_with_8px_gaps_spanning_width() {
    let r = Rect::new(10.0, 20.0, 380.0, 30.0);
    let segs = stepper_segments(r, 1);
    assert!(segs
        .iter()
        .all(|s| s.bar.height == STEPPER_BAR_HEIGHT && s.bar.y == 20.0));
    assert!((segs[1].bar.x - segs[0].bar.right() - STEPPER_GAP).abs() < 0.01);
    assert!((segs[2].bar.right() - r.right()).abs() < 0.01);
    assert!((segs[0].bar.width - segs[2].bar.width).abs() < 0.01);
    assert!(segs[0].label_rect.y > segs[0].bar.bottom());
    assert_eq!(
        STEPPER_HEIGHT,
        STEPPER_BAR_HEIGHT + STEPPER_LABEL_GAP + STEPPER_LABEL_HEIGHT
    );
}

#[test]
fn stepper_step_is_clamped() {
    assert!(stepper_segments(Rect::new(0.0, 0.0, 300.0, 30.0), 0)
        .iter()
        .all(|s| !s.filled && !s.current));
    assert!(stepper_segments(Rect::new(0.0, 0.0, 300.0, 30.0), 9)
        .iter()
        .all(|s| s.filled));
}
