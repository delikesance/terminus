# HANDOFF — état courant

Mis à jour : 2026-10-16 (branche `development`, HEAD `80f3a01e`, poussée sur `origin/development`). Protocole : voir CLAUDE.md § HANDOFF.md.

## Objectif en cours
R1 (`milestone.md`) = spec per-host / raccourcis (`docs/superpowers/specs/2026-10-13-per-host-scope-and-shortcuts-design.md`, relue 2026-10-16, statut « reviewed »).
- R1a : **fait** (implémenté, tests verts, non vérifié en GUI).
- R1b : **en cours**, plan validé (7 étapes) : 1 doublon `[`/`]` ✅ (`a4f5bf7e`) ; 2 variantes `Action` + parser ✅ (non commité) ; 3 helper cyclage d'hôte (test rouge d'abord) ; 4 câblage `Screen` (`screen/sessions/hosts_nav.rs`) ; 5 défauts dans `bindings/host_nav.rs` (neuf, `mod.rs` à 1870 lignes allowlistées) ; 6 palette hôte suivant/précédent ; 7 milestone + HANDOFF.
- R1c : page Settings > Shortcuts, non commencée.
- R16 (Files suit le dossier OSC 7 du terminal courant) et R17 (fil d'Ariane éditable) : ajoutés à `milestone.md`, non planifiés. R16 touche `ShowView` : à faire après R1b.

## Fait
- `80f3a01e` : R16 / R17 ajoutés à `milestone.md` (audit : OSC 7 lu seulement à la création de la session SFTP, sur l'onglet le plus récent de l'hôte ; pas de saisie de chemin).
- `66fa2c9f` : `Workspace` retient la vue par hôte (`leave_host`/`enter_host`/`follow_host`/`forget_host`) ; `machine_selected` supprimé.
- `3d2f6982` : cyclage Ctrl+Tab, `SelectTab(n)`, `SelectLastTab` limités à l'hôte (`context/host_tabs.rs`). Test `host_tab_to_restore_is_the_last_focused_one` adapté (le cyclage ne traverse plus les hôtes).
- `10efc5de` : point de synchro `sync_sidebar_selection` → `follow_host` ; nouvelle session = Terminal ; fermeture du dernier onglet d'un hôte et suppression d'hôte = oubli de la vue.
- `7cb77c5a` : spec relue et corrigée (doublon Ctrl+Shift+[ / ], raccourcis, liste PTY réservée, découpage R1a/R1b/R1c).
- `94a0aa8e` : règle 5 de CLAUDE.md (milestone.md).

## Décisions
- Split = session de l'hôte : un split depuis un pane SSH/WSL ouvre la même session (`screen/sessions/split.rs:41`, `SplitTarget::Row` dans `screen/sessions/mod.rs:26`, commit `4f803042`). Seul un pane local ouvre un shell local. Absent de `milestone.md` ; non vérifié en GUI.
- Chemins SSH : Option A conservée (voir CLAUDE.md, milestone.md §2).
- Ctrl+Shift+Up/Down et Ctrl+Alt+1..5 pour R1b (Alt+digit = readline/tmux ; Ctrl+Alt+arrows = GNOME workspaces).
- `SelectTab(n)` devient relatif à l'hôte, y compris macOS (accepté).
- Un hôte sans onglet est sauté par `SelectNextHost` (volontaire).
- Pas de test `Screen` : pas de harnais ; le câblage `screen/sessions/*` n'est couvert que par compilation, clippy et revue.

## Blocages
- Vérification à l'écran de R1a non faite : deux hôtes, Files sur l'un, Terminal sur l'autre, Ctrl+Tab. Via `scripts/screenshot.sh` (Xvfb, lavapipe) ou à la main.
- Enforcement de la règle milestone.md : en attente de choix (hook pre-commit versionné, job CI, ou les deux).
- Suppressions stagées préexistantes dans l'index (`.claude/agents/*`, `.claude/settings.json`, `.codex/*`, `.cursor/*`, `AGENTS.md`) : non liées, ne pas commiter sans demande.

## Prochaine action
1. Commiter l'étape 2 (`git commit -- frontends/rioterm/src/bindings/mod.rs HANDOFF.md`).
2. R1b étape 3 : helper de cyclage d'hôte (test rouge d'abord). Question ouverte : ordre des hôtes = accessor dans `terminus-ui/src/sidebar/state.rs`, sinon repli sur l'ordre des onglets (à trancher avec l'utilisateur).
3. Vérifier R1a à l'écran (voir Blocages).

## Pièges à retenir (détail dans CLAUDE.md)
- `git commit` sans chemin embarque les suppressions stagées : toujours `git commit -- <chemins>`.
- Fichiers non suivis : `git add -- <chemins>` avant `git commit -- <chemins>`.
- `nix` absent de cette machine : `cargo test -p terminus-ui` / `-p rioterm` fonctionnent directement (fontconfig présent).

## État du working tree
Non commité : `frontends/rioterm/src/bindings/mod.rs` (R1b étape 2), `HANDOFF.md`. Suppressions stagées préexistantes : ne pas commiter.

## R1b étape 2 (fait)
- `Action::{SelectNextHost, SelectPrevHost, ShowView(WorkspaceView)}` + parser config (`selectnexthost`, `selectprevhost`, `showview(terminal|files|tunnels|snippets|history)`, inconnu → `None`). Pas encore câblé dans `screen/keys.rs` (le `_ => ()` les ignore) ni lié par défaut.
- Test `host_and_view_actions_parse_from_config_strings` : rouge (variantes absentes), puis vert.
- Validations : `cargo test -p rioterm` exit 0 (445), `cargo fmt --check` exit 0, `check-file-size.sh` exit 0, clippy sans avertissement sur `bindings/mod.rs`.
