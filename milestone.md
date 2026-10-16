# TERMINUS — Roadmap

> Fork de Rio (`raphamorim/rio`) · Version `0.7.26` (`Cargo.toml`) · Réécrit le
> 2026-10-16 à partir d'un audit du code de la branche `development` (HEAD
> `4ade5ab6`). Les statuts décrivent ce que le **code contient**; rien n'a été
> lancé ni testé pour cet audit, et les points « non vérifié en GUI » le restent.
>
> Règle de tenue : un statut ne passe à ✅ qu'avec une preuve (fichier, test ou
> capture). Corriger ce fichier dès qu'il diverge du code. L'état de session
> (tâche en cours, blocages) va dans `HANDOFF.md`, pas ici.

Légende : ✅ fait · ⚠️ partiel · ❌ absent · 📎 dette assumée

---

## 1. Produit

Client SSH / SFTP / tunnels avec terminal GPU, sur base Rio. Plateformes
livrées : Linux (tarball, deb, rpm, Nix) et Windows (NSIS, MSI, zip portable).
macOS : aucun artefact Terminus.

## 2. Architecture réelle

| Couche | Chemin | Rôle |
| :--- | :--- | :--- |
| Domaine | `crates/terminus-core` | Store SQLite, vault Argon2id, sync SQL, SSH/SFTP (russh), WSL, historique, onglets sauvegardés, détection d'OS. Sans UI |
| Jonction | `crates/terminus-bridge` | `sftp_worker`, `SshTransport` (`EventedPty`), `folder_diff`. Le stub `TerminusBridge` / `TerminusCoreService` a été **supprimé** (R6/D2, commit `dcb85303` : `terminus_bridge_impl.rs` et ses `pub mod`/`pub use` retirés de `lib.rs`) ; le `pub use` commenté de `modal_event_handlers` reste volontairement (voir CLAUDE.md) |
| Chrome | `crates/terminus-ui` | État, géométrie, hit-test, thèmes. Sans GPU |
| Helper distant | `crates/terminus-walk` | Binaire déposé sur l'hôte pour la sync différentielle |
| Mise à jour | `crates/terminus-update` | Vérification GitHub Releases, SHA-256 |
| Front | `frontends/rioterm` | Peinture sugarloaf, câblage input/sessions, worker hôtes, tunnels, mises à jour |

### Chemins SSH — décision acceptée (Option A, 2026-09-16)

| Chemin | Techno | Entrée |
| :--- | :--- | :--- |
| Onglet shell | PTY local + binaire `ssh` | `screen/shell.rs::ssh_shell` |
| Tunnels | processus `ssh -N -L/-R/-D` | `tunnel_worker/` |
| SFTP | russh + worker | `terminus-bridge::sftp_worker` |
| `SshTransport` | russh ↔ `EventedPty` | **non branché** : `SessionSpec::Ssh` existe mais est `#[allow(dead_code)]` (`context/mod.rs:15-24`) |

Conséquence : deux piles SSH. Host-key et auth peuvent diverger entre CLI
(`StrictHostKeyChecking=accept-new`) et russh. Dette tracée **D1**.

---

## 3. État par domaine

### 3.1 Chrome et hôtes

| Capacité | Statut | Preuve / limite |
| :--- | :--- | :--- |
| Activity bar + sidebar, marge de grille réservée | ✅ | `terminus-ui/src/shell/layout.rs`, `reapply_chrome_inset` |
| CRUD hôtes (assistant 3 étapes), groupes, DnD, renommage, menu contextuel | ✅ | `terminus-ui/src/sidebar/`, `add_host/`, `hosts/` |
| Lignes locale + WSL + hôtes | ✅ | `terminus-core::{machine,wsl}`, ids `local` / `wsl:<nom>` / uuid |
| Auth mot de passe / clé / clés gérées / GSSAPI | ✅ | `terminus-core/src/{auth_method,managed_key,gssapi}.rs` |
| Largeur de sidebar redimensionnable | ⚠️ R8 (état seul) | livré et testé dans `terminus-ui` : `shell/layout/sidebar_width.rs` (`SidebarWidth` clamp 200..480, `begin_drag`/`drag_to`/`end_drag`, `handle_rect`/`hit_handle`, `parse_setting`/`format_setting`, `SETTING_KEY="sidebar_width"`), `ShellLayout::with_sidebar_width`, `grid_insets_for` ; **non câblé** : `SIDEBAR_WIDTH = 260.0` reste la constante de `ShellLayout::new`, `shell/sidebar.rs`, `sidebar::WIDTH`, `chrome/state.rs` (`reserved_width`), `min_window_size` ; côté `rioterm` rien (lecture du setting au démarrage, souris/curseur, peinture de la poignée, `reapply_chrome_inset`, `set_setting` au relâchement) ; non vérifié en GUI |
| Liste d'hôtes virtualisée | ❌ | aucune logique de fenêtre visible dans `terminus-ui/src/sidebar/` |
| Host Inspector | ❌ | aucun symbole |
| Onboarding (CTA « Add first host ») | ⚠️ R9 | bouton sous le hint « No saved hosts yet » (liste vide, sans filtre) : `sidebar/layout.rs::empty_cta_rect`, `PanelHit::AddFirstHost` (`sidebar/hit.rs`) → `ChromeAction::AddHost` (`chrome/route_press.rs`), peinture `renderer/chrome/notices.rs::render_empty_cta`, test `selection_tests::an_empty_list_offers_an_add_first_host_cta` ; **non vérifié en GUI** ; pas de scaffold en 3 étapes (hors périmètre de R9) ; le survol partage `add_hover` avec le bouton d'en-tête |
| Fenêtre Linux sans cadre | ⚠️ | `screen/frameless.rs` présent ; rendu non vérifié sous GNOME / X11 |

### 3.2 Sessions et onglets

| Capacité | Statut | Preuve / limite |
| :--- | :--- | :--- |
| Shell SSH interactif (OpenSSH en PTY local) | ✅ MVP | keepalive, GSSAPI, `TERM=xterm-256color` |
| Carte « Connection lost » / « Session killed » + Reconnect / Close | ✅ MVP | `screen/sessions/lost.rs`, par panneau ; Reconnect en split = nouvel onglet ; non vérifié en GUI |
| Reprise des onglets au lancement | ✅ MVP | `screen/saved_tabs.rs::restore_saved_tabs_if_due`, **désactivée par défaut** (réglable dans Settings > General) ; ne restaure ni layouts ni panneaux SFTP |
| Auto-reconnexion WSL | ❌ | la carte Reconnect ne vise que les hôtes SSH stockés |
| Shell unifié russh (`SshTransport`) | 📎 D1 | voir §2 |
| Vue active par hôte, Ctrl+Tab limité à l'hôte | ⚠️ R1a | `Workspace::{leave,enter,follow,forget}_host` (`terminus-ui/src/shell/workspace.rs`), `context/host_tabs.rs` (cyclage, `SelectTab`/`SelectLastTab` par hôte), point de synchro `sync_sidebar_selection` ; tests unitaires verts ; câblage `Screen` non testé (pas de harnais) et **non vérifié en GUI** ; actions `ShowView`, `SelectNextHost`/`SelectPrevHost` parsées (`bindings/mod.rs`, commit `8f9fcfb0`) ; helper de cyclage d'hôte `ContextManager::neighbour_host_tab` (`context/host_tabs.rs`) + `HostPanel::host_ids` (`terminus-ui/src/sidebar/state.rs`), tests unitaires verts ; câblé dans `screen/keys.rs` → `screen/sessions/hosts_nav.rs::select_adjacent_host` et `Screen::show_view` (couvert par compilation/clippy seulement, pas de harnais), défauts `Ctrl+Shift+Up/Down` (hôte précédent/suivant) et `Ctrl+Alt+1..5` (`ShowView`) dans `bindings/host_nav.rs` (test `host_cycling_and_view_shortcuts_are_bound_by_default`) ; entrées de palette « Next Host » / « Previous Host » (`renderer/command_palette/action.rs`, `screen/palette.rs`, test `the_palette_offers_next_and_previous_host`) ; R1b complète côté code, tout reste **non vérifié en GUI** ; Shift+Tab reste dans l'hôte ; remappage dans Settings = R1c |

### 3.3 SFTP

| Capacité | Statut | Preuve / limite |
| :--- | :--- | :--- |
| Double panneau Local\|Remote et Host\|Host | ✅ | `sftp_pane/`, `sftp_ui/`, `PaneKind::Sftp` |
| Opérations (list, mkdir, remove récursif, rename, transfert, édition distante) | ✅ | `sftp_worker/commands.rs`, `edit.rs` |
| Dossiers : récursif, sync différentielle, remote→remote, conflits, annulation | ✅ | `folder.rs`, `differential_*.rs`, `conflict.rs`, `remote_archive.rs` |
| File de transferts | ⚠️ | `sftp_worker/queue.rs` : file mpsc, jobs exécutés **un par un** ; pas de vue de file dans l'UI |
| Retry | ⚠️ | `sftp_worker/retry.rs` : rejoue seulement les opérations idempotentes (`Operation::is_idempotent`) ; appliqué aux transferts, archives, suppressions récursives |
| Breadcrumbs cliquables par segment | ⚠️ | `paths.rs::crumb_segments_for_cwd` calcule les segments ; comportement du clic non revérifié (l'ancien jalon disait « un niveau up ») |
| Files s'ouvre dans le dossier courant du terminal courant | ⚠️ | le répertoire OSC 7 (`tab_reported_dir`) n'est lu qu'à la **création** de la session SFTP, et pris sur l'onglet **le plus récent** de l'hôte (`screen/sftp.rs::machine_tab_dir`), pas sur l'onglet courant ; un navigateur déjà ouvert garde son dossier ; clic sur l'onglet « Files » du header non audité |
| Fil d'Ariane éditable au clavier (saisir un chemin) | ❌ | aucun symbole de saisie de chemin ; seuls les segments cliquables existent (`paths.rs::crumb_segments_for_cwd`) |
| Upload par glisser-déposer vers un onglet SSH, dossier configurable | ✅ | `screen/file_drop.rs`, `remote_upload*.rs`, `[uploads]`, Settings > Uploads ; test sshd réel présent |

### 3.4 Tunnels

| Capacité | Statut | Preuve / limite |
| :--- | :--- | :--- |
| CRUD, start/stop, états Stopped / Starting / Running / Failed, badge | ✅ MVP | `tunnel_worker/`, `screen/tunnels.rs` |
| Persistance et supervision | ✅ | `persistence.rs`, `registry.rs`, `ticker.rs` |
| Uptime + connexions | ✅ (Linux) | `tunnel_worker/stats.rs`, échantillon `/proc/net/tcp` |
| Compteur d'octets | ⚠️ R4 (estimation) | `tunnel_worker/stats.rs::{parse_proc_io_bytes,sample_bytes}` lit `/proc/<pid>/io` du process `ssh -N` (octets ≈ (rchar+wchar)/2, inclut handshake/keepalives ; faussé par ProxyCommand/ControlMaster), `controller.rs::refresh_stats` → `TunnelStats.bytes`, affiché `~1.5 MB` (`views/tunnels/model.rs::summary`) ; Linux seulement ; tests `stats_tests::*`, `controller_tests::a_running_tunnel_carries_uptime_and_a_connection_sample`, `model_tests::stats_summary_shows_approximate_traffic_when_known` ; **non vérifié en GUI** ; piste pour un compteur exact : `sock_diag`/netlink ; la ligne de détail s'allonge (clip sur fenêtre étroite non traité) |
| Stats des tunnels d'une autre machine | ❌ | selon l'ancien suivi, non revérifié |

### 3.5 Vault, sync, réglages

| Capacité | Statut | Preuve / limite |
| :--- | :--- | :--- |
| Vault Argon2id, déverrouillage en overlay, worker dédié | ✅ | `vault.rs`, `vault_unlock.rs`, `vault_remember.rs` |
| Fondu du modal vault | ❌ | `anim.rs` existe, non utilisé par les dialogues |
| Réglages : General, Keys, Sync, Appearance, Updates, Uploads | ✅ | `SettingsPage` (`shell/workspace.rs:13`) |
| Sync SQL : statut, dernier sync | ⚠️ R5 | `SyncReport.conflicts` n'est **jamais incrémenté** par le moteur (`sync/report.rs:14` déclaration, `:46` somme ; `push_all`/`pull_all` ne détectent aucun conflit) : le compteur de conflits reste à 0 tant que le moteur ne les calcule pas |
| Badge de sync (dernier sync, conflits) | ⚠️ R5 | `terminus-ui/src/sync_badge.rs::SyncBadge::{label,tone}` (masqué si non connecté, « Synced just now / Nm / Nh / Nd ago », « N conflicts » prioritaire en ton Warn ; 6 tests), `SyncEngine::last_conflicts` (`sync/engine.rs`), `HostEvent::SyncStatus(SyncStamp)` (`hosts/sync.rs::sync_stamp`), `Shell.sync`, libellé peint sur le bouton Settings de la sidebar (`renderer/shell/sidebar.rs`) ; **non vérifié en GUI** ; manque : badge « pending », âge relatif recalculé sur timer (seulement au prochain redraw), détection de conflits côté moteur |
| Revue des conflits | ❌ | aucun écran |

### 3.6 Palette, snippets, historique

| Capacité | Statut | Preuve / limite |
| :--- | :--- | :--- |
| Palette : Open Host…, Open SFTP, vues, mises à jour | ✅ | `renderer/command_palette/` |
| Syntaxe `>sftp [hôte]` et `>forward [list\|start\|stop]` | ✅ MVP | `terminus-ui/src/palette_query.rs` ; limitée à la machine à l'écran ; rendu non vérifié en GUI |
| Snippets | ✅ | `snippets.rs`, `add_snippet.rs` |
| Historique shell (hooks locaux, vue, effacement) | ✅ | `history_worker.rs`, `shell_integration.rs` |
| Raccourcis hôtes / vues, remappage dans Settings | ❌ | même spec que §3.2 ; `SelectTab(n)` n'a de défaut que sur macOS (Cmd) |

### 3.7 Thèmes et accessibilité

| Capacité | Statut | Preuve / limite |
| :--- | :--- | :--- |
| Thèmes chrome sombre / clair + mode Système | ✅ | `theme.rs` : `ThemeMode {Dark, Light}`, `violet_ink`, `violet_paper` |
| Presets supplémentaires | ❌ | 2 thèmes seulement |
| Fonction de ratio de contraste WCAG | ⚠️ | `theme.rs:439` ; aucun preset haut contraste, aucune option reduced-motion |
| Audit clavier, focus rings, doc des raccourcis | ❌ | non fait |

### 3.8 Qualité, performance, livraison

| Capacité | Statut | Preuve / limite |
| :--- | :--- | :--- |
| Tests unitaires et d'intégration | ✅ | ≈ 1 400 attributs `#[test]` : `terminus-ui` 680, `rioterm` 445, `terminus-core` 145, `terminus-bridge` 98, `terminus-update` 28, `terminus-walk` 3 (comptés par grep, non exécutés) |
| CI sur PR | ⚠️ | `.github/workflows/ci.yml` : **rustfmt + `check-file-size.sh` seulement**. Build, tests et clippy tournent en local ; aucun hook git installé dans `.git/hooks` |
| Budget de taille de fichier | ✅ | `scripts/check-file-size.sh` (500 lignes dures, liste d'exceptions décroissante) |
| Benchmarks / test de charge 100+ hôtes | ❌ | aucun `[[bench]]`, aucun répertoire `benches` dans `crates/` |
| Release Linux + Windows | ✅ (manuel) | `scripts/release.sh`, `nix run .#release` ; tarball, deb, rpm, NSIS, MSI, zip, `checksums.txt` |
| Release macOS | ❌ | aucun cas macOS dans `release.sh` |
| Workflow GitHub de release | ❌ | `.github/workflows/` = `ci.yml` + `flakehub.yml` |
| Auto-update vérifié (SHA-256) | ✅ | `terminus-update`, `updater.rs` ; releases **non signées** |

---

## 4. Dette et risques

| # | Élément | Impact | Mitigation |
| :--- | :--- | :--- | :--- |
| D1 | Deux piles SSH (CLI pour shell + tunnels, russh pour SFTP) | host-key et auth divergents | Option A acceptée ; unifier = brancher `SshTransport` + `SessionSpec::Ssh` + modal TOFU |
| D3 | CI sans build, tests ni clippy | une régression passe si le contrôle local est sauté | ajouter les jobs, ou installer un hook versionné |
| D4 | File SFTP sérielle sans vue | gros transferts opaques | vue de file dans l'UI |
| D5 | Releases non signées | l'intégrité repose sur `checksums.txt` et GitHub | signature à décider |
| D6 | Liste d'hôtes non virtualisée | lag possible à 100+ hôtes | virtualiser, puis benchmark |
| D7 | Fenêtre sans cadre non vérifiée sous GNOME / X11 | régression visuelle | validation à l'écran |
| D8 | Interop WSL cassée dans un sandbox FHS | distros listées mais non ouvrables | documenté dans `DEVELOPMENT.md` |

---

## 5. Feuille de route

Ordre de priorité. Chaque jalon a un critère de fin vérifiable.

### P0 — Confiance « daily driver »

| # | Jalon | Critère de fin |
| :--- | :--- | :--- |
| R1 | **Vue et sessions par hôte + raccourcis** (spec relue 2026-10-16 ; R1a implémenté ; R1b faite côté code (non vérifiée en GUI) ; R1c à faire) | tests `terminus-ui` pour `per_host` et cyclage; Ctrl+Tab ne traverse plus les hôtes (vérifié à l'écran) |
| R2 | **CI : build + tests + clippy** (D3) | job vert sur PR, ou hook pre-commit versionné, documenté dans CLAUDE.md |
| R3 | **Vue de file de transferts SFTP** (D4) | plusieurs transferts visibles, annulables, sans bloquer l'UI |

### P1 — Observabilité et dette

| # | Jalon | Critère de fin |
| :--- | :--- | :--- |
| R4 | Compteur d'octets des tunnels (estimation `/proc/<pid>/io` livrée, non vérifiée en GUI ; exact = sock_diag, ouvert) | octets affichés sur la carte, test sur l'échantillonneur |
| R5 | Sync visible : badge, dernier sync, conflits (`SyncReport.conflicts`) — **partiel** : badge + dernier sync livrés (`7b679ded`, non vérifié en GUI) ; conflits jamais calculés par le moteur, pas de revue | état de sync lisible hors Settings |
| R6 | D2 : trancher le bridge — ✅ code supprimé (`dcb85303`) : aucune référence à `TerminusBridge`/`TerminusCoreService`/`terminus_bridge_impl` hors ce fichier ; clippy et tests `terminus-bridge` verts ; `cargo check --workspace --all-targets` exit 0 | trait branché ou code supprimé; plus de commentaire mort dans `lib.rs` |
| R7 | Breadcrumbs : clic sur un segment | navigation vers le segment, test sur `sftp_pane` |
| R8 | Redimensionnement de la sidebar, largeur persistée — **partiel** : état/géométrie/setting dans `terminus-ui` (`a705e82c`), câblage `rioterm` et constantes `SIDEBAR_WIDTH` restants | largeur utilisateur conservée après redémarrage |
| R9 | Onboarding : CTA « Add first host » (fait côté code, non vérifié en GUI) | premier hôte ajoutable depuis l'état vide |
| R16 | Files suit le dossier du terminal courant | clic sur « Files » (header, palette, `ShowView`) navigue vers le dossier OSC 7 de l'onglet courant ; test sur le choix du dossier ; repli documenté sans OSC 7 |
| R17 | Fil d'Ariane éditable (saisie de chemin, Entrée valide, Échap annule) | chemin saisi atteint, erreur affichée si invalide ; test sur `sftp_pane` ; lié à R7 |

### P2 — Échelle et GA

| # | Jalon | Critère de fin |
| :--- | :--- | :--- |
| R10 | Virtualisation de la liste + benchmarks 100+ hôtes (D6) | seuils mesurés dans un script ou la CI |
| R11 | Thèmes supplémentaires, haut contraste, reduced-motion | presets conformes WCAG AA via le ratio de `theme.rs` |
| R12 | Audit clavier + doc des raccourcis | parcours sans souris documenté |
| R13 | Workflow GitHub de release, macOS inclus | artefacts publiés par la CI |
| R14 | D1 : shell russh unifié | un seul stack SSH, TOFU en UI |
| R15 | Host Inspector, auto-reconnexion WSL, reprise fidèle des layouts | à cadrer avant exécution |

---

## 6. Hors périmètre

- Remplacer Rio comme terminal générique (le code amont est hérité tel quel).
- WASM / `librio` comme surface Terminus.
- Parité complète avec Termius / SecureCRT avant R13.
