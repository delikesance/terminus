use super::*;
use crate::connection::{NodeVisual, TRACK_LINE_HEIGHT};
use crate::geom::Rect;
use crate::theme::ChromeTheme;
use std::time::Duration;

fn theme() -> ChromeTheme {
    ChromeTheme::default()
}

fn s(n: u64) -> Duration {
    Duration::from_secs(n)
}

#[test]
fn dot_is_8px_and_centred_in_row() {
    let r = status_dot_rect(10.0, 100.0, 20.0);
    assert_eq!((r.width, r.height), (8.0, 8.0));
    assert_eq!(r.y, 106.0);
    assert_eq!(status_label_x(10.0), 28.0);
}

#[test]
fn idle_is_hollow_ring_others_filled() {
    let t = theme();
    assert_eq!(
        status_dot_paint(StatusKind::Idle, &t),
        DotPaint::Ring {
            color: t.idle_ring,
            width: 1.5
        }
    );
    assert_eq!(
        status_dot_paint(StatusKind::Running, &t),
        DotPaint::Fill(t.success)
    );
    assert_eq!(
        status_dot_paint(StatusKind::Connected, &t),
        DotPaint::Fill(t.accent)
    );
    assert_eq!(
        status_dot_paint(StatusKind::Warning, &t),
        DotPaint::Fill(t.warning)
    );
    assert_eq!(
        status_dot_paint(StatusKind::Error, &t),
        DotPaint::Fill(t.danger_fill)
    );
}

#[test]
fn step_paint_matches_board() {
    let t = theme();
    assert_eq!(step_paint(StepState::Pending, &t).fill, t.surface);
    let a = step_paint(StepState::Active, &t);
    assert_eq!(a.fill, t.accent);
    assert_eq!(a.glyph, t.on_accent);
    assert_eq!(a.halo.unwrap()[3], 0.2);
    assert_eq!(step_paint(StepState::Done, &t).fill, t.step_done_bg);
    assert_eq!(step_paint(StepState::Done, &t).glyph, t.success);
    assert_eq!(step_paint(StepState::Failed, &t).fill, t.step_failed_bg);
    assert!(step_paint(StepState::Pending, &t).halo.is_none());
}

#[test]
fn steps_at_marks_done_active_pending_and_failed() {
    use StepState::*;
    assert_eq!(steps_at(2, false), [Done, Done, Active, Pending]);
    assert_eq!(steps_at(2, true), [Done, Done, Failed, Pending]);
    assert_eq!(StepState::from(NodeVisual::Done), Done);
}

#[test]
fn step_line_nodes_40px_segments_join_edges() {
    let line = StepLine::from_centers(&[50.0, 150.0, 250.0, 350.0], 100.0);
    for n in line.nodes {
        assert_eq!((n.width, n.height), (40.0, 40.0));
        assert_eq!(n.y, 80.0);
    }
    assert_eq!(line.segments[0].x, line.nodes[0].right());
    assert_eq!(line.segments[0].right(), line.nodes[1].x);
    assert_eq!(line.segments[0].height, TRACK_LINE_HEIGHT);
    assert_eq!(line.segments[0].y, 100.0 - TRACK_LINE_HEIGHT / 2.0);
    assert_eq!(line.halo(1).width, 52.0);
}

#[test]
fn step_line_accepts_connection_layout() {
    let (w, cx) = crate::connection::track_layout_from_labels(&[40.0; 4]);
    let line = StepLine::from_centers(&cx.map(|c| c + 100.0), 0.0);
    assert!(line.nodes[3].right() <= 100.0 + w);
    assert!(line.segments.iter().all(|s| s.width > 0.0));
}

#[test]
fn evenly_spaces_four_nodes() {
    let line = StepLine::evenly(0.0, 400.0, 20.0);
    assert_eq!(line.nodes[0].x + 20.0, 50.0);
    assert_eq!(line.nodes[3].x + 20.0, 350.0);
}

#[test]
fn segment_lit_after_done_node() {
    use StepState::*;
    let st = [Done, Active, Pending, Pending];
    assert!(StepLine::segment_lit(&st, 0));
    assert!(!StepLine::segment_lit(&st, 1));
}

#[test]
fn progress_heights_and_fraction_clamp() {
    assert_eq!(ProgressKind::Transfer.bar_height(), 4.0);
    assert_eq!(ProgressKind::Update.bar_height(), 6.0);
    assert_eq!(ProgressKind::Failed.bar_height(), 4.0);
    let tr = progress_track_rect(10.0, 10.0, 200.0, ProgressKind::Transfer);
    assert_eq!(progress_fill_rect(&tr, 0.62).width, 124.0);
    assert_eq!(progress_fill_rect(&tr, 2.0).width, 200.0);
    assert_eq!(progress_fill_rect(&tr, -1.0).width, 0.0);
    assert_eq!(progress_fill_rect(&tr, f32::NAN).width, 0.0);
    let cap = progress_caption_rect(&tr);
    assert_eq!(cap.y, 22.0);
    assert_eq!(progress_height(ProgressKind::Update), 6.0 + 8.0 + 16.0);
}

#[test]
fn failed_bar_uses_danger() {
    let t = theme();
    assert_eq!(progress_fill_color(ProgressKind::Failed, &t), t.danger_fill);
    assert_eq!(progress_fill_color(ProgressKind::Transfer, &t), t.accent);
}

#[test]
fn errors_persist_others_expire() {
    let mk = |k| Toast::new(k, "t", "b", vec![], s(10));
    let e = mk(ToastKind::Error);
    assert!(e.is_persistent());
    assert!(!e.is_expired(s(100_000)));
    let i = mk(ToastKind::Info);
    assert!(!i.is_expired(s(14)));
    assert!(i.is_expired(s(15)));
    assert!(!i.is_expired(s(5))); // clock before creation
    let w = mk(ToastKind::Warning);
    assert!(!w.is_expired(s(17)));
    assert!(w.is_expired(s(18)));
}

#[test]
fn toast_colors_match_board() {
    let t = theme();
    let c = ToastKind::Error.colors(&t);
    assert_eq!(c.bg, hex(0x2A, 0x16, 0x1C));
    assert_eq!(c.dot, t.danger_fill);
    assert_eq!(ToastKind::Info.colors(&t).border, hex(0x25, 0x3F, 0x5A));
    assert_eq!(ToastKind::Success.colors(&t).dot, t.success);
    assert_eq!(ToastKind::Warning.colors(&t).body, hex(0xE9, 0xDC, 0xC2));
}

#[test]
fn wrap_breaks_on_width() {
    let m = |s: &str| s.chars().count() as f32 * 10.0;
    assert_eq!(wrap_lines("aa bb cc", 50.0, m), vec!["aa bb", "cc"]);
    assert_eq!(wrap_lines("", 50.0, m), vec![""]);
    assert_eq!(wrap_lines("longword x", 20.0, m), vec!["longword", "x"]);
}

#[test]
fn toast_text_width_is_298_by_default() {
    let t = Toast::new(ToastKind::Info, "a", "b", vec![], s(0));
    assert_eq!(t.width, 400.0);
    assert_eq!(t.text_width(), 298.0);
}

#[test]
fn toast_layout_geometry_and_hits() {
    let t = Toast::new(
        ToastKind::Error,
        "Couldn't reach",
        "body",
        vec!["Try again".into(), "Edit server".into()],
        s(0),
    );
    let l = toast_layout(&t, 100.0, 200.0, 2, &[60.0, 70.0]);
    assert_eq!(l.rect.height, toast_height(2, true));
    assert_eq!(l.dismiss.width, 30.0);
    assert_eq!(l.dismiss.right(), l.rect.right() - 18.0);
    assert_eq!(l.dot.x, 118.0);
    assert_eq!(l.title.x, 118.0 + 8.0 + 14.0);
    assert_eq!(l.body_lines.len(), 2);
    assert_eq!(l.actions[1].x, l.actions[0].right() + 14.0);
    let c = |r: Rect| (r.x + r.width / 2.0, r.y + r.height / 2.0);
    let (x, y) = c(l.dismiss);
    assert_eq!(l.hit_test(x, y), Some(ToastHit::Dismiss));
    let (x, y) = c(l.actions[1]);
    assert_eq!(l.hit_test(x, y), Some(ToastHit::Action(1)));
    let (x, y) = c(l.title);
    assert_eq!(l.hit_test(x, y), Some(ToastHit::Body));
    assert_eq!(l.hit_test(0.0, 0.0), None);
    assert!(l.actions[0].bottom() <= l.rect.bottom() - 18.0 + 0.01);
}

#[test]
fn toast_without_actions_is_shorter() {
    assert!(toast_height(1, false) < toast_height(1, true));
    assert!(toast_height(2, false) > toast_height(1, false));
}

#[test]
fn stack_grows_upward_from_bottom_right() {
    let r = stack_rects(1000.0, 800.0, 400.0, &[100.0, 80.0]);
    assert_eq!(r[0], Rect::new(580.0, 680.0, 400.0, 100.0));
    assert_eq!(r[1].bottom(), r[0].y - TOAST_STACK_GAP);
    assert_eq!(r[1].x, r[0].x);
    assert!(stack_rects(1000.0, 800.0, 400.0, &[]).is_empty());
}
