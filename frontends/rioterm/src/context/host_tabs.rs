//! Tab navigation scoped to one host: the strip of pills only shows the
//! current host's tabs, so cycling and `SelectTab(n)` stay inside them.
//! A tab without a host id belongs to the local machine.

use super::ContextManager;
use rio_backend::event::EventListener;

impl<T: EventListener + Clone + std::marker::Send + 'static> ContextManager<T> {
    fn host_of(&self, index: usize) -> &str {
        self.contexts[index]
            .current()
            .host_id
            .as_deref()
            .unwrap_or(crate::hosts::LOCAL_ID)
    }

    /// Indices of the current tab's host's tabs, in strip order.
    fn current_host_tabs(&self) -> Vec<usize> {
        let host = self.host_of(self.current_index);
        (0..self.contexts.len())
            .filter(|&i| self.host_of(i) == host)
            .collect()
    }

    /// The tab after (or before) the current one among its host's tabs,
    /// wrapping; the current tab itself when the host has only one.
    pub(super) fn host_neighbour(&self, forward: bool) -> usize {
        let tabs = self.current_host_tabs();
        let at = tabs
            .iter()
            .position(|&i| i == self.current_index)
            .unwrap_or(0);
        let next = if forward {
            (at + 1) % tabs.len()
        } else {
            (at + tabs.len() - 1) % tabs.len()
        };
        tabs[next]
    }

    /// The `n`th tab (0-based) of the current host, as the pills number them.
    pub(super) fn host_tab_at(&self, n: usize) -> Option<usize> {
        self.current_host_tabs().get(n).copied()
    }

    /// The last tab of the current host.
    pub(super) fn host_last_tab(&self) -> Option<usize> {
        self.current_host_tabs().last().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::VoidListener;
    use rio_backend::event::WindowId;

    /// Tabs 0..hosts.len(); `""` stands for a tab without host id (local).
    fn manager(hosts: &[&str]) -> ContextManager<VoidListener> {
        let mut cm = ContextManager::start_with_capacity(
            hosts.len() + 1,
            VoidListener {},
            WindowId::from(0),
        )
        .unwrap();
        for _ in 1..hosts.len() {
            cm.add_context(false, 0);
        }
        for (i, host) in hosts.iter().enumerate() {
            cm.contexts[i].current_mut().host_id =
                (!host.is_empty()).then(|| host.to_string());
        }
        cm.set_current(0);
        cm
    }

    #[test]
    fn next_never_lands_on_another_host() {
        let mut cm = manager(&["a", "b", "a"]);
        cm.switch_to_next();
        assert_eq!(cm.current_index(), 2);
        cm.switch_to_next();
        assert_eq!(cm.current_index(), 0, "wraps inside host a");
        cm.switch_to_prev();
        assert_eq!(cm.current_index(), 2);
    }

    #[test]
    fn a_host_with_one_tab_does_not_move() {
        let mut cm = manager(&["a", "b", "a"]);
        cm.set_current(1);
        cm.switch_to_next();
        assert_eq!(cm.current_index(), 1);
        cm.switch_to_prev();
        assert_eq!(cm.current_index(), 1);
    }

    #[test]
    fn a_tab_without_host_id_is_the_local_machine() {
        let mut cm = manager(&["", "a", crate::hosts::LOCAL_ID, "a"]);
        cm.switch_to_next();
        assert_eq!(cm.current_index(), 2);
        cm.switch_to_next();
        assert_eq!(cm.current_index(), 0);
    }

    #[test]
    fn select_tab_indexes_the_current_hosts_tabs() {
        let mut cm = manager(&["a", "b", "a", "b"]);
        cm.set_current(1);
        cm.select_tab(1);
        assert_eq!(cm.current_index(), 3, "second tab of host b");
        cm.select_tab(5);
        assert_eq!(cm.current_index(), 3, "past the end: no-op");
        cm.select_tab(0);
        assert_eq!(cm.current_index(), 1);
    }

    #[test]
    fn select_last_tab_picks_the_hosts_last_tab() {
        let mut cm = manager(&["a", "b", "a", "b"]);
        cm.select_last_tab();
        assert_eq!(cm.current_index(), 2, "last tab of host a");
    }
}
