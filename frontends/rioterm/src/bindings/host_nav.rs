//! Default shortcuts for host and view navigation (R1b), kept out of
//! `mod.rs`: `Ctrl+Shift+Up/Down` cycle hosts, `Ctrl+Alt+1..5` show a view.
//! Alt+digit is readline/tmux and Ctrl+Alt+arrows are GNOME workspaces,
//! hence these combinations.

use super::*;
use terminus_ui::shell::WorkspaceView as V;

pub(super) fn default_bindings() -> Vec<KeyBinding> {
    let mods = ModifiersState::CONTROL | ModifiersState::SHIFT;
    let views = ModifiersState::CONTROL | ModifiersState::ALT;
    bindings!(
        KeyBinding;
        Key::Named(ArrowUp), mods, ~BindingMode::SEARCH, ~BindingMode::VI; Action::SelectPrevHost;
        Key::Named(ArrowDown), mods, ~BindingMode::SEARCH, ~BindingMode::VI; Action::SelectNextHost;
        "1", views; Action::ShowView(V::Terminal);
        "2", views; Action::ShowView(V::Files);
        "3", views; Action::ShowView(V::Tunnels);
        "4", views; Action::ShowView(V::Snippets);
        "5", views; Action::ShowView(V::History);
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> Vec<KeyBinding> {
        default_key_bindings(&rio_backend::config::Config::default())
    }

    fn bound(
        bindings: &[KeyBinding],
        key: Key,
        mods: ModifiersState,
        action: Action,
    ) -> bool {
        bindings.iter().any(|b| {
            b.action == action
                && b.mods == mods
                && matches!(&b.trigger, BindingKey::Keycode { key: k, .. } if *k == key)
        })
    }

    #[test]
    fn host_cycling_and_view_shortcuts_are_bound_by_default() {
        let b = defaults();
        let cs = ModifiersState::CONTROL | ModifiersState::SHIFT;
        assert!(bound(&b, Key::Named(ArrowUp), cs, Action::SelectPrevHost));
        assert!(bound(&b, Key::Named(ArrowDown), cs, Action::SelectNextHost));
        let ca = ModifiersState::CONTROL | ModifiersState::ALT;
        for (digit, view) in [
            ("1", V::Terminal),
            ("2", V::Files),
            ("3", V::Tunnels),
            ("4", V::Snippets),
            ("5", V::History),
        ] {
            assert!(
                bound(&b, Key::Character(digit.into()), ca, Action::ShowView(view)),
                "Ctrl+Alt+{digit}"
            );
        }
    }

    #[test]
    fn shift_tab_still_stays_in_the_host() {
        // Shift+Tab chords keep cycling tabs, not hosts.
        let b = defaults();
        let cs = ModifiersState::CONTROL | ModifiersState::SHIFT;
        assert!(bound(&b, Key::Named(Tab), cs, Action::SelectPrevTab));
    }
}
