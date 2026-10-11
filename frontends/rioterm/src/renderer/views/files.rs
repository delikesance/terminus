//! Files view painter and input: two SFTP panes and a transfer bar, rendered
//! from the existing [`SftpPaneState`] and driven through the existing
//! [`ActiveSftp`] actions (navigate, upload/download by drag, rename, mkdir,
//! delete, conflicts, Local|Host and Host|Host).
//!
//! Geometry lives in `terminus_ui::views::files`; rows are the List
//! component's file rows, the bar uses the Feedback progress geometry, the
//! Cancel and conflict buttons are Button components and the conflict prompt
//! is the Overlay dialog.

use rio_backend::sugarloaf::text::CoverageMask;
use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::components::button::{ButtonKind, ButtonSize, ButtonState};
use terminus_ui::components::feedback::{
    progress_fill_color, progress_fill_rect, ProgressKind,
};
use terminus_ui::components::list::{
    file_row_layout, FileKind, FileRowState, FILE_RENAME_HEIGHT,
};
use terminus_ui::components::overlay::{
    self as ov, DialogFocus, DialogHit, DialogKey, DialogKind, DialogLayout,
    DialogOutcome,
};
use terminus_ui::geom::Rect;
use terminus_ui::icons::{Icon, IconPlacement};
use terminus_ui::sftp_pane::{
    SftpConflictKind, SftpFocus, SftpNameKind, SftpPaneState, SftpRow,
};
use terminus_ui::theme::{unit_color, ChromeTheme};
use terminus_ui::tokens::font_size;
use terminus_ui::views::files::{
    self as geo, drop_target, modified_label, pane_path, pane_title, row_offset,
    size_label, FilesHit, FilesLayout,
};

use crate::renderer::chrome::{draw_icon, paint_flat};
use crate::renderer::components::button::{label_spec, paint_button};
use crate::renderer::components::overlay::{dialog_layout_for, paint_dialog, DialogSpec};
use crate::renderer::ui_text::{
    draw_mono_text, draw_ui_text, measure_mono_text, measure_ui_text, UiWeight,
};
use crate::sftp_ui::ActiveSftp;

// Depth layers within the view.
const D_FRAME: f32 = 0.0;
const D_ROW: f32 = 0.1;
const D_CTRL: f32 = 0.2;
const D_GHOST: f32 = 0.4;
const SCRIM_ORDER: u8 = 29;

/// View-local interaction state (everything else lives in [`SftpPaneState`]).
#[derive(Debug, Clone, Default)]
pub struct FilesView {
    /// What the pointer is over.
    pub hover: Option<FilesHit>,
    /// Keyboard focus inside the conflict dialog.
    pub dialog_focus: Option<DialogFocus>,
}

mod search;
use search::paint_search;

/// What the app should do after a Files input.
pub enum FilesAction {
    /// Nothing changed.
    None,
    /// UI state changed: call `Route::request_overlay_redraw`.
    Redraw,
    /// Open this context menu (the app owns menu painting/hit-testing).
    ContextMenu(terminus_ui::ActionMenu),
}

impl FilesAction {
    pub fn needs_redraw(&self) -> bool {
        !matches!(self, FilesAction::None)
    }
}

fn text_top(center: f32, size: f32) -> f32 {
    center - size * 0.62
}

fn rrect(s: &mut Sugarloaf, r: &Rect, color: [f32; 4], rad: f32, depth: f32) {
    s.rounded_rect(None, r.x, r.y, r.width, r.height, color, depth, rad, 0);
}

fn blend(bg: [f32; 4], fg: [f32; 4], alpha: f32) -> [f32; 4] {
    [
        bg[0] + (fg[0] - bg[0]) * alpha,
        bg[1] + (fg[1] - bg[1]) * alpha,
        bg[2] + (fg[2] - bg[2]) * alpha,
        1.0,
    ]
}

/// Trim `text` (keeping the head, or the tail when `keep_tail`) with an
/// ellipsis until `measure` fits `max_w`.
fn fit(
    text: &str,
    max_w: f32,
    keep_tail: bool,
    measure: &mut dyn FnMut(&str) -> f32,
) -> String {
    if measure(text) <= max_w {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let (mut lo, mut hi) = (0usize, chars.len());
    let build = |n: usize| -> String {
        if keep_tail {
            format!(
                "\u{2026}{}",
                chars[chars.len() - n..].iter().collect::<String>()
            )
        } else {
            format!("{}\u{2026}", chars[..n].iter().collect::<String>())
        }
    };
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if measure(&build(mid)) <= max_w {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    build(lo)
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn home_dir() -> Option<String> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
}

// --------------------------------------------------------------- up arrow

/// Lucide has the arrow only as `arrow-up`; the mock's "up one folder" glyph
/// (`M12 19V5M6 11l6-6 6 6`) is the same shape, rasterized here so the shared
/// generated icon table stays untouched.
const UP_ARROW_ID: u64 = 0xF11E_5000_0001;

fn rasterize_up_arrow(size: u16) -> Option<CoverageMask> {
    let unit = IconPlacement::unit(size);
    let mut b = tiny_skia::PathBuilder::new();
    for (cmd, x, y) in [
        ('M', 12.0, 19.0),
        ('L', 12.0, 5.0),
        ('M', 6.0, 11.0),
        ('L', 12.0, 5.0),
        ('L', 18.0, 11.0),
    ] {
        if cmd == 'M' {
            b.move_to(x * unit, y * unit);
        } else {
            b.line_to(x * unit, y * unit);
        }
    }
    let path = b.finish()?;
    let mut pixmap = tiny_skia::Pixmap::new(size as u32, size as u32)?;
    let mut paint = tiny_skia::Paint {
        anti_alias: true,
        ..Default::default()
    };
    paint.set_color_rgba8(255, 255, 255, 255);
    let stroke = tiny_skia::Stroke {
        width: 2.0 * unit,
        line_cap: tiny_skia::LineCap::Round,
        line_join: tiny_skia::LineJoin::Round,
        ..tiny_skia::Stroke::default()
    };
    pixmap.stroke_path(
        &path,
        &paint,
        &stroke,
        tiny_skia::Transform::identity(),
        None,
    );
    let bytes = pixmap.pixels().iter().map(|p| p.alpha()).collect();
    CoverageMask::new(size, bytes)
}

fn draw_up_arrow(s: &mut Sugarloaf, rect: &Rect, color: [u8; 4]) {
    let scale = s.scale_factor();
    let place = IconPlacement::new(
        rect.x + (rect.width - geo::ICON_BTN_ICON) / 2.0,
        rect.y + (rect.height - geo::ICON_BTN_ICON) / 2.0,
        geo::ICON_BTN_ICON,
    );
    let size = place.device_size(scale);
    s.text_mut().draw_mask(
        place.x,
        place.y,
        UP_ARROW_ID,
        size,
        color,
        rasterize_up_arrow,
    );
}

// ------------------------------------------------------------------ paint

fn kind_of(row: &SftpRow) -> FileKind {
    if row.is_dir {
        FileKind::Folder
    } else {
        FileKind::File
    }
}

fn paint_icon_button(
    s: &mut Sugarloaf,
    theme: &ChromeTheme,
    rect: &Rect,
    hovered: bool,
    up_arrow: bool,
) {
    if hovered {
        rrect(s, rect, theme.surface, geo::ICON_BTN_RADIUS, D_CTRL);
    }
    let color = if hovered {
        theme.text
    } else {
        theme.text_muted
    };
    if up_arrow {
        draw_up_arrow(s, rect, color);
    } else {
        let scale = s.scale_factor();
        draw_icon(
            s,
            Icon::FolderPlus,
            IconPlacement::new(
                rect.x + (rect.width - geo::ICON_BTN_ICON) / 2.0,
                rect.y + (rect.height - geo::ICON_BTN_ICON) / 2.0,
                geo::ICON_BTN_ICON,
            ),
            unit_color(color),
            scale,
        );
    }
}

fn paint_caret(s: &mut Sugarloaf, theme: &ChromeTheme, x: f32, cy: f32) {
    let r = Rect::new(x, cy - 8.0, 1.5, 16.0);
    paint_flat(s, &r, theme.accent, D_CTRL + 0.05, 0);
}

fn caret_x(s: &mut Sugarloaf, field: &Rect, prefix: &str) -> f32 {
    field.x + 8.0 + measure_ui_text(s, prefix, font_size::LABEL, UiWeight::Regular)
}

#[allow(clippy::too_many_arguments)]
fn paint_pane(
    s: &mut Sugarloaf,
    theme: &ChromeTheme,
    layout: &FilesLayout,
    state: &SftpPaneState,
    view: &FilesView,
    focus: SftpFocus,
    drop: Option<(SftpFocus, Option<usize>)>,
    now: i64,
    home: Option<&str>,
) {
    let p = layout.pane(focus);
    let side = state.side(focus);
    let drop_here = drop.filter(|(f, _)| *f == focus);
    // Frame; a drop onto the pane itself (not a folder row) tints it.
    let frame_fill = match drop_here {
        Some((_, None)) => blend(theme.frame, theme.accent, 0.08),
        _ => theme.frame,
    };
    rrect(s, &p.frame, frame_fill, geo::PANE_RADIUS, D_FRAME);

    // Header: title, mono path, icon buttons.
    let cy = p.header.y + p.header.height / 2.0;
    let title = pane_title(state, focus);
    let title_max = (p.title_area.width * 0.6).max(60.0);
    let title = fit(title, title_max, false, &mut |t| {
        measure_ui_text(s, t, 14.0, UiWeight::SemiBold)
    });
    let tw = draw_ui_text(
        s,
        p.title_area.x,
        text_top(cy, 14.0),
        &title,
        14.0,
        theme.text,
        UiWeight::SemiBold,
    );
    let path = pane_path(state, focus, home);
    let path_x = p.title_area.x + tw + geo::HEADER_GAP;
    let path_w = (p.title_area.right() - path_x).max(0.0);
    let path = fit(&path, path_w, true, &mut |t| {
        measure_mono_text(s, t, 11.0, UiWeight::Regular)
    });
    draw_mono_text(
        s,
        path_x,
        text_top(cy, 11.0),
        &path,
        11.0,
        theme.text_faint,
        UiWeight::Regular,
    );
    let hov = |h: FilesHit| view.hover == Some(h);
    paint_icon_button(s, theme, &p.up, hov(FilesHit::Up(focus)), true);
    paint_icon_button(
        s,
        theme,
        &p.new_folder,
        hov(FilesHit::NewFolder(focus)),
        false,
    );

    paint_search(s, theme, p, side, frame_fill);

    // Column captions, aligned with the row columns.
    let cap_row = Rect::new(
        p.captions.x + geo::ROW_INSET,
        p.captions.y,
        p.captions.width - 2.0 * geo::ROW_INSET,
        p.captions.height,
    );
    let cl = file_row_layout(cap_row);
    let cap_y = text_top(
        p.captions.y + (p.captions.height - 8.0) / 2.0,
        font_size::CAPTION,
    );
    draw_ui_text(
        s,
        cl.name.x,
        cap_y,
        "Name",
        font_size::CAPTION,
        theme.text_faint,
        UiWeight::Regular,
    );
    for (col, label) in [(&cl.size, "Size"), (&cl.date, "Modified")] {
        let w = measure_ui_text(s, label, font_size::CAPTION, UiWeight::Regular);
        draw_ui_text(
            s,
            col.right() - w,
            cap_y,
            label,
            font_size::CAPTION,
            theme.text_faint,
            UiWeight::Regular,
        );
    }

    // Listing.
    let list = p.list;
    let offset = row_offset(state, focus);
    let mkdir_edit = state
        .name_edit
        .as_ref()
        .filter(|e| e.side == focus && e.kind == SftpNameKind::Mkdir);
    let visible = layout.visible_rows(focus);
    if let Some(edit) = mkdir_edit {
        let r = layout.row_rect(focus, 0, side.scroll, 0);
        if r.y >= list.y && r.bottom() <= list.bottom() + 0.5 {
            paint_name_row(
                s,
                theme,
                &r,
                FileKind::Folder,
                &edit.draft.value,
                edit.draft.caret,
                &edit.draft.prefix(),
                frame_fill,
            );
        }
    }
    let first = (side.scroll / geo::ROW_H).round() as usize;
    for slot in first..first + visible {
        if slot < offset {
            continue;
        }
        let i = slot - offset;
        let Some(row) = side.entries.get(i) else {
            break;
        };
        let r = layout.row_rect(focus, i, side.scroll, offset);
        if r.y < list.y - 0.5 || r.bottom() > list.bottom() + 0.5 {
            continue;
        }
        let renaming = state.name_edit.as_ref().filter(|e| {
            e.side == focus
                && e.kind == SftpNameKind::Rename
                && e.from_path.as_deref() == Some(row.path.as_str())
        });
        let row_state = if renaming.is_some() {
            FileRowState::Renaming
        } else if drop_here.is_some_and(|(_, into)| into == Some(i)) {
            FileRowState::DropTarget
        } else if side.selected == Some(i) {
            FileRowState::Selected
        } else if view.hover == Some(FilesHit::Row(focus, i)) {
            FileRowState::Hover
        } else {
            FileRowState::Default
        };
        let (name, size, date) = (
            row.name.as_str(),
            size_label(row.is_dir, row.size),
            modified_label(row.modified, now),
        );
        if let Some(edit) = renaming {
            paint_name_row(
                s,
                theme,
                &r,
                kind_of(row),
                &edit.draft.value,
                edit.draft.caret,
                &edit.draft.prefix(),
                frame_fill,
            );
            // size/date stay visible beside the field
            let l = file_row_layout(r);
            for (col, text) in [(&l.size, size.as_str()), (&l.date, date.as_str())] {
                let w = measure_ui_text(s, text, font_size::LABEL, UiWeight::Regular);
                draw_ui_text(
                    s,
                    col.right() - w,
                    text_top(r.y + r.height / 2.0, font_size::LABEL),
                    text,
                    font_size::LABEL,
                    theme.text_muted,
                    UiWeight::Regular,
                );
            }
            continue;
        }
        let l = file_row_layout(r);
        let name = fit(name, l.name.width, false, &mut |t| {
            measure_ui_text(s, t, font_size::LABEL, UiWeight::Regular)
        });
        crate::renderer::components::list::paint_file_row(
            s,
            theme,
            r,
            frame_fill,
            row_state,
            kind_of(row),
            &name,
            &size,
            &date,
        );
    }

    // Empty / error / loading states.
    if side.entries.is_empty() && mkdir_edit.is_none() {
        let (msg, color) = if let Some(e) = &side.connect_error {
            (e.clone(), theme.danger_text)
        } else if state.loading {
            ("Loading\u{2026}".to_string(), theme.text_faint)
        } else {
            ("This folder is empty".to_string(), theme.text_faint)
        };
        let msg = fit(&msg, list.width - 32.0, false, &mut |t| {
            measure_ui_text(s, t, font_size::LABEL, UiWeight::Regular)
        });
        draw_ui_text(
            s,
            list.x + geo::HEADER_PAD_L,
            text_top(list.y + 24.0, font_size::LABEL),
            &msg,
            font_size::LABEL,
            color,
            UiWeight::Regular,
        );
    }
}

/// A row whose name is being edited: folder/file icon + field + caret.
#[allow(clippy::too_many_arguments)]
fn paint_name_row(
    s: &mut Sugarloaf,
    theme: &ChromeTheme,
    rect: &Rect,
    kind: FileKind,
    value: &str,
    _caret: usize,
    prefix: &str,
    under: [f32; 4],
) {
    // The List component draws the icon and the focused field; the name text
    // it draws is the draft itself.
    let l = crate::renderer::components::list::paint_file_row(
        s,
        theme,
        *rect,
        under,
        FileRowState::Renaming,
        kind,
        value,
        "",
        "",
    );
    let _ = FILE_RENAME_HEIGHT;
    let cx = caret_x(s, &l.rename_field, prefix);
    paint_caret(s, theme, cx, rect.y + rect.height / 2.0);
}

fn paint_bar(
    s: &mut Sugarloaf,
    theme: &ChromeTheme,
    layout: &FilesLayout,
    state: &SftpPaneState,
    view: &FilesView,
) {
    let (Some(bar), Some(t)) = (layout.bar.as_ref(), state.transfer.as_ref()) else {
        return;
    };
    paint_flat(
        s,
        &Rect::new(bar.bar.x, bar.bar.y, bar.bar.width, 1.0),
        theme.divider,
        D_ROW,
        0,
    );
    let cy = bar.bar.y + bar.bar.height / 2.0;
    let label = fit(&t.label, bar.label.width, false, &mut |x| {
        measure_ui_text(s, x, font_size::LABEL, UiWeight::Regular)
    });
    draw_ui_text(
        s,
        bar.label.x,
        text_top(cy, font_size::LABEL),
        &label,
        font_size::LABEL,
        theme.text,
        UiWeight::Regular,
    );
    // 4px track + fill.
    rrect(s, &bar.track, theme.raised, bar.track.height / 2.0, D_ROW);
    let frac = if t.total == 0 { 0.0 } else { t.fraction() };
    let fill = progress_fill_rect(&bar.track, frac);
    if fill.width > 0.5 {
        rrect(
            s,
            &fill,
            progress_fill_color(ProgressKind::Transfer, theme),
            fill.height / 2.0,
            D_ROW + 0.05,
        );
    }
    let cap = t.caption();
    if !cap.is_empty() {
        let w = measure_ui_text(s, &cap, font_size::LABEL, UiWeight::Regular);
        draw_ui_text(
            s,
            bar.caption.right() - w,
            text_top(cy, font_size::LABEL),
            &cap,
            font_size::LABEL,
            theme.text_muted,
            UiWeight::Regular,
        );
    }
    // Cancel (Secondary, Small), centred in the hit rect.
    let spec = label_spec(
        s,
        (0.0, 0.0),
        ButtonKind::Secondary,
        ButtonSize::Small,
        "Cancel",
        false,
    );
    let mut spec = spec;
    spec.origin = (
        bar.cancel.x + (bar.cancel.width - spec.width()) / 2.0,
        bar.cancel.y + (bar.cancel.height - spec.rect().height) / 2.0,
    );
    let st = if view.hover == Some(FilesHit::CancelTransfer) {
        ButtonState::Hover
    } else {
        ButtonState::Default
    };
    paint_button(s, theme, &spec, st, "Cancel", None);
}

fn paint_ghost(s: &mut Sugarloaf, theme: &ChromeTheme, state: &SftpPaneState) {
    let Some(d) = state.drag.as_ref() else { return };
    let w = measure_ui_text(s, &d.name, font_size::LABEL, UiWeight::Medium)
        + 2.0 * 12.0
        + 15.0
        + 8.0;
    let r = Rect::new(d.pointer_x + 14.0, d.pointer_y + 10.0, w, 32.0);
    s.rounded_rect(
        None,
        r.x,
        r.y,
        r.width,
        r.height,
        theme.raised,
        D_GHOST,
        8.0,
        0,
    );
    let scale = s.scale_factor();
    let icon = if d.is_dir { Icon::Folder } else { Icon::File };
    let color = if d.is_dir {
        theme.info
    } else {
        unit_color(theme.text_faint)
    };
    draw_icon(
        s,
        icon,
        IconPlacement::new(r.x + 12.0, r.y + 8.5, 15.0),
        color,
        scale,
    );
    draw_ui_text(
        s,
        r.x + 35.0,
        text_top(r.y + 16.0, font_size::LABEL),
        &d.name,
        font_size::LABEL,
        theme.text,
        UiWeight::Medium,
    );
}

// --------------------------------------------------------- conflict dialog

/// Spec for the conflict prompt (copy per the mock).
fn conflict_spec(state: &SftpPaneState) -> Option<(DialogSpec<'static>, String, String)> {
    let c = state.conflict.as_ref()?;
    let name = c
        .relative_path
        .rsplit('/')
        .next()
        .unwrap_or(&c.relative_path);
    let what = match c.kind {
        SftpConflictKind::File => "file",
        SftpConflictKind::Directory => "folder",
    };
    let dest = state.side(state.other_focus());
    let host = if dest.is_local() {
        "this computer"
    } else {
        dest.title()
    };
    let title = format!("{name} already exists");
    let body = format!(
        "There is already a {what} with this name in {} on {host}.",
        if dest.cwd.is_empty() {
            "/"
        } else {
            dest.cwd.as_str()
        }
    );
    Some((
        DialogSpec {
            kind: DialogKind::WithOption,
            title: "",
            body: "",
            confirm: "Replace",
            cancel: "Keep existing",
            option: Some("Do this for every conflict"),
            option_checked: c.apply_to_all,
        },
        title,
        body,
    ))
}

fn conflict_layout(
    s: &mut Sugarloaf,
    spec: &DialogSpec,
    content: Rect,
) -> (DialogLayout, Vec<String>) {
    // Measure at the origin, then centre in `content`.
    let (probe, lines) =
        dialog_layout_for(s, spec, (0.0, 0.0), (content.width, content.height));
    let x = (content.x + (content.width - probe.dialog.width) / 2.0).round();
    let y = (content.y + (content.height - probe.dialog.height) / 2.0).round();
    let (mut layout, lines2) =
        dialog_layout_for(s, spec, (x, y), (content.width, content.height));
    layout.scrim = content;
    let _ = lines;
    (layout, lines2)
}

fn paint_conflict(
    s: &mut Sugarloaf,
    theme: &ChromeTheme,
    content: Rect,
    state: &SftpPaneState,
    view: &FilesView,
) {
    let Some((mut spec, title, body)) = conflict_spec(state) else {
        return;
    };
    spec.title = &title;
    spec.body = &body;
    let (layout, lines) = conflict_layout(s, &spec, content);
    paint_flat(s, &layout.scrim, ov::SCRIM, 0.3, SCRIM_ORDER);
    paint_dialog(
        s,
        theme,
        &spec,
        &layout,
        &lines,
        Some(view.dialog_focus.unwrap_or(DialogFocus::Confirm)),
    );
}

// ------------------------------------------------------------------ entry

/// Paint the Files view into `content`.
pub fn paint(
    s: &mut Sugarloaf,
    theme: &ChromeTheme,
    content: Rect,
    state: &SftpPaneState,
    view: &FilesView,
) {
    paint_at(
        s,
        theme,
        content,
        state,
        view,
        now_unix(),
        home_dir().as_deref(),
    );
}

/// [`paint`] with an explicit clock and home directory (previews, tests).
pub fn paint_at(
    s: &mut Sugarloaf,
    theme: &ChromeTheme,
    content: Rect,
    state: &SftpPaneState,
    view: &FilesView,
    now: i64,
    home: Option<&str>,
) {
    let layout = FilesLayout::for_state(content, state);
    let drop = state
        .drag
        .as_ref()
        .and_then(|d| drop_target(&layout, state, d.pointer_x, d.pointer_y));
    for focus in [SftpFocus::Left, SftpFocus::Right] {
        paint_pane(s, theme, &layout, state, view, focus, drop, now, home);
    }
    paint_bar(s, theme, &layout, state, view);
    paint_ghost(s, theme, state);
    paint_conflict(s, theme, content, state, view);
}

/// What `(x, y)` is over (for the cursor and hover).
pub fn hit_at(content: Rect, state: &SftpPaneState, x: f32, y: f32) -> FilesHit {
    FilesLayout::for_state(content, state).hit_test(state, x, y)
}

/// Pointer press. `double` marks a platform-detected double click (a second
/// click on the same row within 400 ms also counts).
pub fn pointer_press(
    s: &mut Sugarloaf,
    sftp: &mut ActiveSftp,
    view: &mut FilesView,
    content: Rect,
    x: f32,
    y: f32,
    double: bool,
) -> FilesAction {
    if sftp.state.conflict.is_some() {
        let Some((mut spec, title, body)) = conflict_spec(&sftp.state) else {
            return FilesAction::None;
        };
        spec.title = &title;
        spec.body = &body;
        let (layout, _) = conflict_layout(s, &spec, content);
        match layout.hit_test(x, y) {
            DialogHit::Confirm => sftp.answer_conflict(true),
            DialogHit::Cancel => sftp.answer_conflict(false),
            DialogHit::Option => {
                sftp.state.toggle_conflict_apply_all();
            }
            DialogHit::Inside | DialogHit::Scrim => return FilesAction::None,
        }
        view.dialog_focus = None;
        return FilesAction::Redraw;
    }
    let layout = FilesLayout::for_state(content, &sftp.state);
    let hit = layout.hit_test(&sftp.state, x, y);
    if !matches!(hit, FilesHit::Search(_)) {
        sftp.state.blur_filters();
    }
    match hit {
        FilesHit::Miss | FilesHit::Bar => return FilesAction::None,
        FilesHit::Search(f) => sftp.state.focus_filter(f),
        FilesHit::CancelTransfer => sftp.cancel_transfer(),
        FilesHit::NewFolder(f) => {
            sftp.state.focus = f;
            sftp.begin_mkdir_focused();
        }
        FilesHit::Header(f) | FilesHit::Pane(f) => sftp.state.focus = f,
        FilesHit::Row(..) | FilesHit::Up(_) => {
            let h = hit.to_sftp_hit();
            sftp.handle_hit(h.clone(), double);
            sftp.drag_press(h, x, y);
        }
    }
    FilesAction::Redraw
}

/// Pointer move; `pressed` is whether the primary button is down.
pub fn pointer_move(
    sftp: &mut ActiveSftp,
    view: &mut FilesView,
    content: Rect,
    x: f32,
    y: f32,
    pressed: bool,
) -> FilesAction {
    let mut changed = false;
    if pressed && sftp.drag_move(x, y) {
        changed = true;
    }
    let hit = (sftp.state.conflict.is_none()).then(|| hit_at(content, &sftp.state, x, y));
    let hit = hit.filter(|h| !matches!(h, FilesHit::Miss));
    if view.hover != hit {
        view.hover = hit;
        changed = true;
    }
    if changed {
        FilesAction::Redraw
    } else {
        FilesAction::None
    }
}

/// Pointer release: finishes a drag (upload / download / copy between panes).
pub fn pointer_release(
    sftp: &mut ActiveSftp,
    content: Rect,
    x: f32,
    y: f32,
) -> FilesAction {
    if sftp.state.drag.is_none() {
        sftp.cancel_drag();
        return FilesAction::None;
    }
    let layout = FilesLayout::for_state(content, &sftp.state);
    let target = drop_target(&layout, &sftp.state, x, y);
    sftp.drag_release_to(target.map(|(f, _)| f), target.and_then(|(_, i)| i));
    FilesAction::Redraw
}

/// Wheel over the view; `lines` > 0 scrolls the listing down.
pub fn wheel(
    sftp: &mut ActiveSftp,
    content: Rect,
    x: f32,
    y: f32,
    lines: f32,
) -> FilesAction {
    let layout = FilesLayout::for_state(content, &sftp.state);
    let Some(pane) = layout.focus_at(x, y) else {
        return FilesAction::None;
    };
    let rows = sftp.state.side(pane).entries.len() + row_offset(&sftp.state, pane);
    let max = layout.max_scroll(pane, rows);
    // Quantized to whole rows; positive `lines` reveals later entries.
    let n = lines.abs().round().max(1.0) * lines.signum();
    if sftp.scroll_pane(pane, n * geo::SCROLL_STEP, max) {
        FilesAction::Redraw
    } else {
        FilesAction::None
    }
}

/// Right-click: a context menu for the row / empty space under the pointer.
pub fn context_menu(sftp: &mut ActiveSftp, content: Rect, x: f32, y: f32) -> FilesAction {
    let layout = FilesLayout::for_state(content, &sftp.state);
    let hit = layout.hit_test(&sftp.state, x, y);
    let blank = hit.pane();
    match sftp.context_menu_for(hit.to_sftp_hit(), blank, x, y) {
        Some(menu) => FilesAction::ContextMenu(menu),
        None => FilesAction::None,
    }
}

/// Key while the conflict dialog is open (Esc keeps the existing file, Enter
/// activates the focused button, Tab swaps). Returns `None` when no conflict
/// is pending, so the caller routes the key to `ActiveSftp::handle_key`.
pub fn conflict_key(
    sftp: &mut ActiveSftp,
    view: &mut FilesView,
    key: DialogKey,
) -> Option<FilesAction> {
    sftp.state.conflict.as_ref()?;
    let focus = view.dialog_focus.unwrap_or(DialogFocus::Confirm);
    match (
        key,
        terminus_ui::components::overlay::dialog_key(key, focus),
    ) {
        // Esc keeps the existing file (the dialogs' semantics), like
        // "Keep existing"; the transfer goes on with the next item.
        (DialogKey::Escape, _) => sftp.answer_conflict(false),
        (_, DialogOutcome::Confirm) => sftp.answer_conflict(true),
        (_, DialogOutcome::Cancel) => sftp.answer_conflict(false),
        (_, DialogOutcome::Focus(f)) => {
            view.dialog_focus = Some(f);
            return Some(FilesAction::Redraw);
        }
    }
    view.dialog_focus = None;
    Some(FilesAction::Redraw)
}

// ---------------------------------------------------------------- preview

/// `TERMINUS_VIEW_PREVIEW=files` paints this view full-window.
pub fn preview_selected() -> bool {
    std::env::var("TERMINUS_VIEW_PREVIEW")
        .map(|v| v.split(',').any(|p| p.trim() == "files"))
        .unwrap_or(false)
}

fn seed_row(
    dir: &str,
    name: &str,
    is_dir: bool,
    size: u64,
    age_days: i64,
    now: i64,
) -> SftpRow {
    SftpRow {
        name: name.into(),
        path: format!("{dir}/{name}"),
        is_dir,
        size,
        modified: Some(now - age_days * 86_400 - 3_600),
    }
}

/// Mock data of the design (This computer | jerem prod, 62 % upload). Variants
/// via `TERMINUS_VIEW_PREVIEW_STATE`: `rename`, `mkdir`, `drag`, `conflict`,
/// `hover`, `idle` (no transfer), `empty`.
pub fn preview_state(now: i64) -> (SftpPaneState, FilesView) {
    let mut st =
        SftpPaneState::new_local_remote("/home/jeremy/projects", "h1", "jerem prod");
    let l = "/home/jeremy/projects";
    let r = "/home/ubuntu/app";
    st.set_listed(
        SftpFocus::Left,
        l.into(),
        vec![
            seed_row(l, "terminus", true, 0, 0, now),
            seed_row(l, "notes", true, 0, 1, now),
            seed_row(l, "build.tar.gz", false, 18 * 1024 * 1024, 0, now),
            seed_row(l, "notes.md", false, 4096, 3, now),
        ],
    );
    st.set_listed(
        SftpFocus::Right,
        r.into(),
        vec![
            seed_row(r, "releases", true, 0, 0, now),
            seed_row(r, "logs", true, 0, 0, now),
            seed_row(r, "docker-compose.yml", false, 2048, 3, now),
            seed_row(r, ".env", false, 1024, 3, now),
        ],
    );
    st.left.selected = Some(2);
    st.status = "Ready".into();
    st.set_transfer(
        "Uploading build.tar.gz",
        11 * 1024 * 1024,
        18 * 1024 * 1024 + 300_000,
    );
    let mut view = FilesView::default();
    match std::env::var("TERMINUS_VIEW_PREVIEW_STATE")
        .unwrap_or_default()
        .as_str()
    {
        "rename" => {
            st.begin_rename();
        }
        "mkdir" => {
            st.focus = SftpFocus::Right;
            st.begin_mkdir();
        }
        "drag" => {
            st.drag = Some(terminus_ui::sftp_pane::SftpDrag {
                from: SftpFocus::Left,
                row_index: 2,
                name: "build.tar.gz".into(),
                path: format!("{l}/build.tar.gz"),
                is_dir: false,
                pointer_x: 1000.0,
                pointer_y: 275.0,
            });
        }
        "conflict" => st.begin_conflict(1, SftpConflictKind::File, "build.tar.gz"),
        "hover" => view.hover = Some(FilesHit::Row(SftpFocus::Right, 1)),
        "idle" => st.clear_transfer(),
        "empty" => {
            st.clear_transfer();
            st.right.entries.clear();
        }
        _ => {}
    }
    (st, view)
}

/// Full-window preview: content = window minus a 260 px strip on the left and
/// a 96 px strip on top, over the canvas.
pub fn paint_preview(s: &mut Sugarloaf, theme: &ChromeTheme) {
    let scale = s.scale_factor();
    let size = s.window_size();
    let (w, h) = (size.width / scale, size.height / scale);
    crate::renderer::ui_text::sync_ui_fonts(s);
    paint_flat(s, &Rect::new(0.0, 0.0, w, h), theme.canvas, 0.0, 0);
    let content = Rect::new(260.0, 96.0, (w - 260.0).max(0.0), (h - 96.0).max(0.0));
    let now = now_unix();
    let (state, view) = preview_state(now);
    paint_at(s, theme, content, &state, &view, now, Some("/home/jeremy"));
}
