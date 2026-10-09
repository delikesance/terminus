//! Command-palette `>` syntax: `>sftp [host]` and
//! `>forward [list|start <name>|stop <name>]`.
//!
//! Pure text parsing, no state and no painting: the palette decides what
//! rows to show for the parsed [`Query`] and the screen runs the action.

/// What a `>` query asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaletteOp {
    /// `>sftp [host]`: browse a host's files (host is a fuzzy filter).
    Sftp { host: Option<String> },
    /// `>forward ...`: port forwards (tunnels).
    Forward(ForwardOp),
}

/// `>forward` subcommands. Names are the rest of the line (may hold spaces)
/// and may be empty: the palette then lists every candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForwardOp {
    List,
    Start(String),
    Stop(String),
}

/// How the palette input text reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Query {
    /// No leading `>`: the regular fuzzy search.
    Plain,
    /// A complete operation.
    Op(PaletteOp),
    /// An unfinished operation (`>`, `>sf`, `>forward st`): offer
    /// completions. Carries the lowercased text after `>`, trimmed.
    Hints(String),
    /// Starts with `>` but matches nothing known.
    Unknown,
}

/// One completion offered while an operation is being typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpHint {
    /// Row title.
    pub label: &'static str,
    /// Text the query becomes when the row is confirmed.
    pub complete: &'static str,
}

/// Every operation, in display order.
pub const OP_HINTS: &[OpHint] = &[
    OpHint {
        label: "SFTP: browse a server",
        complete: ">sftp ",
    },
    OpHint {
        label: "Forward: list tunnels",
        complete: ">forward list",
    },
    OpHint {
        label: "Forward: start a tunnel",
        complete: ">forward start ",
    },
    OpHint {
        label: "Forward: stop a tunnel",
        complete: ">forward stop ",
    },
];

/// Completions for the text typed after `>` (lowercased, trimmed).
pub fn hints_for(typed: &str) -> Vec<OpHint> {
    OP_HINTS
        .iter()
        .filter(|h| h.complete[1..].starts_with(typed))
        .copied()
        .collect()
}

/// First whitespace-separated word and the trimmed remainder.
fn split_word(text: &str) -> (&str, &str) {
    match text.split_once(char::is_whitespace) {
        Some((word, rest)) => (word, rest.trim()),
        None => (text, ""),
    }
}

/// Parse the palette input.
pub fn parse(input: &str) -> Query {
    let Some(body) = input.trim().strip_prefix('>') else {
        return Query::Plain;
    };
    let body = body.trim_start();
    let (verb, rest) = split_word(body);
    let verb_lower = verb.to_lowercase();
    match verb_lower.as_str() {
        "sftp" => Query::Op(PaletteOp::Sftp {
            host: (!rest.is_empty()).then(|| rest.to_string()),
        }),
        "forward" => {
            let (sub, name) = split_word(rest);
            let forward = |op| Query::Op(PaletteOp::Forward(op));
            match sub.to_lowercase().as_str() {
                "" | "list" => forward(ForwardOp::List),
                "start" => forward(ForwardOp::Start(name.to_string())),
                "stop" => forward(ForwardOp::Stop(name.to_string())),
                partial if name.is_empty() => hints_query(&format!("forward {partial}")),
                _ => Query::Unknown,
            }
        }
        _ if rest.is_empty() => hints_query(&verb_lower),
        _ => Query::Unknown,
    }
}

/// `Hints` when some operation still completes `typed`, else `Unknown`.
fn hints_query(typed: &str) -> Query {
    if hints_for(typed).is_empty() {
        Query::Unknown
    } else {
        Query::Hints(typed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sftp(host: Option<&str>) -> Query {
        Query::Op(PaletteOp::Sftp {
            host: host.map(str::to_string),
        })
    }

    fn fwd(op: ForwardOp) -> Query {
        Query::Op(PaletteOp::Forward(op))
    }

    #[test]
    fn no_chevron_is_a_plain_search() {
        assert_eq!(parse(""), Query::Plain);
        assert_eq!(parse("sftp"), Query::Plain);
        assert_eq!(parse("forward start x"), Query::Plain);
        assert_eq!(parse("a > b"), Query::Plain);
    }

    #[test]
    fn sftp_with_and_without_a_host() {
        assert_eq!(parse(">sftp"), sftp(None));
        assert_eq!(parse(">sftp "), sftp(None));
        assert_eq!(parse(">sftp prod"), sftp(Some("prod")));
        assert_eq!(parse(">sftp prod box"), sftp(Some("prod box")));
    }

    #[test]
    fn whitespace_and_case_are_forgiven() {
        assert_eq!(parse("  >SFTP   Prod  "), sftp(Some("Prod")));
        assert_eq!(parse("> sftp prod"), sftp(Some("prod")));
        assert_eq!(parse(">Forward LIST"), fwd(ForwardOp::List));
        assert_eq!(
            parse(">forward START db"),
            fwd(ForwardOp::Start("db".into()))
        );
    }

    #[test]
    fn forward_defaults_to_list() {
        assert_eq!(parse(">forward"), fwd(ForwardOp::List));
        assert_eq!(parse(">forward "), fwd(ForwardOp::List));
        assert_eq!(parse(">forward list"), fwd(ForwardOp::List));
    }

    #[test]
    fn forward_start_and_stop_take_the_rest_as_the_name() {
        assert_eq!(
            parse(">forward start web 8080"),
            fwd(ForwardOp::Start("web 8080".into()))
        );
        assert_eq!(
            parse(">forward stop   db  "),
            fwd(ForwardOp::Stop("db".into()))
        );
    }

    #[test]
    fn forward_start_and_stop_may_have_no_name_yet() {
        assert_eq!(
            parse(">forward start"),
            fwd(ForwardOp::Start(String::new()))
        );
        assert_eq!(parse(">forward stop "), fwd(ForwardOp::Stop(String::new())));
    }

    #[test]
    fn a_bare_chevron_offers_every_operation() {
        assert_eq!(parse(">"), Query::Hints(String::new()));
        assert_eq!(parse(" > "), Query::Hints(String::new()));
        assert_eq!(hints_for("").len(), OP_HINTS.len());
    }

    #[test]
    fn partial_verbs_offer_completions() {
        assert_eq!(parse(">sf"), Query::Hints("sf".into()));
        assert_eq!(parse(">FOR"), Query::Hints("for".into()));
        let hints = hints_for("sf");
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].complete, ">sftp ");
    }

    #[test]
    fn partial_forward_subcommands_offer_completions() {
        assert_eq!(parse(">forward st"), Query::Hints("forward st".into()));
        let completes: Vec<&str> =
            hints_for("forward st").iter().map(|h| h.complete).collect();
        assert_eq!(completes, [">forward start ", ">forward stop "]);
        let completes: Vec<&str> =
            hints_for("forward l").iter().map(|h| h.complete).collect();
        assert_eq!(completes, [">forward list"]);
    }

    #[test]
    fn unknown_operations_match_nothing() {
        assert_eq!(parse(">foo"), Query::Unknown);
        assert_eq!(parse(">sftpx"), Query::Unknown);
        assert_eq!(parse(">forward star foo"), Query::Unknown);
        assert_eq!(parse(">forward restart"), Query::Unknown);
        assert_eq!(parse(">foo bar"), Query::Unknown);
    }

    #[test]
    fn every_hint_completion_parses_to_something() {
        for hint in OP_HINTS {
            assert!(
                matches!(parse(hint.complete), Query::Op(_)),
                "{}",
                hint.complete
            );
        }
    }
}
