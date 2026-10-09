use super::*;

#[test]
fn island_geometry_invariants() {
    const {
        assert!(TAB_INSET_Y * 2.0 < ISLAND_HEIGHT);
        assert!(CLOSE_MARGIN_RIGHT + CLOSE_HIT_HALF_WIDTH < CLOSE_MIN_ISLAND_WIDTH);
        assert!(CLOSE_HOVER_HALF * 2.0 <= ISLAND_HEIGHT - TAB_INSET_Y * 2.0);
    }
}

/// Lone tab hugs its content — not a full-strip or fixed natural bar.
#[test]
fn single_tab_uses_content_hug_width() {
    let natural = tab_slot_width_for_content(80.0, false, false);
    let layout = tab_strip_layout_from_widths(1600.0, 2.0, 240.0, &[natural]);
    assert_eq!(layout.width_at(0), natural.max(MIN_TAB_WIDTH));
    assert_eq!(layout.tabs_width(), layout.width_at(0));
    let logical_w = 800.0;
    assert!(layout.left_margin + layout.tabs_width() < logical_w / 2.0);
}

#[test]
fn content_hug_keeps_tabs_different_widths() {
    let short = tab_slot_width_for_content(40.0, false, false);
    let long = tab_slot_width_for_content(120.0, true, true);
    let layout = tab_strip_layout_from_widths(3000.0, 2.0, 240.0, &[short, long]);
    assert!(layout.width_at(0) < layout.width_at(1));
    assert_eq!(layout.tabs_width(), layout.width_at(0) + layout.width_at(1));
}

#[test]
fn island_rect_insets_slot_and_clamps_radius() {
    // Slot at x=100, width 180 → island inset by half the gap on
    // each side and TAB_INSET_Y vertically.
    let (x, y, w, h, radius) = island_rect(100.0, 180.0);
    assert_eq!(x, 100.0 + TAB_GAP / 2.0);
    assert_eq!(y, TAB_INSET_Y);
    assert_eq!(w, 180.0 - TAB_GAP);
    assert_eq!(h, ISLAND_HEIGHT - TAB_INSET_Y * 2.0);
    assert_eq!(radius, TAB_RADIUS);

    let (_, _, w, h, radius) = island_rect(0.0, 4.0);
    assert_eq!(w, 0.0);
    assert_eq!(radius, 0.0);
    assert!(radius <= h / 2.0);
}

#[test]
fn island_fills_adapt_to_background_luminance() {
    let dark = island_fills([0.06, 0.05, 0.06, 1.0]);
    let light = island_fills([0.98, 0.98, 0.97, 1.0]);
    // Dark themes: inactive pills are solid (readable); active is brighter.
    assert!(dark.inactive[3] >= 0.9);
    assert!(dark.active[0] > dark.inactive[0]);
    assert_eq!(light.inactive[0], 0.0);
    // On light themes the active island must read as the brighter,
    // elevated card: a strong white overlay against the recessed
    // black-tinted inactive fill.
    assert_eq!(light.active[0], 1.0);
    assert!(light.active[3] >= 0.8);
    // Both themes keep a hairline so tabs read as separate pills.
    assert!(light.outline.is_some());
    assert!(dark.outline.is_some());
}

#[test]
fn over_composites_source_over_destination() {
    let dst = [0.2, 0.4, 0.6, 1.0];
    let out = over(dst, [1.0, 1.0, 1.0, 0.25]);
    assert!((out[0] - 0.4).abs() < 1e-6);
    assert!((out[1] - 0.55).abs() < 1e-6);
    assert!((out[2] - 0.7).abs() < 1e-6);
    assert_eq!(out[3], 1.0);
    // Zero-alpha source is a no-op; full-alpha replaces.
    assert_eq!(over(dst, [0.9, 0.1, 0.3, 0.0]), dst);
    assert_eq!(over(dst, [0.9, 0.1, 0.3, 1.0]), [0.9, 0.1, 0.3, 1.0]);
}

#[test]
fn test_island_initialization() {
    let inactive_color = [0.5, 0.5, 0.5, 1.0];
    let active_color = [0.9, 0.9, 0.9, 1.0];

    let island = Island::new(inactive_color, active_color, true, 240.0);

    assert_eq!(island.inactive_text_color, inactive_color);
    assert_eq!(island.active_text_color, active_color);
    assert!(island.hide_if_single);
}

#[test]
fn test_island_height() {
    let island = Island::new([0.8, 0.8, 0.8, 1.0], [1.0, 1.0, 1.0, 1.0], false, 240.0);
    assert_eq!(island.height(), ISLAND_HEIGHT);
}

fn test_island() -> Island {
    Island::new([0.5, 0.5, 0.5, 1.0], [0.9, 0.9, 0.9, 1.0], false, 240.0)
}

#[test]
fn progress_first_report_seeds_started_and_seen() {
    let mut island = test_island();
    island.set_progress_report(ProgressReport {
        state: ProgressState::Indeterminate,
        progress: None,
    });
    assert!(island.progress_started_at.is_some());
    assert!(island.progress_last_seen.is_some());
    assert_eq!(island.progress_state, Some(ProgressState::Indeterminate));
}

#[test]
fn progress_repeated_same_state_keeps_started_at_stable() {
    // Issue #1509: a TUI that heartbeats `OSC 9;4;3` (or any same-state
    // report) must NOT restart the indeterminate animation phase, or the
    // pulsing block snaps back to the left edge on every report.
    let mut island = test_island();
    island.set_progress_report(ProgressReport {
        state: ProgressState::Indeterminate,
        progress: None,
    });
    let first_started = island.progress_started_at.unwrap();
    let first_seen = island.progress_last_seen.unwrap();

    // Sleep so a subsequent Instant::now() is observably later — the
    // started_at field must stay equal while last_seen advances.
    std::thread::sleep(std::time::Duration::from_millis(15));
    island.set_progress_report(ProgressReport {
        state: ProgressState::Indeterminate,
        progress: None,
    });

    assert_eq!(
        island.progress_started_at,
        Some(first_started),
        "started_at must not move on a same-state heartbeat"
    );
    assert!(
        island.progress_last_seen.unwrap() > first_seen,
        "last_seen must advance on every report"
    );
}

#[test]
fn progress_state_transition_resets_started_at() {
    // Set → Indeterminate is a real state change, so the animation
    // anchor should be reseated. (Set has no animation, but the
    // started_at field still becomes meaningful as soon as we hit
    // Indeterminate.)
    let mut island = test_island();
    island.set_progress_report(ProgressReport {
        state: ProgressState::Set,
        progress: Some(50),
    });
    let first = island.progress_started_at.unwrap();

    std::thread::sleep(std::time::Duration::from_millis(15));
    island.set_progress_report(ProgressReport {
        state: ProgressState::Indeterminate,
        progress: None,
    });

    assert!(
        island.progress_started_at.unwrap() > first,
        "transitioning into a new state must move started_at forward"
    );
    assert_eq!(island.progress_state, Some(ProgressState::Indeterminate));
}

#[test]
fn progress_set_value_change_does_not_reseat_started_at() {
    // Same `Set` state with a different percentage is still the same
    // state — only the value updates. started_at stays put; the bar
    // just redraws at the new fraction.
    let mut island = test_island();
    island.set_progress_report(ProgressReport {
        state: ProgressState::Set,
        progress: Some(20),
    });
    let first = island.progress_started_at.unwrap();

    std::thread::sleep(std::time::Duration::from_millis(15));
    island.set_progress_report(ProgressReport {
        state: ProgressState::Set,
        progress: Some(60),
    });

    assert_eq!(island.progress_started_at, Some(first));
    assert_eq!(island.progress_value, Some(60));
}

/// Each char = 1.0 wide, including the ellipsis. Easy arithmetic.
fn fixed_unit_width(_c: char) -> f32 {
    1.0
}

fn rendered_width(s: &str, char_width: impl FnMut(char) -> f32) -> f32 {
    s.chars().map(char_width).sum()
}

#[test]
fn title_fits_is_returned_unchanged() {
    assert_eq!(
        fit_title_with_widths("hello", 10.0, fixed_unit_width),
        "hello"
    );
    assert_eq!(fit_title_with_widths("hi", 2.0, fixed_unit_width), "hi");
}

#[test]
fn title_that_fits_borrows_without_allocating() {
    // Confirms the zero-allocation "no truncation" hot path: when the
    // full title fits, the returned Cow must stay Borrowed so the
    // render loop doesn't allocate a new String every frame.
    let out = fit_title_with_widths("ok", 10.0, fixed_unit_width);
    assert!(
        matches!(out, Cow::Borrowed(_)),
        "expected borrowed, got {out:?}"
    );
}

#[test]
fn title_zero_budget_returns_ellipsis() {
    // Historically this was short-circuited to return the full title;
    // now it falls through the loop and returns "…" consistently with
    // tiny-but-positive budgets.
    assert_eq!(fit_title_with_widths("abc", 0.0, fixed_unit_width), "…");
}

#[test]
fn title_overflow_gets_ellipsized_and_fits_budget() {
    // "hello world" budgeted at 5 → best we can do without exceeding
    // is "hell" (4) + "…" (1) = 5. Anything more overflows.
    let out = fit_title_with_widths("hello world", 5.0, fixed_unit_width);
    assert_eq!(out, "hell…");
    assert!(
        rendered_width(&out, fixed_unit_width) <= 5.0,
        "truncated width {} must be ≤ budget 5",
        rendered_width(&out, fixed_unit_width)
    );
}

#[test]
fn title_respects_budget_with_wide_chars() {
    // Mixed widths: 'W' = 2.0, others (including ellipsis) = 1.0.
    // Title "WxWxW", budget 4.0. Walk:
    // ix=0 W: before add, 0+1(suffix) ≤ 4 → truncate_ix=0; accum→2
    // ix=1 x: 2+1 ≤ 4 → truncate_ix=1; accum→3
    // ix=2 W: 3+1 ≤ 4 → truncate_ix=2; accum→5; 5>4 → cut.
    // Output: title[..2] + "…" = "Wx…", width 2+1+1 = 4 ≤ 4 ✓
    let widths = |c: char| if c == 'W' { 2.0 } else { 1.0 };
    let out = fit_title_with_widths("WxWxW", 4.0, widths);
    assert_eq!(out, "Wx…");
    assert!(rendered_width(&out, widths) <= 4.0);
}

#[test]
fn title_truncation_preserves_utf8_boundaries() {
    // Each emoji/char = 2.0 wide; ellipsis = 2.0.
    // Title "🎟🎟🎟" = 6.0. Budget 4.0 → one emoji + "…" = 4.0 ≤ 4 ✓.
    // Crucial: the byte index we cut at must be on a UTF-8 boundary.
    let w = |_c: char| 2.0;
    let out = fit_title_with_widths("🎟🎟🎟", 4.0, w);
    assert_eq!(out, "🎟…");
    assert!(out.chars().count() == 2, "{out:?} should be 2 graphemes");
}

#[test]
fn title_budget_smaller_than_ellipsis_still_returns_ellipsis() {
    // Budget 0.5 < ellipsis_width 1.0: first char overflows, prefix is
    // empty, we return just "…" so the user at least sees *something*
    // indicating truncation rather than a blank tab label.
    let out = fit_title_with_widths("abc", 0.5, fixed_unit_width);
    assert_eq!(out, "…");
}

#[test]
fn title_empty_input_returned_as_is() {
    assert_eq!(fit_title_with_widths("", 10.0, fixed_unit_width), "");
}

#[test]
fn title_exact_fit_not_truncated() {
    // Title "abcd" = 4.0, budget 4.0 → fits exactly, no truncation.
    assert_eq!(fit_title_with_widths("abcd", 4.0, fixed_unit_width), "abcd");
}

#[test]
fn tab_strip_layout_geometry() {
    // Overflow: four equal naturals compress proportionally to fill.
    let natural = 168.0;
    let layout = tab_strip_layout_from_widths(1000.0, 2.0, 240.0, &[natural; 4]);
    let left = island_margin_left();
    let right = island_margin_right();
    assert_eq!(layout.left_margin, left);
    assert_eq!(layout.right_margin, right);
    let expected = (500.0 - right - left) / 4.0;
    assert!((layout.width_at(0) - expected).abs() < 0.01);
    assert!((layout.tabs_width() - expected * 4.0).abs() < 0.01);
    assert!(tab_strip_layout(1000.0, 2.0, 0, 240.0).is_empty());
}

#[test]
fn tab_strip_layout_caps_slot_width() {
    let wide = tab_slot_width_for_content(200.0, true, true);
    let layout = tab_strip_layout_from_widths(3000.0, 2.0, 240.0, &[wide, wide]);
    assert_eq!(layout.width_at(0), 240.0);
    assert_eq!(layout.tabs_width(), 480.0);
    assert!(layout.left_margin + layout.tabs_width() < 1500.0);

    let layout = tab_strip_layout_from_widths(10.0, 2.0, 240.0, &[wide; 4]);
    assert_eq!(layout.width_at(0), 0.0);
    assert_eq!(layout.tabs_width(), 0.0);
}

#[test]
fn remap_tab_move_forward_rotates_indices() {
    // Move tab 1 → 3: tabs 2 and 3 shift left by one.
    assert_eq!(Island::remap_index(1, 1, 3), 3);
    assert_eq!(Island::remap_index(2, 1, 3), 1);
    assert_eq!(Island::remap_index(3, 1, 3), 2);
    assert_eq!(Island::remap_index(0, 1, 3), 0);
    assert_eq!(Island::remap_index(4, 1, 3), 4);
}

#[test]
fn remap_tab_move_backward_rotates_indices() {
    // Move tab 3 → 0: tabs 0, 1, 2 shift right by one.
    assert_eq!(Island::remap_index(3, 3, 0), 0);
    assert_eq!(Island::remap_index(0, 3, 0), 1);
    assert_eq!(Island::remap_index(1, 3, 0), 2);
    assert_eq!(Island::remap_index(2, 3, 0), 3);
    assert_eq!(Island::remap_index(4, 3, 0), 4);
}

#[test]
fn remap_tab_move_carries_picker_and_springs() {
    let mut island = test_island();
    island.color_picker_tab = Some(3);

    // Tab 1 → 3 (rotate): the open picker shifts 3 → 2. Per-tab colors
    // and titles now live on the tab in ContextManager (see
    // context::test::test_custom_color_* / test_custom_title_*), so they
    // no longer need remapping here.
    island.remap_tab_move(1, 3, 100.0);
    assert_eq!(island.color_picker_tab, Some(2));

    // Displaced tabs (now at 1 and 2) got slide springs of +width.
    assert_eq!(island.slide_springs.len(), 2);
    assert_eq!(island.slide_springs.get(&1).unwrap().position, 100.0);
    assert_eq!(island.slide_springs.get(&2).unwrap().position, 100.0);
}

#[test]
fn drag_threshold_gates_start() {
    let mut island = test_island();
    island.start_drag(0, 10.0, 50.0);
    assert!(island.is_dragging());
    assert_eq!(island.drag_index(), None, "not started below threshold");
    assert!(!island.update_drag(52.0));
    assert!(island.update_drag(58.0), "8px exceeds threshold");
    assert_eq!(island.drag_index(), Some(0));
    island.cancel_drag();
    assert!(!island.is_dragging());
}

fn test_layout() -> TabStripLayout {
    TabStripLayout {
        left_margin: 0.0,
        right_margin: ISLAND_MARGIN_RIGHT,
        widths: smallvec::smallvec![100.0, 100.0, 100.0, 100.0],
    }
}

#[test]
fn close_button_anchors_to_island_right_edge() {
    // Full-width slot: slot 1 spans 180..360, island 183..354, so
    // the button centers at 354 - CLOSE_MARGIN_RIGHT.
    let layout = TabStripLayout {
        left_margin: 0.0,
        right_margin: ISLAND_MARGIN_RIGHT,
        widths: smallvec::smallvec![180.0, 180.0],
    };
    let cx = close_button_center_x(&layout, 1).unwrap();
    assert_eq!(
        cx,
        180.0 + TAB_GAP / 2.0 + (180.0 - TAB_GAP) - CLOSE_MARGIN_RIGHT
    );
    // The whole forgiving hit box stays inside the island.
    assert!(cx + CLOSE_HIT_HALF_WIDTH <= 360.0 - TAB_GAP / 2.0);

    // Narrow islands (many tabs) drop the button — no hit box, so
    // rendering and click handling agree via the shared helper.
    let narrow = TabStripLayout {
        left_margin: 0.0,
        right_margin: ISLAND_MARGIN_RIGHT,
        widths: smallvec::smallvec![60.0; 10],
    };
    assert_eq!(close_button_center_x(&narrow, 3), None);
}

#[test]
fn close_hit_box_clears_the_title_budget() {
    // Left-padded title on slot 0 ends at island_x + TAB_PADDING_X +
    // max_text; the close hit box must start at or after that point.
    let layout = TabStripLayout {
        left_margin: 0.0,
        right_margin: ISLAND_MARGIN_RIGHT,
        widths: smallvec::smallvec![180.0, 180.0],
    };
    let cx = close_button_center_x(&layout, 0).unwrap();
    let island_x = TAB_GAP / 2.0;
    let max_text = (layout.width_at(0)
        - TAB_GAP
        - TAB_PADDING_X
        - CLOSE_MARGIN_RIGHT
        - CLOSE_HIT_HALF_WIDTH)
        .max(0.0);
    let title_max_right = island_x + TAB_PADDING_X + max_text;
    assert!(cx - CLOSE_HIT_HALF_WIDTH >= title_max_right);
}

#[test]
fn drag_center_clamps_to_strip() {
    let mut island = test_island();
    // Tab 0 grabbed 10px from its left edge, tabs region spans
    // 0..400 with 100-wide slots.
    island.start_drag(0, 10.0, 50.0);
    island.update_drag(200.0); // started
    let center = island.drag_center(&test_layout()).unwrap();
    assert_eq!(center, 190.0 + 50.0);

    // Dragged far right: floating left clamps to 300, center 350.
    island.update_drag(1000.0);
    assert_eq!(island.drag_center(&test_layout()), Some(350.0));

    // Far left: clamps to 0, center 50.
    island.update_drag(-500.0);
    assert_eq!(island.drag_center(&test_layout()), Some(50.0));
}

#[test]
fn end_drag_seeds_settle_spring() {
    let mut island = test_island();
    island.start_drag(2, 0.0, 200.0);
    island.update_drag(250.0); // floating left = 250, slot x = 200
    island.end_drag(&test_layout());
    assert!(!island.is_dragging());
    let spring = island.slide_springs.get(&2).unwrap();
    assert_eq!(spring.position, 50.0);
}

#[test]
fn progress_remove_clears_all_progress_state() {
    let mut island = test_island();
    island.set_progress_report(ProgressReport {
        state: ProgressState::Set,
        progress: Some(50),
    });
    island.set_progress_report(ProgressReport {
        state: ProgressState::Remove,
        progress: None,
    });
    assert!(island.progress_state.is_none());
    assert!(island.progress_value.is_none());
    assert!(island.progress_started_at.is_none());
    assert!(island.progress_last_seen.is_none());
}
