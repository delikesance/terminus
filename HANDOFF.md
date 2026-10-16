# HANDOFF — état courant

Mis à jour : 2026-10-16 (branche `development`, HEAD `80f3a01e`, poussée sur `origin/development`). Protocole : voir CLAUDE.md § HANDOFF.md.

## Objectif en cours
R1 (`milestone.md`) = spec per-host / raccourcis (`docs/superpowers/specs/2026-10-13-per-host-scope-and-shortcuts-design.md`, relue 2026-10-16, statut « reviewed »).
- R1a : **fait** (implémenté, tests verts, non vérifié en GUI).
- R1b : **en cours**, plan validé (7 étapes) : 1 doublon `[`/`]` ✅ (non commité) ; 2 variantes `Action` + parser ; 3 helper cyclage d'hôte (test rouge d'abord) ; 4 câblage `Screen` (`screen/sessions/hosts_nav.rs`) ; 5 défauts dans `bindings/host_nav.rs` (neuf, `mod.rs` à 1870 lignes allowlistées) ; 6 palette hôte suivant/précédent ; 7 milestone + HANDOFF.
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
1. Commiter l'étape 1 de R1b (`git commit -- frontends/rioterm/src/bindings/mod.rs HANDOFF.md`).
2. R1b étape 2. Question ouverte étape 3 : accessor de l'ordre des hôtes de la sidebar (`terminus-ui/src/sidebar/state.rs`) ; sinon repli sur l'ordre des onglets (à trancher avec l'utilisateur).
3. Vérifier R1a à l'écran (voir Blocages).

## Pièges à retenir (détail dans CLAUDE.md)
- `git commit` sans chemin embarque les suppressions stagées : toujours `git commit -- <chemins>`.
- Fichiers non suivis : `git add -- <chemins>` avant `git commit -- <chemins>`.
- `nix` absent de cette machine : `cargo test -p terminus-ui` / `-p rioterm` fonctionnent directement (fontconfig présent).

## État du working tree
Non commité : `frontends/rioterm/src/bindings/mod.rs` (R1b étape 1), `HANDOFF.md`. Suppressions stagées préexistantes : ne pas commiter.

## R1b étape 1 (fait)
- Stub `#[cfg(test)] platform_key_bindings` (vide) supprimé : les tests voient maintenant la vraie table de la plateforme hôte.
- Nouveau test `defaults_bind_each_combination_to_one_action` (via `triggers_match`) ; chaînes voulues exemptées nommément (Copy+ClearSelection, ClearLogNotice+^L, ToggleViMode+ScrollToBottom). L'ancien test ne voyait pas le doublon (il exigeait la même action).
- Retrait de Ctrl+Shift+[ / ] → `SelectPrev/NextTab` (Linux, Windows). Windows/macOS non compilés ici.

## Validations (R1b étape 1, cette session)
- Repro : nouveau test en échec (doublons `[`/`]`), puis vert après correctif.
- `cargo test -p rioterm` : exit 0, 444 tests.
- `cargo fmt --all -- --check` : exit 0 ; `scripts/check-file-size.sh` : exit 0.
- `cargo clippy -p rioterm --all-targets` : aucun avertissement sur `bindings/mod.rs`.
- Non fait : vérification GUI de R1a ; CI distante.
