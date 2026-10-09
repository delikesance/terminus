use super::*;

/// What the palette is currently browsing and filtering over.
///
/// `Commands` is the default — fuzzy-matches against the static
/// `COMMANDS` list (and matching hosts when the query is non-empty)
/// and dispatches a `PaletteAction` / opens a host on Enter.
///
/// `Fonts` is entered via the `ListFonts` command. The palette stays
/// open, its content is replaced with the owned list of font family
/// names, and Enter closes the palette (no font-switching action yet).
/// The list is owned so the filter pass doesn't keep a borrow on the
/// sugarloaf FontLibrary.
///
/// `Hosts` is entered via `ListHosts` ("Open Host…"). Same stay-open
/// pattern as Fonts; Enter opens an SSH session for the selected host.
pub(super) enum PaletteMode {
    Commands,
    Fonts(Vec<String>),
    Hosts(Vec<HostPaletteItem>),
}

/// What confirming a host in the palette's hosts list does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostPick {
    /// Open (or focus) a shell session.
    Session,
    /// Open the SFTP dual pane for it.
    Sftp,
}

/// One stored SSH host as the palette needs it (no secrets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostPaletteItem {
    pub id: String,
    /// Display name from the store.
    pub title: String,
    /// `user@host[:port]` for secondary matching / hint.
    pub subtitle: String,
}

/// What confirming a tunnel row in the palette does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TunnelOp {
    /// `>forward start <name>`.
    Start,
    /// `>forward stop <name>`.
    Stop,
    /// `>forward list`: jump to the Tunnels view.
    Show,
}

/// One tunnel of the machine on screen as the palette needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunnelPaletteItem {
    pub id: String,
    pub name: String,
    /// Right-hand hint: route and state.
    pub hint: String,
    /// A process exists (Stop applies, Start does not).
    pub active: bool,
}

/// One row in the filtered result list. Variants carry exactly the
/// data the render pass needs — no `&'static Command` vs `&str`
/// lifetime mixing.
pub(super) enum PaletteRow<'a> {
    Command {
        title: &'a str,
        shortcut: &'a str,
        action: PaletteAction,
    },
    Font {
        family: &'a str,
    },
    Host {
        id: &'a str,
        title: &'a str,
        subtitle: &'a str,
    },
    Tunnel {
        id: &'a str,
        title: &'a str,
        hint: &'a str,
        op: TunnelOp,
    },
    /// Completion of an unfinished `>` query.
    OpHint {
        title: &'static str,
        complete: &'static str,
    },
}

impl<'a> PaletteRow<'a> {
    pub(super) fn title(&self) -> &'a str {
        match *self {
            PaletteRow::Command { title, .. } => title,
            PaletteRow::Font { family } => family,
            PaletteRow::Host { title, .. } => title,
            PaletteRow::Tunnel { title, .. } => title,
            PaletteRow::OpHint { title, .. } => title,
        }
    }

    pub(super) fn shortcut(&self) -> &'a str {
        match *self {
            PaletteRow::Command { shortcut, .. } => shortcut,
            PaletteRow::Font { .. }
            | PaletteRow::Host { .. }
            | PaletteRow::Tunnel { .. }
            | PaletteRow::OpHint { .. } => "",
        }
    }

    pub(super) fn action(&self) -> Option<PaletteAction> {
        match *self {
            PaletteRow::Command { action, .. } => Some(action),
            PaletteRow::Font { .. }
            | PaletteRow::Host { .. }
            | PaletteRow::Tunnel { .. }
            | PaletteRow::OpHint { .. } => None,
        }
    }
}

/// Fuzzy match: checks if all query chars appear in order in the target.
/// Returns a score (higher = better match), or None if no match.
pub(super) fn fuzzy_score(query: &str, target: &str) -> Option<i32> {
    let query_lower: Vec<char> = query.to_lowercase().chars().collect();
    let target_lower: Vec<char> = target.to_lowercase().chars().collect();

    if query_lower.is_empty() {
        return Some(0);
    }

    let mut qi = 0;
    let mut score: i32 = 0;
    let mut prev_match = false;
    let mut first_match_pos = None;

    for (ti, &tc) in target_lower.iter().enumerate() {
        if qi < query_lower.len() && tc == query_lower[qi] {
            if first_match_pos.is_none() {
                first_match_pos = Some(ti);
            }
            // Consecutive match bonus
            if prev_match {
                score += 5;
            }
            // Word boundary bonus (start of string or after space/punctuation)
            if ti == 0 || !target_lower[ti - 1].is_alphanumeric() {
                score += 10;
            }
            prev_match = true;
            qi += 1;
        } else {
            prev_match = false;
        }
    }

    if qi < query_lower.len() {
        return None; // Not all query chars matched
    }

    // Bonus for matching near the start
    if let Some(pos) = first_match_pos {
        score += (20_i32).saturating_sub(pos as i32);
    }

    Some(score)
}

/// Best fuzzy score across a host's searchable fields (empty query → 0).
pub(super) fn host_fuzzy_score(query: &str, host: &HostPaletteItem) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    [
        fuzzy_score(query, &host.title),
        fuzzy_score(query, &host.subtitle),
        fuzzy_score(query, &host.id),
    ]
    .into_iter()
    .flatten()
    .max()
}
