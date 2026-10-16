# HANDOFF — état courant

Mis à jour : 2026-10-16 (branche `development`, non poussée). Protocole : voir CLAUDE.md § HANDOFF.md.

## Objectif en cours
Feuille de route `milestone.md` en autopilote. R1 (spec per-host / raccourcis, `docs/superpowers/specs/2026-10-13-per-host-scope-and-shortcuts-design.md`) : R1a et R1b faites côté code, non vérifiées en GUI ; R1c (page Settings > Shortcuts) non commencée.

## Fait (commits sur `development`, non poussés)
- R1a : `66fa2c9f`, `3d2f6982`, `10efc5de`. R1b : `a4f5bf7e`, `8f9fcfb0`, `c66d1a0e`, `e78de80c`, `df8ae3fb`, `1af36884`.
- R9 CTA « Add first host » : `ce875184` (+ docs `e1d7eef1`). Non vérifié en GUI. Pas de scaffold en 3 étapes.
- R6/D2 stub bridge supprimé : `dcb85303` (+ docs `7710cb7a`, CLAUDE.md corrigé). D2 retirée de milestone.md.
- R4 compteur d'octets des tunnels, **estimation** `/proc/<pid>/io` (Linux, affiché `~`) : `30bb7db0` (+ docs `0e094e22`). Non vérifié en GUI. Exact = `sock_diag`, ouvert.
- R8 sidebar redimensionnable, **partiel** : état/géométrie/setting dans `terminus-ui` : `a705e82c` (+ docs `1d32b486`).
- R5 sync visible, **partiel** : badge « Synced … ago » / conflits sur le bouton Settings : `7b679ded` (+ docs `4e891f16`). Non vérifié en GUI.

## Décisions
- Split = session de l'hôte (`screen/sessions/split.rs:41`) ; Option A conservée pour les chemins SSH.
- Ctrl+Shift+Up/Down (hôte) et Ctrl+Alt+1..5 (vues) ; `SelectTab(n)` relatif à l'hôte ; hôte sans onglet sauté.
- Pas de test `Screen` (pas de harnais) : le câblage `screen/sessions/*` n'est couvert que par compilation, clippy et revue.
- Intégration des worktrees : `git cherry-pick -n` + `git commit -C <sha> -- <chemins>` (un `cherry-pick` direct refuse l'index sale des suppressions stagées).

## Blocages / à trancher
- **Vérification à l'écran non faite** : R1a/R1b (deux hôtes, Files/Terminal, Ctrl+Tab, Ctrl+Shift+Up/Down, Ctrl+Alt+1..5), R9, R4, R5. Via `scripts/screenshot.sh` ou à la main.
- **R2 (CI build + tests + clippy, D3)** : en attente de choix (hook pre-commit versionné, job CI, ou les deux).
- **R5** : `SyncReport.conflicts` n'est jamais incrémenté par `terminus-core` (le badge conflits reste à 0) ; pas de badge « pending » ; âge relatif non rafraîchi sur timer ; pas de revue des conflits.
- **R8** : reste à câbler `SIDEBAR_WIDTH` (`ShellLayout::new`, `shell/sidebar.rs:106`, `sidebar::WIDTH`, `chrome/state.rs:9`, `min_window_size`) et côté `rioterm` : lecture du setting `sidebar_width` au démarrage, souris/curseur, poignée, `reapply_chrome_inset`, `set_setting` au relâchement.
- Suppressions stagées préexistantes (`.claude/agents/*`, `.claude/settings.json`, `.codex/*`, `.cursor/*`, `AGENTS.md`) : non liées, ne pas commiter.
- Rien n'est poussé.

## Prochaine action
1. Vérifier à l'écran R1a/R1b/R9/R4/R5 (`scripts/screenshot.sh`).
2. Câbler R8 (voir ci-dessus), puis R1c.
3. Trancher R2.

## Validations à rejouer
`cargo fmt --all -- --check` ; `scripts/check-file-size.sh` ; `cargo test -p terminus-ui -p terminus-core -p terminus-bridge -p rioterm` ; `cargo clippy -p rioterm -p terminus-ui --all-targets` (avertissements préexistants : `user_event.rs`, `layout/`, `renderer/mod.rs`, `screen/render.rs`, crates amont).

## Pièges à retenir (détail dans CLAUDE.md)
- `git commit` sans chemin embarque les suppressions stagées : toujours `git commit -- <chemins>` ; fichiers non suivis : `git add -- <chemins>` avant.
- `nix` absent de cette machine : `cargo test -p terminus-ui` / `-p rioterm` fonctionnent directement.
- Cible cargo partagée entre worktrees concurrents : faux « no method » possible ; `touch` des sources du crate puis relancer.
