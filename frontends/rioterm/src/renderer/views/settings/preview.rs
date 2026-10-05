//! `TERMINUS_VIEW_PREVIEW=settings-keys|settings-sync|settings-appearance|
//! settings-updates`: paint one Settings tab full-window, with seeded data,
//! into a content rect = window minus a 260 px left strip and a 96 px top
//! strip. `TERMINUS_VIEW_PREVIEW_STATE` picks a variant: `draft`, `import`,
//! `confirm` (keys), `menu` (appearance), `error` (sync), `available`
//! (updates).

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::geom::Rect;
use terminus_ui::settings::SshKeyItem;
use terminus_ui::theme::ChromeTheme;
use terminus_ui::tokens::font_size;
use terminus_ui::views::settings::keys::DraftMode;
use terminus_ui::views::settings::sync::SyncStatus;
use terminus_ui::views::settings::{CursorStyle, Key, Page, SettingsView, UpdateStatus};

use super::{paint, with_measure};
use crate::renderer::chrome::paint_flat;
use crate::renderer::ui_text::{draw_ui_text, UiWeight};

const LEFT: f32 = 260.0;
const TOP: f32 = 96.0;

/// Page selected by `TERMINUS_VIEW_PREVIEW`, read once.
pub fn selector() -> Option<Page> {
    static PAGE: std::sync::OnceLock<Option<Page>> = std::sync::OnceLock::new();
    *PAGE.get_or_init(|| {
        let v = std::env::var("TERMINUS_VIEW_PREVIEW").ok()?;
        Page::from_preview(v.trim())
    })
}

fn variant() -> String {
    std::env::var("TERMINUS_VIEW_PREVIEW_STATE").unwrap_or_default()
}

fn key(id: &str, name: &str, created: &str) -> SshKeyItem {
    SshKeyItem {
        id: id.into(),
        name: name.into(),
        fingerprint: format!("SHA256:{}", "x7Qm2Ld9aVt0pZc4HsKq8uWb1NyEjR3fGiT5oDe6vXA"),
        created: created.into(),
        public_key: "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIExample".into(),
    }
}

/// Seeded view for a page and variant.
pub fn seeded(page: Page, variant: &str) -> SettingsView {
    let mut v = SettingsView::new(crate::updater::CURRENT_VERSION);
    v.page = page;
    v.keys.set_keys(vec![
        key("k1", "id_ed25519", "Mar 4, 2026"),
        key("k2", "work-laptop", "Jun 18, 2026"),
    ]);
    v.sync.apply_snapshot(
        "sqlite:/mnt/shared/terminus.db",
        SyncStatus {
            connected: true,
            vault_unlocked: true,
            line: String::new(),
            is_error: false,
        },
    );
    v.appearance.font = "JetBrains Mono".into();
    v.appearance.size = 14.0;
    v.appearance.set_fonts(
        [
            "Cascadia Code",
            "Courier New",
            "DejaVu Sans Mono",
            "Fira Code",
            "Hack",
            "Iosevka",
            "JetBrains Mono",
            "Liberation Mono",
            "Martian Mono",
            "Noto Sans Mono",
            "Source Code Pro",
            "Ubuntu Mono",
        ]
        .map(String::from)
        .to_vec(),
    );
    v.updates.status = UpdateStatus::UpToDate;
    match (page, variant) {
        (Page::Keys, "draft") => {
            v.keys.open_draft(DraftMode::Generate);
            v.keys.insert_text("deploy-key");
        }
        (Page::Keys, "import") => {
            v.keys.open_draft(DraftMode::Import);
            v.keys.insert_text("old-laptop");
            v.keys.key(Key::Tab);
            v.keys.insert_text(
                "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAA",
            );
            v.keys
                .set_draft_error("That key is encrypted: enter its passphrase");
        }
        (Page::Keys, "confirm") => {
            let content = Rect::new(LEFT, TOP, 1180.0, 804.0);
            let l = v
                .keys
                .layout(content, &mut |t, s, _| t.chars().count() as f32 * s * 0.55);
            let r = l.rows[0].card.actions[1].unwrap();
            v.keys.press(
                content,
                &mut |t, s, _| t.chars().count() as f32 * s * 0.55,
                r.x + 4.0,
                r.y + 4.0,
            );
        }
        (Page::Sync, "error") => {
            v.sync.status = SyncStatus {
                connected: false,
                vault_unlocked: true,
                line: "Could not reach the database".into(),
                is_error: true,
            };
        }
        (Page::Appearance, "menu") => {
            v.appearance.font_menu_open = true;
            v.appearance.cursor = CursorStyle::Beam;
        }
        (Page::Updates, "available") => {
            v.updates.status = UpdateStatus::Available {
                version: "0.7.0".into(),
                can_install: true,
            };
        }
        _ => {}
    }
    v
}

/// Paint the preview frame; returns false when no preview is selected.
pub fn paint_frame(sugarloaf: &mut Sugarloaf, theme: &ChromeTheme) -> bool {
    let Some(page) = selector() else {
        return false;
    };
    let scale = sugarloaf.scale_factor();
    let size = sugarloaf.window_size();
    let (w, h) = (size.width / scale, size.height / scale);
    paint_flat(sugarloaf, &Rect::new(0.0, 0.0, w, h), theme.canvas, 0.0, 0);
    paint_flat(
        sugarloaf,
        &Rect::new(0.0, 0.0, LEFT, h),
        theme.frame,
        0.01,
        0,
    );

    // Stand-in for the shell header: title and the four tabs.
    draw_ui_text(
        sugarloaf,
        LEFT + 28.0,
        26.0,
        "Settings",
        font_size::DISPLAY,
        theme.text,
        UiWeight::SemiBold,
    );
    let mut x = LEFT + 28.0;
    for p in Page::ALL {
        let on = p == page;
        let label_w = with_measure(sugarloaf, |m| m(p.label(), 14.0, false));
        draw_ui_text(
            sugarloaf,
            x,
            70.0,
            p.label(),
            14.0,
            if on { theme.text } else { theme.text_muted },
            UiWeight::Medium,
        );
        if on {
            paint_flat(
                sugarloaf,
                &Rect::new(x, 92.0, label_w, 2.0),
                theme.accent,
                0.02,
                0,
            );
        }
        x += label_w + 28.0;
    }

    let content = Rect::new(LEFT, TOP, (w - LEFT).max(0.0), (h - TOP).max(0.0));
    let view = seeded(page, &variant());
    paint(sugarloaf, theme, content, &view);
    true
}
