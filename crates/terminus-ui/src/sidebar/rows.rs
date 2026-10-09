use super::*;
use crate::icons::Icon;
use crate::os_icons::HostStatus;

/// What a row opens, and therefore which glyph it carries.
///
/// The badge is the panel's whole visual taxonomy: it is what tells a
/// local shell apart from a WSL distro apart from an SSH host without
/// reading the subtitle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Badge {
    /// The machine terminus runs on: a local shell.
    Local,
    /// A WSL distro of the Windows machine this one is nested in.
    Wsl,
    /// A stored SSH host.
    Ssh,
}

impl Badge {
    pub fn icon(self) -> Icon {
        match self {
            Badge::Local => Icon::Monitor,
            Badge::Wsl => Icon::SquareTerminal,
            Badge::Ssh => Icon::Server,
        }
    }
}

/// One host, as the list draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostItem {
    /// Echoed back by [`PanelHit::Item`]'s row, and what the frontend
    /// resolves a session from: `local`, `wsl:<distro>` or a host id.
    pub id: String,
    pub name: String,
    /// `user@host:port`, or the distro's state, already formatted.
    pub endpoint: String,
    pub badge: Badge,
    /// Whether the row came from the host store.
    ///
    /// The platform rows — this computer and the Windows distros — are
    /// always there because they are where terminus runs, so they are
    /// listed without being counted: the header's number has to agree with
    /// the `Hosts` section, which is the list the user edits.
    pub stored: bool,
    /// Stable OS key for brand glyphs (`nixos`, `ubuntu`, …).
    pub os_id: Option<String>,
    /// Live status dot (running WSL, active SSH tab, …).
    pub status: HostStatus,
    /// Indented under a collapsible group header.
    pub nested: bool,
    /// Open terminal sessions currently attached to this host.
    pub session_count: usize,
}

/// One open terminal session, as the list draws it under its host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionItem {
    /// Index into the window's tab/`ContextManager` list.
    pub tab_index: usize,
    /// Parent host id (`local`, `wsl:…`, ssh id).
    pub host_id: String,
    pub title: String,
    /// Whether this is the focused terminal.
    pub active: bool,
    /// Whether the row may show a close affordance (not the pinned home).
    pub closable: bool,
}

/// One line of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// A section label. Not selectable, not clickable.
    Section(String),
    /// A collapsible host group from the store.
    Group {
        id: String,
        name: String,
        host_count: usize,
        /// Open sessions across all hosts in this group.
        session_count: usize,
        collapsed: bool,
    },
    Host(HostItem),
    /// Open terminal under a host (always follows its parent host row).
    Session(SessionItem),
}

impl Row {
    pub fn height(&self) -> f32 {
        match self {
            // Gap above + label box (the first visible header drops the gap,
            // see [`HostPanel::row_slot_height`]).
            Row::Section(_) | Row::Group { .. } => SECTION_GAP + SECTION_HEIGHT,
            Row::Host(_) => ITEM_HEIGHT + CARD_GAP,
            // Mid-sibling default; [`HostPanel::row_slot_height`] widens after
            // the last session under a host.
            Row::Session(_) => SESSION_HEIGHT + SESSION_GAP,
        }
    }

    /// Painted card height inside the row slot (excludes the trailing gap).
    pub fn card_height(&self) -> f32 {
        match self {
            Row::Section(_) | Row::Group { .. } => SECTION_HEIGHT,
            Row::Host(_) => ITEM_HEIGHT,
            Row::Session(_) => SESSION_HEIGHT,
        }
    }

    pub fn host(&self) -> Option<&HostItem> {
        match self {
            Row::Host(item) => Some(item),
            Row::Section(_) | Row::Group { .. } | Row::Session(_) => None,
        }
    }

    pub fn session(&self) -> Option<&SessionItem> {
        match self {
            Row::Session(item) => Some(item),
            _ => None,
        }
    }

    /// The section label, if this row is one.
    pub fn label(&self) -> Option<&str> {
        match self {
            Row::Section(label) => Some(label),
            Row::Group { .. } | Row::Host(_) | Row::Session(_) => None,
        }
    }

    /// Group metadata when this row is a collapsible group header.
    pub fn group(&self) -> Option<(&str, usize, bool)> {
        match self {
            Row::Group {
                name,
                host_count,
                collapsed,
                ..
            } => Some((name.as_str(), *host_count, *collapsed)),
            _ => None,
        }
    }
}
