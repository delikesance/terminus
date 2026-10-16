# HANDOFF — état courant

Mis à jour : 2026-10-16 (branche `development`, HEAD `docs: mark R1a done`). Protocole : voir CLAUDE.md § HANDOFF.md.

## Objectif en cours
R1 (`milestone.md`) = spec per-host / raccourcis (`docs/superpowers/specs/2026-10-13-per-host-scope-and-shortcuts-design.md`, relue 2026-10-16, statut « reviewed »).
- R1a : **fait** (implémenté, tests verts, non vérifié en GUI).
- R1b : **prochaine étape** (actions `ShowView`, `SelectNextHost`/`SelectPrevHost`, défauts Ctrl+Shift+Up/Down et Ctrl+Alt+1..5, suppression du doublon Ctrl+Shift+[ / ] onglet/split). Pas encore planifiée.
- R1c : page Settings > Shortcuts, non commencée.

## Fait
- `66fa2c9f` : `Workspace` retient la vue par hôte (`leave_host`/`enter_host`/`follow_host`/`forget_host`) ; `machine_selected` supprimé.
- `3d2f6982` : cyclage Ctrl+Tab, `SelectTab(n)`, `SelectLastTab` limités à l'hôte (`context/host_tabs.rs`). Test `host_tab_to_restore_is_the_last_focused_one` adapté (le cyclage ne traverse plus les hôtes).
- `10efc5de` : point de synchro `sync_sidebar_selection` → `follow_host` ; nouvelle session = Terminal ; fermeture du dernier onglet d'un hôte et suppression d'hôte = oubli de la vue.
- `7cb77c5a` : spec relue et corrigée (doublon Ctrl+Shift+[ / ], raccourcis, liste PTY réservée, découpage R1a/R1b/R1c).
- `94a0aa8e` : règle 5 de CLAUDE.md (milestone.md).

## Décisions
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
1. Vérifier R1a à l'écran (voir Blocages), puis mettre à jour milestone.md (« non vérifié en GUI » retiré seulement après capture).
2. Planifier R1b : plan court avant écriture (fichiers : `bindings/mod.rs` — 1870 lignes, à découper —, `screen/keys.rs`, `renderer/command_palette/action.rs`).
3. Retirer la paire Ctrl+Shift+[ / ] de `SelectPrev/NextTab` (Linux/Windows) avant d'ajouter les nouveaux défauts.

## Pièges à retenir (détail dans CLAUDE.md)
- `git commit` sans chemin embarque les suppressions stagées : toujours `git commit -- <chemins>`.
- Fichiers non suivis : `git add -- <chemins>` avant `git commit -- <chemins>`.
- `nix` absent de cette machine : `cargo test -p terminus-ui` / `-p rioterm` fonctionnent directement (fontconfig présent).

## État du working tree
Non commité : `HANDOFF.md`, `milestone.md` (commit « docs: mark R1a done » juste après ce fichier). Suppressions stagées préexistantes : ne pas commiter.

## Validations (R1a, cette session)
- `cargo test -p terminus-ui` : exit 0, 686 tests.
- `cargo test -p rioterm` : exit 0, 443 tests.
- `cargo fmt --all -- --check` : exit 0.
- `scripts/check-file-size.sh` : exit 0.
- `cargo clippy -p rioterm -p terminus-ui --all-targets` : aucun avertissement sur les fichiers touchés.
- Non fait : vérification GUI ; CI distante non relancée.
