# TERMINUS — Milestone Plan
> Fork de Rio (`raphamorim/rio`) | Architecture: Rio (GPU Shell) + terminus-core (Domain Backend)

---

## 🧩 Architecture d'intégration

### Principe fondamental
**`terminus-core`** est un crate Tokio pur, sans dépendance wgpu/winit. Il communique avec le frontend Rio via une facade `TerminusBridge`. Le PTY distant SSH est injecté dans le moteur Rio en implémentant le trait `EventedPty: ProcessReadWrite` (de `teletypewriter`), ce qui permet de réutiliser le pipeline complet `Machine → Crosswords → Sugarloaf` sans toucher au renderer.

### Structure du workspace

```toml
[workspace]
members = [
    # Cœurs existants de Rio
    "sugarloaf", "teletypewriter", "corcovado", "rio-graphics", "rio-fonts",
    "rio-vt", "rio-backend", "rio-grapheme-width", "rio-unicode", "rio-window",
    "rio-notifier", "rio-grid", "frontends/rioterm", "librio", "librio-wasm", "libsugarloaf",
    # Nouveaux crates Terminus
    "crates/terminus-core",   # Store, SSH, Vault, Sync, Port Forwarding, SFTP, OS Detect
    "crates/terminus-bridge", # Facade async bridge → RioEvent (Singleton Arc)
    "crates/terminus-ui",     # Renderers: Sidebar, ActivityBar, SFTP pane
]
```

### Points d'intégration critiques

| Fichier Rio | Modification | Composant Terminus |
| :--- | :--- | :--- |
| `frontends/rioterm/src/context/mod.rs` | `ContextManagerConfig` gagne un champ `session: SessionSpec { Local(Shell), Ssh(HostTarget) }` | `terminus-core::ssh::connect` → `SshTransport` → même `Machine::new \|> spawn` |
| `teletypewriter/src/lib.rs` | Nouveau `SshTransport` impl `EventedPty` (OS pipes ↔ russh channel) | `terminus-bridge::ssh_transport` |
| `frontends/rioterm/src/layout/mod.rs` | Layout racine Taffy → flex-row 3 feuilles : ActivityBar(48px) + Sidebar(260px) + Content(flex) | `terminus-ui::layout_nodes` |
| `frontends/rioterm/src/renderer/command_palette.rs` | `PaletteMode` étendu : `Hosts`, `Snippets`, `History`, `Groups` | `terminus-bridge::list_hosts`, `list_snippets` |
| `frontends/rioterm/src/renderer/mod.rs` | Nouveaux modules : `activity_bar.rs`, `sidebar.rs`, `sftp_pane.rs` | `terminus-ui::renderers` |
| `frontends/rioterm/src/router/mod.rs` | `Modal` étendu : `TofuHostKeyApproval`, `VaultUnlock`, `HostEditor`, `SftpPane` | `terminus-bridge::modal_events` |
| `frontends/rioterm/src/application.rs` | `Application<T>` gagne `TerminusCoreService` Arc (init store + vault + sync + forward_runtime) | `terminus-core::Store`, `Vault`, `SyncEngine` |
| `rio-backend/src/config/theme` | Tokens `TerminusUiColors` + 7 thèmes intégrés | Palette thèmes |

---

## 📈 État d'avancement

### Fait

| # | Jalon | État |
| :--- | :--- | :--- |
| 1.1 | Intégration `terminus-core` | ✅ Store SQLite, migrations, 44 tests |
| 1.2 | `terminus-bridge` | ✅ `SshTransport` `EventedPty` + tests |
| 1.3 | `SshTransport` | ✅ transport + tests corcovado |
| 1.5a | **Sidebar Hosts — liste + ajout d'hôte** | ✅ voir ci-dessous |

**1.5a — première tranche du jalon 1.5** (le reste du jalon reste à faire) :

* `crates/terminus-ui` n'est plus un squelette : il porte l'état, la géométrie
  et le hit-testing du chrome (rail d'activité, panneau d'hôtes, éditeur
  d'hôte), sans dépendance de rendu — 59 tests sans GPU.
* `frontends/rioterm/src/renderer/chrome.rs` peint ce que `terminus-ui` décrit ;
  `frontends/rioterm/src/hosts.rs` fait vivre SQLite derrière un thread dédié,
  le thread UI n'attend jamais.
* Le chrome **réserve sa bande dans la marge de la grille** : le terminal se
  reflow à côté au lieu d'être recouvert, et `reapply_chrome_inset` réagit à un
  hot-reload de config.
* Ajout d'hôte de bout en bout : clic sur « + Add host », formulaire (4 champs,
  Tab/Shift+Tab, flèches, Home/End, suppression), Entrée enregistre — la ligne
  apparaît dans la liste, persistée dans `terminus.db`. Échap annule.
* Icônes = géométrie réelle des sources Lucide, décomposée hors ligne par
  `scripts/gen-lucide-icons.py` (sugarloaf n'a pas de renderer SVG).

Reste pour clore 1.5 : arborescence Groups/Hôtes, pastilles d'état
(`connected`/`connecting`/`error`), badges OS, redimensionnement de la sidebar,
et le câblage du clic sur un hôte vers `SessionSpec::Ssh`.

---

## 🎯 Phase 1 — Fondations & Shell MVP

**Objectif** : Prouver la valeur ajoutée sur Rio — hôtes vivants, vault, opérations keyboard-first.

| # | Jalon | Détails | Livrables |
| :--- | :--- | :--- | :--- |
| 1.1 | **Intégrer `terminus-core` dans le workspace** | Ajouter le crate, configurer sqlx SQLite (`~/.local/share/terminus/terminus.db`), migrations automatiques | `cargo check -p terminus-core` ✓ |
| 1.2 | **Créer `terminus-bridge` (Singleton Arc)** | `TerminusCoreService` initialisé dans `application.rs` au démarrage. Expose : `list_hosts`, `connect`, `unlock_vault`, `start_forward`, `list_snippets`, `get_sync_status` | `TerminusBridge` trait + impl |
| 1.3 | **Implémenter `SshTransport` (EventedPty)** | Paires de pipes OS connectées aux canaux russh. `register()` via corcovado Poll. `set_winsize` → `channel.window_change`. `child_event_token` → session close | `SshTransport` impl `EventedPty` + tests |
| 1.4 | **Modifier `Context::create_context`** | Brancher `SessionSpec::Ssh` : appeler `ssh::connect`, approuver TOFU via Modal oneshot, injecter `SshTransport` dans `Machine::new` | Sessions SSH interactives dans les onglets |
| 1.5 | **Sidebar Hosts tree** | Taffy flex-row racine → ActivityBar(48px) + Sidebar(260px collapsible) + Content. Renderer `sidebar.rs` : arborescence Groups/Hôtes, pastilles état (`connected`/`connecting`/`error`), OS badges | Sidebar fonctionnelle avec états live |
| 1.6 | **Command Palette étendue** | `PaletteMode::Hosts` fuzzy search sur hostname/username/tags. `PaletteAction::OpenSshSession(HostTarget)` | Recherche + connexion d'hôtes depuis `Ctrl+P` |
| 1.7 | **Vault Unlock overlay** | Modal non-bloquant, 120ms fade-in. Lock glyph sur Sidebar. Argon2id → DEK → déchiffrement | Vault déverrouillable depuis l'UI |
| 1.8 | **Session management v1** | Détection déconnexion, `Msg::Shutdown` → fermeture onglet propre, rappel de sessions via Store | Onglets renommables, reconnexion |

---

## 🎯 Phase 2 — Ops Power (Forwards + SFTP)
**Objectif** : Le hub d'opérations distantes en une seule fenêtre.

| # | Jalon | Détails | Livrables |
| :--- | :--- | :--- | :--- |
| 2.1 | **Port Forwarding live** | `ForwardRuntime` dans un thread tokio. Panel Sidebar : liste des tunnels actifs, toggle start/stop, stats débit up/down | Tunnels démarrables/arrêtables depuis la sidebar |
| 2.2 | **SFTP Dual-Pane** | `ContextGridItem::PaneKind::Sftp`. Volet gauche = LocalFS, volet droit = russh-sftp. Breadcrumb paths, barre de progression transferts, drag-drop | Navigateur fichiers double-volet |
| 2.3 | **Transferts async** | Queue de transferts avec retry, calcul taille restante, animation progression WGPU | Transferts en arrière-plan sans bloquer le terminal |
| 2.4 | **Palette pour Forwards/SFTP** | Actions `>forward 3000:localhost:3000`, `>sftp /remote/path` | Exécution rapide depuis le palette |
| 2.5 | **Empty states & drop-targets** | Sidebar vide → scaffold 3 étapes "Add first host". SFTP pane vide → drop target | Onboarding `< 60 secondes` |

---

## 🎯 Phase 3 — Scale, Sync & Bulk
**Objectif** : Opérations multi-hôtes, config convergente entre appareils.

| # | Jalon | Détails | Livrables |
| :--- | :--- | :--- | :--- |
| 3.1 | **Groupe "Open All"** | Clic droit Groupe → connexions parallèles, onglets couleurs groupées, kill-group | Bulk connect pour flottes |
| 3.2 | **Sync-aware UI** | Badge "last synced", compteur pending changes, panneau review conflits | Sync visible et gérable |
| 3.3 | **Host Inspector** | Overlay compact : OS/tags/IP, sessions récentes, boutons SFTP/Forward/Sync | Actions contextuelles rapides |
| 3.4 | **Session recall** | Rappel automatique des sessions au redémarrage via Store | Persistence des onglets |
| 3.5 | **WSL reconnect** | Détection WSL, reconnexion automatique dist | Résilience réseau |

---

## 🎯 Phase 4 — Polish & Accessibility (GA)
**Objectif** : Dureté, thèmes, accessibilité complète.

| # | Jalon | Détails | Livrables |
| :--- | :--- | :--- | :--- |
| 4.1 | **Thèmes Terminus** | 7 built-in (Graphite, Mocha, Terminus, Obsidian, Phosphor, Midnight, Paper). Tokens `TerminusUiColors`. Hot-reload via notify watcher | Thématisation complète |
| 4.2 | **WCAG AA contrast** | Presets contraste élevé, reduced-motion honoring | Accessibilité |
| 4.3 | **Keyboard-only audit** | Focus rings, navigation complète sans souris, raccourcis documentés | Parité accessibilité Rio |
| 4.4 | **Perf regression pass** | Benchmarks Sidebar + SFTP avec 100+ hôtes, mémoire < 50MB | Performance stable |
| 4.5 | **Release CI/CD** | Installeurs : `.exe` (Windows), Nix flake + `.deb` + `.rpm` + AppImage (Linux), `.dmg` (macOS) | Distribution cross-platform |

---

## 📊 Résumé des priorités

| Priorité | Features | Phase |
| :--- | :--- | :--- |
| **P0** | Sidebar Hosts, Command Palette, Vault, Sessions, Port Forwards, SFTP | Phase 1 & 2 |
| **P1** | Group bulk, Sync UI, Onboarding | Phase 3 |
| **P2** | Inspector, Themes, Accessibility | Phase 4 |

---

## 🔑 Risques & Mitigations

| Risque | Impact | Mitigation |
| :--- | :--- | :--- |
| **corcovado ↔ tokio mismatch** | SSH async ne s'intègre pas au poll loop Rio | `SshTransport` via pipes OS (prouvé dans l'archi) |
| **Vault bloquant** | UI freeze pendant Argon2id (19MiB) | Thread dédié + overlay non-bloquant |
| **SFTP perf** | Latence sur gros transferts | Streaming async 8KiB chunks, queue prioritised |
| **Windows ConPTY + SSH** | Double abstraction PTY | Teletypewriter gère déjà ConPTY nativement |
| **Large host list** | Sidebar lag avec 100+ hôtes | Virtual scroll (item_height 36px, visible slice) |
