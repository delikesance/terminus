//! Settings page painter: `paint` draws the active tab into the content
//! rect; `press` / `hover` / `wheel` take the same rect and return actions.
//!
//! Geometry and state live in `terminus_ui::views::settings`; this module
//! only measures text for it and turns its rects into sugarloaf primitives.
//! The header with the four tabs belongs to the shell.

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::geom::Rect;
use terminus_ui::theme::ChromeTheme;
use terminus_ui::views::settings::{
    config_edit, CursorStyle, Page, SettingsAction, SettingsView, ThemeChoice,
    UpdateStatus,
};

use crate::renderer::ui_text::{measure_ui_text, UiWeight};

pub mod appearance;
pub mod keys;
pub mod preview;
pub mod sync;
pub mod updates;

/// Run `f` with a text measure backed by the UI fonts.
pub fn with_measure<R>(
    sugarloaf: &mut Sugarloaf,
    f: impl FnOnce(&mut dyn FnMut(&str, f32, bool) -> f32) -> R,
) -> R {
    let mut m = |text: &str, size: f32, semibold: bool| {
        let weight = if semibold {
            UiWeight::SemiBold
        } else {
            UiWeight::Medium
        };
        measure_ui_text(sugarloaf, text, size, weight)
    };
    f(&mut m)
}

/// Paint the active tab of `view` into `content` (the shell paints the
/// background and the header).
pub fn paint(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    content: Rect,
    view: &SettingsView,
) {
    match view.page {
        Page::Keys => keys::paint(sugarloaf, theme, content, &view.keys),
        Page::Sync => sync::paint(sugarloaf, theme, content, &view.sync),
        Page::Appearance => {
            appearance::paint(sugarloaf, theme, content, &view.appearance)
        }
        Page::Updates => updates::paint(sugarloaf, theme, content, &view.updates),
    }
}

/// Pointer press inside the Settings content.
pub fn press(
    sugarloaf: &mut Sugarloaf,
    content: Rect,
    view: &mut SettingsView,
    x: f32,
    y: f32,
) -> Option<SettingsAction> {
    with_measure(sugarloaf, |m| view.press(content, m, x, y))
}

/// Pointer shape over the Settings content (hand on controls, I-beam on
/// fields).
pub fn cursor_at(
    sugarloaf: &mut Sugarloaf,
    content: Rect,
    view: &SettingsView,
    x: f32,
    y: f32,
) -> terminus_ui::ChromeCursor {
    with_measure(sugarloaf, |m| view.cursor_at(content, m, x, y))
}

/// Pointer move; true when a repaint is needed.
pub fn hover(
    sugarloaf: &mut Sugarloaf,
    content: Rect,
    view: &mut SettingsView,
    x: f32,
    y: f32,
) -> bool {
    with_measure(sugarloaf, |m| view.hover(content, m, x, y))
}

/// Wheel (`dy` > 0 scrolls down); true when a repaint is needed.
pub fn wheel(
    sugarloaf: &mut Sugarloaf,
    content: Rect,
    view: &mut SettingsView,
    x: f32,
    y: f32,
    dy: f32,
) -> bool {
    with_measure(sugarloaf, |m| view.wheel(content, m, x, y, dy))
}

/// Write the config line a settings action changes (`fonts`, `cursor`,
/// `updates`) into the file the app hot-reloads. No-op for other actions.
pub fn persist(action: &SettingsAction) -> std::io::Result<()> {
    persist_at(&rio_backend::config::config_file_path(), action)
}

pub fn persist_at(
    path: &std::path::Path,
    action: &SettingsAction,
) -> std::io::Result<()> {
    let Some((section, key, value)) = action.config_edit() else {
        return Ok(());
    };
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    let out = config_edit::set_value(&src, section, key, &value);
    if out != src {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, out)?;
    }
    Ok(())
}

/// Seed the appearance and updates controls from the loaded config.
pub fn load_config(view: &mut SettingsView, config: &rio_backend::config::Config) {
    view.appearance.font = config.fonts.family.clone().unwrap_or_default();
    view.appearance.size = config.fonts.size.clamp(
        terminus_ui::views::settings::appearance::MIN_SIZE,
        terminus_ui::views::settings::appearance::MAX_SIZE,
    );
    view.appearance.cursor =
        CursorStyle::from_config(&format!("{:?}", config.cursor.shape));
    view.appearance.theme =
        ThemeChoice::from_config(config.theme_preference().config_value());
    view.updates.check = config.updates.check;
    view.updates.auto_install = config.updates.auto_install;
}

/// Map the updater state to the words the Updates tab shows.
pub fn update_status(state: &crate::updater::UpdateState) -> UpdateStatus {
    use crate::updater::UpdateState as S;
    match state {
        S::Idle => UpdateStatus::Idle,
        S::Checking => UpdateStatus::Checking,
        S::UpToDate => UpdateStatus::UpToDate,
        S::Available {
            version,
            can_install,
            ..
        } => UpdateStatus::Available {
            version: version.clone(),
            can_install: *can_install,
        },
        S::Downloading { version } => UpdateStatus::Downloading {
            version: version.clone(),
        },
        S::ReadyToRestart { version } | S::InstallerReady { version, .. } => {
            UpdateStatus::ReadyToRestart {
                version: version.clone(),
            }
        }
        S::PackageReady { version, .. } => UpdateStatus::Available {
            version: version.clone(),
            can_install: false,
        },
        S::Failed(e) => UpdateStatus::Failed(e.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("terminus-settings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("config.toml")
    }

    #[test]
    fn persist_creates_the_file_and_keeps_other_lines() {
        let path = tmp("persist");
        persist_at(&path, &SettingsAction::SetCursor(CursorStyle::Beam)).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "[cursor]\nshape = \"beam\"\n"
        );
        std::fs::write(
            &path,
            "# mine\n[fonts]\nsize = 16\n[cursor]\nshape = \"block\"\n",
        )
        .unwrap();
        persist_at(&path, &SettingsAction::SetFontSize(18.0)).unwrap();
        persist_at(&path, &SettingsAction::SetCursor(CursorStyle::Underline)).unwrap();
        persist_at(&path, &SettingsAction::SetAutoInstall(false)).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("# mine\n[fonts]\nsize = 18\n"));
        assert!(text.contains("shape = \"underline\""));
        assert!(text.ends_with("[updates]\nauto-install = false\n"));
        // Non-persistent actions leave the file alone.
        persist_at(&path, &SettingsAction::CheckUpdates).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn persisted_values_load_back_through_the_real_config_parser() {
        let path = tmp("roundtrip");
        persist_at(&path, &SettingsAction::SetFont("Fira Code".into())).unwrap();
        persist_at(&path, &SettingsAction::SetFontSize(13.0)).unwrap();
        persist_at(&path, &SettingsAction::SetCursor(CursorStyle::Beam)).unwrap();
        persist_at(&path, &SettingsAction::SetCheckUpdates(false)).unwrap();
        persist_at(&path, &SettingsAction::SetAutoInstall(false)).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let config: rio_backend::config::Config = toml::from_str(&text).unwrap();
        let mut view = SettingsView::new("0.0.0");
        load_config(&mut view, &config);
        assert_eq!(view.appearance.font, "Fira Code");
        assert_eq!(view.appearance.size, 13.0);
        assert_eq!(view.appearance.cursor, CursorStyle::Beam);
        assert!(!view.updates.check && !view.updates.auto_install);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn every_theme_choice_loads_back_as_itself() {
        let path = tmp("theme");
        for choice in ThemeChoice::ALL {
            persist_at(&path, &SettingsAction::SetTheme(choice)).unwrap();
            let text = std::fs::read_to_string(&path).unwrap();
            let mut config: rio_backend::config::Config = toml::from_str(&text).unwrap();
            config.resolve_appearance();
            let mut view = SettingsView::new("0.0.0");
            load_config(&mut view, &config);
            assert_eq!(view.appearance.theme, choice, "{text}");
        }
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
