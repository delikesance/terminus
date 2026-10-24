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

    /// The tab to bring forward to reach the host after (or before) the
    /// current one, wrapping. `order` is the sidebar's host order; hosts with
    /// tabs but absent from it (collapsed group) follow, in strip order.
    /// Hosts without a tab are skipped; `None` when fewer than two hosts
    /// have tabs. The host's last focused tab wins.
    pub fn neighbour_host_tab(&self, order: &[&str], forward: bool) -> Option<usize> {
        let current = self.host_of(self.current_index);
        let mut hosts: Vec<&str> = Vec::new();
        for id in order
            .iter()
            .copied()
            .chain((0..self.contexts.len()).map(|i| self.host_of(i)))
        {
            let has_tab = (0..self.contexts.len()).any(|i| self.host_of(i) == id);
            if has_tab && !hosts.contains(&id) {
                hosts.push(id);
            }
        }
        if hosts.len() < 2 {
            return None;
        }
        let at = hosts.iter().position(|&h| h == current)?;
        let next = if forward {
            (at + 1) % hosts.len()
        } else {
            (at + hosts.len() - 1) % hosts.len()
        };
        self.host_tab_to_restore(hosts[next])
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

    #[test]
    fn next_host_follows_the_given_order_and_wraps() {
        let mut cm = manager(&["a", "b", "c"]);
        let order = ["c", "a", "b"];
        assert_eq!(cm.neighbour_host_tab(&order, true), Some(1), "a -> b");
        cm.set_current(1);
        assert_eq!(cm.neighbour_host_tab(&order, true), Some(2), "b wraps to c");
        assert_eq!(cm.neighbour_host_tab(&order, false), Some(0), "b -> a");
        cm.set_current(2);
        assert_eq!(
            cm.neighbour_host_tab(&order, false),
            Some(1),
            "c wraps to b"
        );
    }

    #[test]
    fn hosts_without_a_tab_are_skipped() {
        let cm = manager(&["a", "c"]);
        let order = ["a", "b", "c"];
        assert_eq!(cm.neighbour_host_tab(&order, true), Some(1), "b has no tab");
    }

    #[test]
    fn fewer_than_two_hosts_with_tabs_is_a_no_op() {
        let cm = manager(&["a", "a"]);
        assert_eq!(cm.neighbour_host_tab(&["a", "b"], true), None);
        assert_eq!(cm.neighbour_host_tab(&["a", "b"], false), None);
    }

    #[test]
    fn a_host_missing_from_the_order_is_still_reachable() {
        // Hosts folded into a collapsed sidebar group have no row.
        let mut cm = manager(&["a", "hidden"]);
        assert_eq!(cm.neighbour_host_tab(&["a"], true), Some(1));
        cm.set_current(1);
        assert_eq!(cm.neighbour_host_tab(&["a"], true), Some(0));
    }

    #[test]
    fn next_host_restores_its_last_focused_tab() {
        let mut cm = manager(&["a", "b", "b"]);
        cm.set_current(2);
        cm.set_current(0);
        assert_eq!(cm.neighbour_host_tab(&["a", "b"], true), Some(2));
        cm.set_current(1);
        cm.set_current(0);
        assert_eq!(cm.neighbour_host_tab(&["a", "b"], true), Some(1));
    }
}
