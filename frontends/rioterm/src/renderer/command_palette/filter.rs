use super::*;

impl CommandPalette {
    /// Filtered list of rows for the current mode. Modes share the same
    /// fuzzy-score + sort pipeline so typing behaves identically.
    pub(super) fn filtered_rows(&self) -> Vec<(i32, PaletteRow<'_>)> {
        let mut results: Vec<(i32, PaletteRow<'_>)> = match &self.mode {
            PaletteMode::Commands => {
                let op_query = self.op_query();
                if op_query != terminus_ui::palette_query::Query::Plain {
                    return self.op_rows(op_query);
                }
                let has_adaptive = self.has_adaptive_theme;
                let mut rows: Vec<(i32, PaletteRow<'_>)> = COMMANDS
                    .iter()
                    .filter(|cmd| {
                        if cmd.action == PaletteAction::ToggleAppearanceTheme {
                            return has_adaptive;
                        }
                        true
                    })
                    .filter_map(|cmd| {
                        let score = fuzzy_score(&self.query.value, cmd.title)?;
                        Some((
                            score,
                            PaletteRow::Command {
                                title: cmd.title,
                                shortcut: self.shortcut_for(cmd.action),
                                action: cmd.action,
                            },
                        ))
                    })
                    .collect();
                // Inline host jump: when the user is already typing, also
                // surface matching hosts so Ctrl+Shift+P → "prod" → Enter works
                // without entering Hosts mode first. Empty query keeps the
                // catalog command-only to avoid dumping a long host list.
                if !self.query.value.is_empty() {
                    for host in &self.hosts_cache {
                        if let Some(score) = host_fuzzy_score(&self.query.value, host) {
                            rows.push((
                                score,
                                PaletteRow::Host {
                                    id: host.id.as_str(),
                                    title: host.title.as_str(),
                                    subtitle: host.subtitle.as_str(),
                                },
                            ));
                        }
                    }
                }
                rows
            }
            PaletteMode::Fonts(fonts) => fonts
                .iter()
                .filter_map(|family| {
                    let score = fuzzy_score(&self.query.value, family)?;
                    Some((score, PaletteRow::Font { family }))
                })
                .collect(),
            PaletteMode::Hosts(hosts) => hosts
                .iter()
                .filter_map(|host| {
                    let score = host_fuzzy_score(&self.query.value, host)?;
                    Some((
                        score,
                        PaletteRow::Host {
                            id: host.id.as_str(),
                            title: host.title.as_str(),
                            subtitle: host.subtitle.as_str(),
                        },
                    ))
                })
                .collect(),
        };

        results.sort_by_key(|r| std::cmp::Reverse(r.0));
        // Servers list after commands so each group is contiguous (the
        // overlay palette draws one header per group). Stable: scores keep
        // their order inside a group.
        results.sort_by_key(|(_, row)| matches!(row, PaletteRow::Host { .. }));
        results
    }

    /// Rows of a `>` query. Scores are all equal: the order is the
    /// operation table's / the host and tunnel lists' own.
    pub(super) fn op_rows(
        &self,
        query: terminus_ui::palette_query::Query,
    ) -> Vec<(i32, PaletteRow<'_>)> {
        use terminus_ui::palette_query::{hints_for, ForwardOp, PaletteOp, Query};
        let tunnel_rows = |name: &str, want_active: Option<bool>, op: TunnelOp| {
            self.tunnels_cache
                .iter()
                .filter(|t| want_active.is_none_or(|a| t.active == a))
                .filter(|t| fuzzy_score(name, &t.name).is_some())
                .map(|t| {
                    (
                        0,
                        PaletteRow::Tunnel {
                            id: t.id.as_str(),
                            title: t.name.as_str(),
                            hint: t.hint.as_str(),
                            op,
                        },
                    )
                })
                .collect()
        };
        match query {
            Query::Plain | Query::Unknown => Vec::new(),
            Query::Hints(typed) => hints_for(&typed)
                .into_iter()
                .map(|h| {
                    (
                        0,
                        PaletteRow::OpHint {
                            title: h.label,
                            complete: h.complete,
                        },
                    )
                })
                .collect(),
            Query::Op(PaletteOp::Sftp { host }) => {
                let filter = host.unwrap_or_default();
                self.hosts_cache
                    .iter()
                    .filter(|h| host_fuzzy_score(&filter, h).is_some())
                    .map(|h| {
                        (
                            0,
                            PaletteRow::Host {
                                id: h.id.as_str(),
                                title: h.title.as_str(),
                                subtitle: h.subtitle.as_str(),
                            },
                        )
                    })
                    .collect()
            }
            Query::Op(PaletteOp::Forward(ForwardOp::List)) => {
                tunnel_rows("", None, TunnelOp::Show)
            }
            Query::Op(PaletteOp::Forward(ForwardOp::Start(name))) => {
                tunnel_rows(&name, Some(false), TunnelOp::Start)
            }
            Query::Op(PaletteOp::Forward(ForwardOp::Stop(name))) => {
                tunnel_rows(&name, Some(true), TunnelOp::Stop)
            }
        }
    }

    /// Group header a row belongs under.
    pub(super) fn group_of(row: &PaletteRow<'_>) -> &'static str {
        match row {
            PaletteRow::Command { .. } => "Commands",
            PaletteRow::Font { .. } => "Fonts",
            PaletteRow::Host { .. } => "Servers",
            PaletteRow::Tunnel { .. } => "Tunnels",
            PaletteRow::OpHint { .. } => "Commands",
        }
    }

    pub(super) fn placeholder(&self) -> &'static str {
        match self.mode {
            PaletteMode::Commands => "Search servers and commands",
            PaletteMode::Fonts(_) => "Type a font name",
            PaletteMode::Hosts(_) if self.host_pick == HostPick::Sftp => {
                "Choose a server to browse"
            }
            PaletteMode::Hosts(_) => "Type a server name",
        }
    }

    /// Rows as the overlay palette shows them (group, label, right hint).
    pub(super) fn row_specs(&self) -> Vec<terminus_ui::palette_view::RowSpec> {
        use terminus_ui::palette_view::RowSpec;
        self.filtered_rows()
            .iter()
            .map(|(_, row)| {
                let hint = match row {
                    PaletteRow::Command { shortcut, .. } => (*shortcut).to_string(),
                    PaletteRow::Host { subtitle, .. } => (*subtitle).to_string(),
                    PaletteRow::Font { .. } => "Copy".to_string(),
                    PaletteRow::Tunnel { hint, .. } => (*hint).to_string(),
                    PaletteRow::OpHint { .. } => String::new(),
                };
                RowSpec::new(Self::group_of(row), row.title(), hint)
            })
            .collect()
    }

    /// The overlay palette for the current scroll window.
    pub(super) fn view(&self) -> terminus_ui::components::overlay::Palette {
        terminus_ui::palette_view::visible_palette(
            &self.query.value,
            self.placeholder(),
            &self.row_specs(),
            self.scroll_offset,
            self.selected_index,
            MAX_VISIBLE_RESULTS,
        )
    }
}
