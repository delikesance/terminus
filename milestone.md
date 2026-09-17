# TERMINUS — Roadmap

> Fork de Rio (`raphamorim/rio`) · Dernière mise à jour : alignée sur le code de
> `feat/rio-integration` (audit source, pas d’intention).

---

## Architecture réelle (aujourd’hui)

### Principe

- **`terminus-core`** — domaine Tokio pur (Store SQLite, Vault, Sync, SSH/SFTP
  russh, WSL, forwards registry, OS detect).
- **`terminus-bridge`** — workers / transports partagés (`SshTransport`,
  `sftp_worker`). Le trait `TerminusBridge` / `TerminusCoreService` est encore
  un stub ; le runtime hôtes + sync vit dans rioterm (`hosts.rs`).
- **`terminus-ui`** — état / géométrie / hit-test chrome (pas de GPU).
- **`frontends/rioterm`** — paint Sugarloaf + câblage input / sessions.

### Sessions SSH interactives — **décision acceptée (Option A)**

> **Décision (2026-09-16) :** le MVP shell interactif reste **OpenSSH CLI dans un
> PTY local**. On ne branche pas `SshTransport` / `SessionSpec::Ssh` pour GA
> immédiat. SFTP continue sur russh. Dette tracée en **1.4-debt**.

| Chemin | Techno | Usage |
| :--- | :--- | :--- |
| Shell onglet | PTY local + binaire `ssh` (`screen/mod.rs` → `ssh_shell`) | Connexion interactive depuis la sidebar |
| SFTP | russh + `sftp_worker` | Navigateur fichiers dual-pane |
| `SshTransport` | russh ↔ pipes, `EventedPty` | **Prêt, non câblé** — dette 1.4-debt |

Conséquences assumées : host-key / auth peuvent diverger légèrement entre shell
CLI (`StrictHostKeyChecking=accept-new`) et SFTP russh ; pas de TOFU modal UI
pour le shell CLI.

### Structure workspace (crates Terminus)

```toml
"crates/terminus-core",   # Store, SSH lib, Vault, Sync, Forwards, SFTP, WSL, OS
"crates/terminus-bridge", # SshTransport, sftp_worker (facade trait = stub)
"crates/terminus-ui",     # Chrome paint-free : sidebar, settings, SFTP, vault…
```

### Intégrations critiques — statut

| Point d’accroche | Statut | Réalité code |
| :--- | :--- | :--- |
| Chrome inset (ActivityBar + Sidebar) | ✅ | Marge grille + `reapply_chrome_inset` |
| Hosts / groups / DnD / context menu | ✅ | `terminus-ui` + `hosts.rs` worker |
| Vault unlock modal | ✅ | UI + Argon2id via worker hôtes |
| Settings (clés SSH + SqlSync) | ✅ | `settings.rs` + sync dans `hosts.rs` |
| SFTP dual-pane | ✅ | `PaneKind::Sftp`, worker, DnD, Host\|Host |
| Shell SSH interactif | ✅ MVP | OpenSSH CLI en PTY local (décision Option A) |
| `SessionSpec::Ssh` + `SshTransport` | 📎 dette | Crate prêt ; branchement = 1.4-debt |
| Palette `Hosts` | ✅ | `Open Host…` + fuzzy inline dans Commands |
| Port-forward UI | ❌ | `ForwardRuntime` core seul |
| Thèmes `TerminusUiColors` | ❌ | Un seul `ChromeTheme::apple_hig()` |
| `TerminusCoreService` bridge | ❌ | Stub commenté |

---

## Tableau de bord

Légende : ✅ fait · ⚠️ partiel · ❌ pas commencé

| # | Jalon | Statut | Preuve / note |
| :--- | :--- | :--- | :--- |
| **1.1** | `terminus-core` dans le workspace | ✅ | Store SQLite + migrate inline, vault, sync, ssh/sftp, wsl… |
| **1.2** | Bridge runtime | ⚠️ | `sftp_worker` + `SshTransport` OK ; trait / service stub ; workers hôtes dans rioterm |
| **1.3** | `SshTransport` EventedPty | ✅ lib | Impl + tests bridge ; non câblé UI (= 1.4-debt) |
| **1.4** | Sessions SSH dans les onglets | ✅ MVP | OpenSSH CLI via `ssh_shell` ; TOFU = `accept-new` OpenSSH |
| **1.4-debt** | Shell russh unifié | 📎 | Brancher `SshTransport` + `SessionSpec::Ssh` + modal TOFU |
| **1.5** | Sidebar Hosts | ✅ | Groupes, DnD, rename, OS badges, connecting shimmer, context menu ; **pas** de resize sidebar |
| **1.6** | Palette Hosts | ✅ | `ListHosts` / mode Hosts + match inline dans Commands |
| **1.7** | Vault Unlock | ✅ | Overlay + déverrouillage ; fade 120 ms non garanti |
| **1.8** | Session management v1 | ⚠️ | Onglets renommables + modal connexion ; pas de recall Store / cleanup disconnect formalisé |
| **2.1** | Port forwarding live | ❌ | Registry core seulement ; pas d’UI ni tunnels `direct-tcpip` |
| **2.2** | SFTP dual-pane | ✅ | Local\|Remote ou Host\|Host, menu contextuel, Edit remote (temp+default app+reupload), DnD, Close |
| **2.3** | Transferts async | ⚠️ | `Transfer` + progress events ; pas de queue/retry UI ni barre WGPU riche ; **pas** de dossiers |
| **2.4** | Palette Forwards / SFTP | ❌ | — |
| **2.5** | Empty states / onboarding | ⚠️ | CTA new-host + menus SFTP vides ; pas de scaffold 3 étapes |
| **3.1** | Groupe « Open All » | ❌ | Menu groupe = rename / delete |
| **3.2** | Sync-aware UI | ⚠️ | Settings SqlSync + glyph rail ; pas de badge last-synced / conflits |
| **3.3** | Host Inspector | ❌ | — |
| **3.4** | Session recall | ❌ | API core `SessionManager` non branchée UI |
| **3.5** | WSL reconnect | ⚠️ | Discover / launch OK ; pas d’auto-reconnect |
| **4.1** | Thèmes Terminus (×7) | ❌ | Thème chrome unique |
| **4.2** | WCAG AA / reduced-motion | ❌ | — |
| **4.3** | Keyboard-only audit | ⚠️ | Forms / settings / SFTP navigables ; pas d’audit documenté |
| **4.4** | Perf regression | ❌ | Pas de benches 100+ hôtes |
| **4.5** | Release Terminus | ⚠️ | CI/packaging Rio hérités ; pas d’installers « Terminus » |

**Lecture :** Phase 1 ≈ MVP chrome + vault + sessions CLI. Phase 2 ≈ SFTP utilisable, forwards absents. Phases 3–4 = scale / GA.

---

## Ce qui est livré (détail)

### Chrome & hôtes

- Activity bar + panneau hôtes (largeur fixe 288), collapse, inset terminal.
- CRUD hôtes (formulaire add/edit), groupes, DnD reorder / into-group, rename inline.
- Context menu hôte : Edit, Open SFTP, Open in other pane, Rename, Delete.
- Auth : password / key / managed keys / GSSAPI (selon chemins store + settings).
- Modal de progression de connexion ; pastilles / shimmer connecting.
- Badges OS (`os_icons`).

### Vault & settings

- Unlock / create vault (Argon2id) depuis UI.
- Settings : Managed SSH Keys (generate / import PEM) + Remote SQL Sync.

### SFTP

- Pane dédié (`PaneKind::Sftp`) : Left/Right avec backend Local ou Remote.
- Worker async (`List*` / `Mkdir*` / `Remove*` / `Rename*` / `Transfer` / `EditRemote`).
- UX : clic droit (Open, **Edit** sur fichier remote, Upload/Download/Copy, Rename,
  New folder, Delete, Refresh), DnD fichier pane↔pane, Host\|Host via
  « Open in other pane ».
- **Edit remote** : télécharge vers `temp/terminus-sftp-edit/…`, ouvre avec l’app
  par défaut (`xdg-open` / `open` / `ShellExecute`), poll mtime → réupload.
- Champ rename/mkdir via composant `FieldPaint` partagé.
- Limites connues : pas de transfert de dossiers ; progress UI minimaliste.

### Infra

- Hot-reload / loops de dev documentés (`DEVELOPMENT.md`).
- Tests unitaires denses sur `terminus-ui` (géométrie) et parties core.

---

## Phase 1 — Fondations (clôturer le MVP shell)

**Objectif :** sessions et découverte d’hôtes au niveau « produit quotidien ».

| # | Jalon | Reste à faire | Critère de done |
| :--- | :--- | :--- | :--- |
| 1.2b | **Bridge unifié** | Extraire / formaliser `TerminusCoreService` depuis `hosts.rs` ; exposer list/connect/vault/sync | Un seul runtime Arc ; rioterm n’embed plus le worker ad hoc |
| 1.4-debt | **Shell russh (Option B, plus tard)** | Remplacer `ssh_shell` CLI par `SshTransport` + `SessionSpec::Ssh` + modal TOFU | Un seul stack SSH (shell = SFTP = forwards) |
| 1.5b | **Sidebar resize** | Poignée de resize + persist largeur | Largeur utilisateur persistée |
| 1.5c | **États hôte** | Harmoniser pastilles `connected` / `connecting` / `error` avec sessions live | Trois états visibles et justes |
| 1.8 | **Session lifecycle** | Detect drop, fermeture propre, (optionnel) re-open | Pas d’onglet zombie ; message clair |

---

## Phase 2 — Ops (forwards + SFTP mature)

**Objectif :** hub fichiers / tunnels sans quitter la fenêtre.

| # | Jalon | Reste à faire | Critère de done |
| :--- | :--- | :--- | :--- |
| 2.1 | **Port forwarding** | `direct-tcpip` (ou équiv.) + UI liste start/stop + stats | Tunnel usable depuis sidebar |
| 2.2b | **SFTP polish** | Breadcrumbs cliquables, empty drop-target, feedback erreur | Parité « explorateur » basique |
| 2.3 | **Transferts robustes** | Queue, retry, dossiers (tar ou récursif), barre de progression visible | Gros fichiers + dossiers sans bloquer UI |
| 2.4 | **Palette ops** | `>sftp`, `>forward …` | Actions sans souris |
| 2.5 | **Onboarding** | Empty sidebar 3 étapes « Add first host » | Premier host &lt; 60 s |

---

## Phase 3 — Scale, sync & bulk

| # | Jalon | Détails | Critère de done |
| :--- | :--- | :--- | :--- |
| 3.1 | **Open All / kill group** | Context menu groupe | Flotte ouverte / fermée en un geste |
| 3.2 | **Sync-aware chrome** | Badge last-synced, pending, review conflits | Sync visible hors Settings |
| 3.3 | **Host Inspector** | Overlay compact OS / tags / actions | Actions rapides sans ouvrir Settings |
| 3.4 | **Session recall** | Brancher `SessionManager` au démarrage | Restauration onglets |
| 3.5 | **WSL reconnect** | Auto-relance dist après drop | Distro WSL resilient |

---

## Phase 4 — Polish & GA

| # | Jalon | Détails | Critère de done |
| :--- | :--- | :--- | :--- |
| 4.1 | **Thèmes Terminus** | 7 presets + tokens chrome | Thèmes switchables |
| 4.2 | **A11y contraste** | High-contrast + reduced-motion | Presets WCAG |
| 4.3 | **Keyboard audit** | Doc raccourcis + focus rings | Parcours sans souris documenté |
| 4.4 | **Perf** | Benches 100+ hôtes, mémoire | Seuils dans CI ou script |
| 4.5 | **Release Terminus** | Installers Win / Linux / macOS brandés | Artefacts « Terminus » |

---

## Priorités (recalées)

| Priorité | Focus | Pourquoi |
| :--- | :--- | :--- |
| **P0** | Session lifecycle, SFTP dossiers + progress | Bloque la confiance « daily driver » |
| **P0** | Port forwarding end-to-end | Seul gros trou Phase 2 |
| **P1** | Bridge unifié, sync chrome, Open All, onboarding | Scale + dette archi |
| **P1** | 1.4-debt shell russh unifié | Quand forwards / auth unique deviennent bloquants |
| **P2** | Thèmes, a11y, perf, packaging Terminus | GA |

---

## Risques (à jour)

| Risque | Impact | État mitigation |
| :--- | :--- | :--- |
| Double chemin SSH (CLI shell vs russh SFTP) | Auth / host-key divergents | **Accepté (Option A)** ; unification = 1.4-debt |
| Bridge stub + workers dans rioterm | Dette de structure | 1.2b |
| Forwards core sans I/O | Feature « fantôme » | 2.1 |
| Transferts SFTP sans dossiers / queue | Ops limitées | 2.3 |
| Liste d’hôtes non virtualisée | Lag 100+ | Toujours à faire (4.4 / sidebar) |
| Vault Argon2id | Freeze UI si mal placé | Worker dédié ✅ |

---

## Hors scope actuel (ne pas confondre avec « manquant »)

- Remplacer Rio en tant que terminal générique (héritage upstream OK).
- WASM / librio comme surface Terminus.
- Parité feature-complete Termius/SecureCRT avant Phase 4.
