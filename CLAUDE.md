# Terminus — CLAUDE.md

Fork de Rio (`raphamorim/rio`) : client SSH/SFTP + terminal GPU. Binaire `terminus` (crate `rioterm`). Dépôt `delikesance/terminus`. Version workspace : `Cargo.toml` (`[workspace.package]`, aujourd'hui 0.7.26). Rust épinglé 1.96.1 (`rust-toolchain.toml`). Branche de travail `development`, PR vers `main`.

## Règles de maintenance de ce fichier (obligatoires)

1. **Pièges** : dès qu'un piège est découvert (échec non évident, comportement contre-intuitif, commande qui ne marche que d'une certaine façon), ajouter une ligne à la section « Pièges » ci-dessous, avec la cause et le contournement. Une erreur commise deux fois y est obligatoirement.
2. **HANDOFF.md** (racine) : état courant de la session, à tenir à jour en continu (voir section dédiée).
3. CLAUDE.md = faits stables uniquement. L'état mouvant (tâche en cours, blocage, prochaine action) va dans HANDOFF.md, jamais ici.
4. Vérifier qu'un fait est toujours vrai avant de s'y fier (chemins, noms) ; corriger ce fichier s'il est périmé.
5. **milestone.md** (feuille de route) : à mettre à jour dans le même commit que tout changement de statut d'un jalon (⚠️ / ✅ / ❌), de dette (D1…Dn) ou de priorité. Un statut passe à ✅ seulement avec une preuve (fichier, symbole, test ou capture) et la mention « non vérifié en GUI » reste tant que l'écran n'a pas été vu. Une tâche terminée sans mise à jour de milestone.md n'est pas terminée. Un statut ne se déduit pas d'une mémoire : auditer le code avant d'écrire.

## HANDOFF.md — protocole

HANDOFF.md est une convention communautaire (pas une fonction native de Claude Code) : un fichier à la racine, écrit pour la **prochaine session Claude**, qui permet de reprendre sans ré-explorer. Sources : [issue anthropics/claude-code#11455](https://github.com/anthropics/claude-code/issues/11455), [skill handoff](https://skills.sh/steveclarke/dotfiles/handoff).

- **Début de session** : lire HANDOFF.md, puis ce fichier, avant toute exploration du code.
- **Mise à jour immédiate** à chaque jalon : décision prise, blocage, tâche terminée, piège trouvé, changement de cap. Aussi avant `/compact`, avant `/clear` et avant la fin d'une session longue. Ne pas attendre la fin.
- **Contenu** (court, actionnable) : objectif en cours ; fait (avec commits) ; décisions et leur raison ; blocages ; **prochaine action exacte** ; fichiers touchés non commités ; commandes de validation à rejouer ; questions ouvertes.
- **Hygiène** : supprimer l'obsolète, pas de journal, pas de doublon avec CLAUDE.md, aucun secret. Ne consigner que du vérifié (marquer « non vérifié » sinon).

## Disposition du dépôt

Workspace Cargo (`Cargo.toml`, 21 membres). Deux familles :

**Code Terminus (le nôtre)**
| Chemin | Rôle |
| --- | --- |
| `crates/terminus-core` | Domaine Tokio pur, sans UI : `store/` (SQLite : hosts, groups, identities, credentials, forwards, snippets, history, settings, migrate), `ssh/` (russh, known_hosts, probe), `sftp/`, `vault` (Argon2id), `sync`, `wsl`, `machine`, `os_detect`, `history`, `saved_tabs`, `managed_key`, `shell_integration`, `gssapi` |
| `crates/terminus-bridge` | Jonction core ↔ Rio : `sftp_worker/` (transferts, diff, conflits, retry), `ssh_transport` (`EventedPty` russh, **non câblé**), `folder_diff`, `modal_event_handlers`. `terminus_bridge_impl.rs` = stub |
| `crates/terminus-ui` | État, géométrie et hit-test du chrome, **sans GPU ni sugarloaf** : `chrome/`, `sidebar/`, `settings/`, `sftp_pane/`, `shell/` (dont `workspace.rs`), `add_host/`, `components/`, `views/`, `screens/`, `theme.rs`, `tokens.rs`, `icons.rs` |
| `crates/terminus-walk` | Binaire helper déposé sur l'hôte distant (hash de dossiers, sync SFTP différentielle) |
| `crates/terminus-update` | Auto-update (endpoint, clé publique, noms d'assets) |
| `frontends/rioterm` | Binaire `terminus`. Peinture (`renderer/`, `renderer/chrome.rs`), câblage input/sessions (`screen/`, `screen/sessions/`, `screen/workspace/`, `application/`, `router/`), `bindings/`, `hosts/` (worker SQLite), `sftp_ui/`, `tunnel_worker/`, `updater.rs`, `ssh_secrets.rs` |

**Code amont Rio (fork ; éviter de le modifier sans raison)** : `sugarloaf` (renderer Vulkan/Metal/wgpu/CPU), `teletypewriter` (PTY), `corcovado` (fork mio), `rio-backend` (config, défauts), `rio-window` (fork winit), `rio-vt`, `rio-grid`, `rio-graphics`, `rio-fonts`, `rio-unicode`, `rio-grapheme-width`, `rio-notifier`, `librio*`, `libsugarloaf`, `frontends/wasm`, `extra/man`.

Docs : `DEVELOPMENT.md` (boucle dev, captures, release, updater, Nix — **source de vérité**), `docs/ARCHITECTURE.md`, `milestone.md` (roadmap en français, réécrite le 2026-10-16 d'après le code ; statuts à tenir à jour avec preuve), `docs/superpowers/specs/*` (specs datées), `docs/linux-release.md`, `docs/sftp-test-matrix.md`, `docs/overlay-dialogs.md`.

## Architecture — invariants

- **Géométrie calculée une seule fois dans `terminus-ui`** (`*_rect`, largeurs `activity_bar::WIDTH`, `sidebar::WIDTH`). Le peintre (`rioterm/src/renderer/chrome.rs`) et le hit-test marchent sur les mêmes rects : un contrôle ne peut pas être dessiné là où on ne peut pas cliquer. Ne jamais dupliquer une largeur.
- Le chrome **réserve sa place dans la marge de la grille** (le terminal reflue à côté) ; `reapply_chrome_inset` se rejoue à tout changement de largeur, hot-reload config compris.
- Sugarloaf n'a **pas de rendu SVG** : icônes = chemins Lucide bruts (`terminus-ui/src/icons.rs`, **généré** par `scripts/gen-lucide-icons.py --write`, ne pas éditer à la main ; `gen-simple-icons.py` pour os_icons), rasterisés par `tiny-skia` en masque de couverture.
- Rendu immédiat à plat (pas de scene graph) : `Sugarloaf::render_with_grids`.
- **Chemins SSH volontairement distincts (Option A, décision 2026-09-16)** : onglet shell = PTY local + binaire `ssh` système (`screen/shell.rs::ssh_shell`, `sessions.rs::open_host_session`) ; tunnels = process `ssh -N -L/-R/-D` (`tunnel_worker`) ; SFTP = russh + `sftp_worker`. `SshTransport` est prêt mais non câblé (dette **1.4-debt**) : ne pas le brancher sans mettre à jour le roadmap et l'UX TOFU/auth. Shell CLI : `StrictHostKeyChecking=accept-new`.
- Identifiants de ligne de la liste d'hôtes : `local`, `wsl:<nom distro>`, ou l'id de l'hôte. `terminus-ui` ne connaît que cet id ; `Screen::open_host_session` le transforme en `Shell`.
- Stockage hôtes : SQLite derrière un thread worker (`rioterm/src/hosts`) pour que le thread UI n'attende jamais.
- Config : `config.toml`, rechargée à chaud. Variables : `TERMINUS_CONFIG_HOME`, `TERMINUS_DATA_DIR` (base disposable), `TERMINUS_LOG_LEVEL`, `TERMINUS_NO_UPDATE_CHECK=1`, `TERMINUS_UPDATE_URL`, `TERMINUS_COMPONENT_GALLERY`, `TERMINUS_VIEW_PREVIEW*`, `TERMINUS_DEV_*`.
- Polices UI : Sora et Martian Mono (embarquées dans `sugarloaf`) via `draw_ui_text`/`draw_mono_text`/`measure_ui_text` (`renderer/ui_text.rs`). Couleurs/métriques : `terminus_ui::theme::ChromeTheme`, `terminus_ui::tokens`.

## Commandes (déclarées par le projet)

Tout build de `rioterm` exige le **devshell nix** (voir Pièges). Les scripts s'y ré-exécutent seuls.

| But | Commande |
| --- | --- |
| Boucle dev (rebuild + restart) | `make dev-hot` ; une fois : `make dev-hot-once` ; sans fenêtre : `scripts/dev.sh --no-run` |
| Windows (cross `cargo-xwin`) | `make dev-hot-win` / `make dev-hot-win-once` |
| Déjà dans un devshell | préfixer `TERMINUS_DEV_NO_NIX=1` |
| Format | `cargo fmt --all -- --check` (`rustfmt.toml` : largeur 90) |
| Taille de fichier | `scripts/check-file-size.sh` |
| Tests UI sans GPU | `cargo test -p terminus-ui` |
| Tests d'un crate | `cargo test -p terminus-core` / `-p terminus-bridge` / `-p rioterm` (dans le devshell) |
| Capture headless (Xvfb) | `scripts/screenshot.sh [--click X,Y] [--text T] [--type T] [--send KEY] --out f.png` |
| Galerie de composants | `TERMINUS_COMPONENT_GALLERY=button scripts/screenshot.sh --out /tmp/button.png` |
| Release | `misc/prepare-release.sh X.Y.Z` puis `nix run .#release` (Docker requis pour Linux) ; `scripts/bump-version.sh` |

CI (`.github/workflows/ci.yml`) : **rustfmt + taille de fichier uniquement** ; build et tests tournent localement. Aucun hook git versionné détecté dans `.git/hooks` : lancer fmt + tests soi-même avant de conclure.

Cibles Make `dev`, `dev-watch`, `run` etc. = héritage Rio (macOS), ne pas s'y fier.

## Conventions

- Budget de fichier : soft 300 lignes, **hard 500** pour `crates/**/*.rs` et `frontends/rioterm/src/**/*.rs` (hors `icons.rs`, `os_icons.rs`). Au-delà : découper, ou entrée dans `scripts/file-size-allowlist.txt` — la liste **ne peut que rétrécir** (fichier repassé sous 500 → retirer son entrée, sinon erreur).
- Commits conventionnels (`feat(scope):`, `fix(scope):`, `chore(release):`, `docs(spec):`, `test(scope):`), scopes courants : `rioterm`, `settings`, `uploads`. Merges `development` → `main` par PR.
- Tag de release = `v` + version `Cargo.toml` (`release.sh` refuse sinon : l'updater compare les deux). Bumper et merger avant de publier.
- Mises à jour : l'installation n'est faite que si le SHA-256 correspond à `checksums.txt` (releases non signées).
- Tests sshd réels : `rioterm/src/screen/remote_upload_sshd_tests.rs`, `terminus-bridge/tests/real_sshd_e2e.rs`, `sftp_e2e.rs` (mock). Scripts : `scripts/test-host-wizard.py`, `test-linux-release.py`, `test-release-signing.sh`.
- Vérifier une UI : lancer l'app ou `scripts/screenshot.sh` et regarder le PNG (voir mémoire « Verify yourself ») ; tests verts ne prouvent pas le rendu.

## Pièges connus

- **Build hors nix = échec** : le shell exporte `PKG_CONFIG_PATH=/usr/lib/pkgconfig` sans `fontconfig.pc` → `yeslogic-fontconfig-sys` panique dans son build script. Utiliser `nix develop` / `scripts/dev.sh`.
- **Deux toolchains** : devshell = 1.96.1 (épinglé), rustup `stable` = 1.98.1 ; les artefacts ne sont pas partagés. Rester dans le devshell.
- **Le code Rust n'est pas « hot »** : une sauvegarde = rebuild incrémental + redémarrage (2–3 s). Seuls config et couleurs se rechargent à chaud ; une modif dans `themes/` n'est pas vue seule (watch non récursif) : toucher `config.toml`.
- **Redraw UI seul** : utiliser `Route::request_overlay_redraw`, pas `request_redraw` (Rio ne rend que si le contexte actif est « dirty » → capture avec l'ancienne frame).
- **Captures Xvfb** : `WAYLAND_DISPLAY` doit être vidé sinon capture noire sans erreur (le script le fait) ; entrées via XTEST seulement (`xdotool key --window` est ignoré par winit) ; pas de WM ; rendu lavapipe (CPU) : valide layout/logique, pas animations/GPU. Utiliser `TERMINUS_SCREENSHOT_DISPLAY=:93` pour captures concurrentes ; l'updater doit être coupé (`TERMINUS_NO_UPDATE_CHECK=1`, déjà fait par le script).
- **`bash` du devshell sans readline** : Tab/historique/Ctrl-R muets dans la fenêtre. Les scripts exportent un `SHELL` valide (`TERMINUS_SHELL` pour forcer). Test : `type -t bind` vide = pas de readline.
- **Prompt sandbox FHS** : rio lance un shell login (`bash --login`, `rio-backend/src/config/defaults.rs`) ; `dev/config/config.toml` met `[shell] args = []` pour la boucle dev seulement.
- **Interop WSL cassée dans un sandbox** (`/init` remplacé → `wsl.exe` : `Exec format error`) : `head -c 2 /init` = `#!` → stand-in. Les distros restent listées mais ne s'ouvrent pas.
- **Windows** : `target/` reste sur ext4 WSL, jamais sur `/mnt/c`. Un exe verrouillé n'est pas écrasé sans le watcher PowerShell (« mes changements ne se rebuildent pas »). Binaire installé nommé `tmnx.exe` depuis 0.7.1 (overlay Discord).
- **WSL** : la distro dans laquelle on est n'est pas relistée (c'est la ligne « this computer »). Distro trouvée seulement sur disque = sans nom utilisable, comptée `+1 unnamed on disk`.
- **`pub use` de `terminus-bridge`** (`HostKeyRequest`, `ModalDecision`…) volontairement commentés : les réactiver casse `cargo check` (livrés avec milestone 1.2/1.4).
- **Fichiers générés** : `icons.rs` (script Lucide), `*.spv` (build.rs de sugarloaf, gitignorés), `.dev/` (gitignoré, supprimer pour réinitialiser la boucle).
- **Git** : l'index contient des suppressions stagées préexistantes (`.claude/agents/*`, `.claude/settings.json`, `.codex/*`, `.cursor/*`, `AGENTS.md`) qui ne sont pas de notre fait ; ne pas les inclure dans un commit sans demande. `.claude/*` est gitignoré (sauf `settings.json` et `agents/`).
- `milestone.md` : un ancien suivi annonçait « pas de queue ni retry SFTP » et « CI build/test/clippy » ; le code dit l'inverse pour la file/retry (`sftp_worker/queue.rs`, `retry.rs`) et `ci.yml` ne fait que rustfmt + taille de fichier. Auditer le code avant de croire un statut.

## Spec en cours

`docs/superpowers/specs/2026-10-13-per-host-scope-and-shortcuts-design.md` (statut : en attente de relecture) : vue active par hôte (`Workspace.per_host`), Ctrl+Tab limité aux onglets de l'hôte, nouvelles actions `ShowView`/`SelectNextHost`/`SelectPrevHost`, page Settings pour remapper. Détail d'avancement : HANDOFF.md.
