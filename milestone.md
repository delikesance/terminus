# TERMINUS — Roadmap

> Fork de Rio (`raphamorim/rio`) · Version `0.7.11` · Dernière mise à jour :
> 2026-10-08, audit du code de la branche `development` (pas d’intention).

---

## Architecture réelle (aujourd’hui)

### Principe

- **`terminus-core`** — domaine Tokio pur (Store SQLite, Vault, Sync, SSH/SFTP
  russh, WSL, historique shell, onglets sauvegardés, OS detect). Contient aussi
  `ForwardRuntime` et `SessionManager`, **non utilisés** par le frontend.
- **`terminus-bridge`** — workers / transports partagés (`SshTransport`,
  `sftp_worker`, `folder_diff`, `walk_remote`). Le trait `TerminusBridge` /
  `TerminusCoreService` est toujours un stub (`terminus_bridge_impl.rs` = un
  commentaire) ; le runtime hôtes + sync vit dans rioterm (`hosts.rs`).
- **`terminus-ui`** — état / géométrie / hit-test chrome (pas de GPU).
- **`terminus-walk`** — binaire helper de parcours distant (hash de dossiers),
  embarqué et déposé sur l’hôte pour la synchro différentielle SFTP.
- **`terminus-update`** — auto-update (endpoint, clé publique, noms d’assets
  Linux / Windows).
- **`frontends/rioterm`** — paint Sugarloaf + câblage input / sessions.

### Chemins SSH — **décision acceptée (Option A)**

> **Décision (2026-09-16) :** le MVP shell interactif reste **OpenSSH CLI dans un
> PTY local**. On ne branche pas `SshTransport` / `SessionSpec::Ssh` pour GA
> immédiat. SFTP continue sur russh. Dette tracée en **1.4-debt**.

| Chemin | Techno | Usage |
| :--- | :--- | :--- |
| Shell onglet | PTY local + binaire `ssh` (`screen/shell.rs` → `ssh_shell`) | Connexion interactive depuis la sidebar |
| Port-forward | Processus `ssh -N -L/-R/-D` (`tunnel_worker.rs`), mêmes args de base que le shell (`tunnel_command` → `shell_for_row`) | Onglet Tunnels |
| SFTP | russh + `sftp_worker` | Navigateur fichiers dual-pane |
| `SshTransport` | russh ↔ pipes, `EventedPty` | **Prêt, non câblé** — dette 1.4-debt |

Conséquences assumées : deux piles SSH coexistent pour trois chemins (OpenSSH
CLI pour shell et tunnels, partageant la même construction d’args ; russh pour
SFTP). Host-key / auth peuvent diverger légèrement entre CLI et russh
(`StrictHostKeyChecking=accept-new` côté CLI) ; pas de TOFU modal UI côté CLI.
`ForwardRuntime` (core, tokio) n’est pas utilisé : les tunnels passent par des
processus `ssh`.

### Structure workspace (crates Terminus)

```toml
"crates/terminus-core",   # Store, SSH lib, Vault, Sync, SFTP, WSL, historique, onglets
"crates/terminus-bridge", # SshTransport, sftp_worker, folder_diff (facade trait = stub)
"crates/terminus-ui",     # Chrome paint-free : sidebar, settings, SFTP, vault, tunnels…
"crates/terminus-walk",   # Helper distant de hash de dossiers
"crates/terminus-update", # Auto-update
```

### Intégrations critiques — statut

| Point d’accroche | Statut | Réalité code |
| :--- | :--- | :--- |
| Chrome inset (ActivityBar + Sidebar) | ✅ | Marge grille + `reapply_chrome_inset` |
| Hosts / groups / DnD / context menu | ✅ | `terminus-ui` + `hosts.rs` worker |
| Vault unlock modal | ✅ | UI + Argon2id via worker hôtes (pas de fade) |
| Settings (clés SSH + SqlSync + Apparence) | ✅ | `views/settings/*` + sync dans `hosts.rs` |
| SFTP dual-pane | ✅ | `PaneKind::Sftp`, worker, DnD, Host\|Host, dossiers |
| Shell SSH interactif | ✅ MVP | OpenSSH CLI en PTY local (décision Option A) |
| Port-forward UI | ⚠️ MVP | Onglet Tunnels (CRUD + start/stop) via `ssh -L/-R/-D` ; pas de stats |
| `SessionSpec::Ssh` + `SshTransport` | 📎 dette | Crate prêt ; branchement = 1.4-debt |
| Palette | ⚠️ | Open Host…, Open SFTP, Show Files/Tunnels/Snippets/History ; pas de syntaxe `>` |
| Reprise d’onglets | ✅ MVP | `saved_tabs` (fichier), pas `SessionManager` |
| Thèmes `ChromeTheme` | ⚠️ | `violet_ink` (sombre) + `violet_paper` (clair), Réglages > Apparence : Sombre / Clair / Système |
| `TerminusCoreService` bridge | ❌ | Stub commenté |

---

## Tableau de bord

Légende : ✅ fait · ⚠️ partiel · ❌ pas commencé · 📎 dette assumée

| # | Jalon | Statut | Preuve / note |
| :--- | :--- | :--- | :--- |
| **1.1** | `terminus-core` dans le workspace | ✅ | Store SQLite + migrate inline, vault, sync, ssh/sftp, wsl… |
| **1.2** | Bridge runtime | ⚠️ | `sftp_worker` + `SshTransport` OK ; trait / service stub ; workers hôtes dans rioterm |
| **1.3** | `SshTransport` EventedPty | ✅ lib | Impl + tests bridge ; non câblé UI (= 1.4-debt) |
| **1.4** | Sessions SSH dans les onglets | ✅ MVP | OpenSSH CLI via `ssh_shell` ; keepalive + GSSAPI ; TOFU = `accept-new` |
| **1.4-debt** | Shell russh unifié | 📎 | Brancher `SshTransport` + `SessionSpec::Ssh` (`#[allow(dead_code)]`) + modal TOFU |
| **1.5** | Sidebar Hosts | ✅ | Groupes, DnD, rename, OS badges, connecting shimmer, context menu ; largeur fixe 260 |
| **1.6** | Palette Hosts | ✅ | `ListHosts` « Open Host… » + match inline dans Commands |
| **1.7** | Vault Unlock | ✅ | Overlay + déverrouillage ; **aucune** animation de fade trouvée (ni dans `vault_unlock.rs`, ni dans `renderer/dialogs/`) |
| **1.8** | Session lifecycle | ⚠️ | Carte « Connection lost » + Reconnect (SSH exit 255, hôte stocké, mono-panneau) ; onglets renommables ; réouverture des onglets au lancement ; autres sorties = onglet fermé |
| **2.1** | Port forwarding live | ⚠️ MVP | Onglet Tunnels : CRUD, start/stop, états Stopped/Starting/Running/Failed, badge, erreurs lisibles, arrêt à la suppression d’hôte ; via `ssh -N -L/-R/-D` ; **pas** de stats ni de palette |
| **2.2** | SFTP dual-pane | ✅ | Local\|Remote ou Host\|Host, menu contextuel, Edit remote (temp+default app+reupload), DnD, Close |
| **2.2b** | SFTP polish | ⚠️ | Drop-target vide, « This folder is empty », erreur en pied de pane OK ; breadcrumbs : un clic remonte d’un niveau, pas de saut vers un segment |
| **2.3** | Transferts async | ⚠️ | Dossiers (`TransferFolder`, récursif, sync différentielle, remote→remote), conflits (`ResolveConflict`), annulation, barre de progression ; worker **série** : pas de queue, pas de retry |
| **2.4** | Palette Forwards / SFTP | ⚠️ | « Open SFTP » (+ choix d’hôte), « Show Tunnels » ; pas de `>sftp` / `>forward`, pas d’action start/stop tunnel |
| **2.5** | Empty states / onboarding | ⚠️ | Hint sidebar vide + formulaire Add server en 3 étapes ; pas de scaffold d’onboarding ni CTA « Add first host » |
| **3.1** | Groupe « Open All » | ❌ | Menu groupe = Rename / Delete group |
| **3.2** | Sync-aware UI | ⚠️ | Settings SqlSync (statut, last-synced) + mot « Synced » sur le bouton Settings ; `SyncReport.conflicts` calculé mais non affiché ; pas de badge pending / conflits |
| **3.3** | Host Inspector | ❌ | — |
| **3.4** | Session recall | ✅ MVP | `saved_tabs` : réouverture des onglets et de leurs noms au lancement (`restore_saved_tabs_if_due`) ; `SessionManager` core toujours non branché |
| **3.5** | WSL reconnect | ⚠️ | Discover / launch OK ; carte Reconnect réservée aux hôtes SSH stockés ; pas d’auto-reconnect WSL |
| **4.1** | Thèmes Terminus (×7) | ⚠️ | 2 thèmes chrome (sombre/clair) + mode Système ; 7 presets à faire |
| **4.2** | WCAG AA / reduced-motion | ❌ | Fonction de ratio de contraste dans `theme.rs`, aucun preset ni option reduced-motion |
| **4.3** | Keyboard-only audit | ⚠️ | Forms / settings / SFTP navigables ; pas d’audit ni de doc raccourcis |
| **4.4** | Perf regression | ❌ | `criterion` déclaré en dev-dep, aucun `[[bench]]`, aucun test 100+ hôtes |
| **4.5** | Release Terminus | ⚠️ | `scripts/release.sh` (manuel) produit tarball Linux, `.deb`/`.rpm` (nfpm), `terminus-setup-x86_64.exe` (NSIS, exe installé `tmnx.exe`), `.msi` (wixl) et zip Windows, tous nommés `terminus-*` ; `terminus-update` ; **rien** pour macOS (le Makefile ne garde que le `.dmg` Rio hérité) ; CI = `ci.yml` (build/test/fmt/clippy) + `flakehub.yml`, **pas** de workflow de release |

**Lecture :** Phase 1 ≈ MVP chrome + vault + sessions CLI + reconnexion SSH. Phase 2 ≈ SFTP mature (dossiers) et tunnels MVP ; reste queue/retry et stats. Phases 3–4 = scale / GA.

---

## Ce qui est livré (détail)

### Chrome & hôtes

- Activity bar + panneau hôtes (largeur fixe 260), collapse, inset terminal.
- CRUD hôtes (formulaire Add server en 3 étapes), groupes (état replié persisté),
  DnD reorder / into-group, rename inline.
- Context menu hôte : Open SFTP, Open in other pane, Edit host, Rename, Delete.
  Context menu groupe : Rename, Delete group.
- Auth : password / key / managed keys / GSSAPI (selon chemins store + settings).
- Modal de progression de connexion (séquence multi-étapes) ; shimmer connecting.
- Badges OS (`os_icons`).
- Sidebar : retour au dernier onglet utilisé d’un hôte.

### Sessions

- Shell SSH via OpenSSH CLI : keepalive (`ServerAliveInterval=15`), GSSAPI,
  `TERM=xterm-256color`, clés temporaires supprimées dès l’auth faite.
- Onglets renommables (double-clic ou menu contextuel) ; onglets et noms rouverts
  au lancement suivant (`saved_tabs`).
- Perte de connexion : l’onglet est conservé avec une carte « Connection lost »
  (Reconnect / Close) — uniquement exit SSH 255 sur hôte stocké mono-panneau.

### Vault & settings

- Unlock / create vault (Argon2id) depuis UI, worker dédié.
- Settings : Managed SSH Keys (generate / import PEM), Remote SQL Sync (statut,
  last-synced), Apparence (Sombre / Clair / Système, suit l’OS en direct).

### SFTP

- Pane dédié (`PaneKind::Sftp`) : Left/Right avec backend Local ou Remote.
- Worker async (`List*` / `Mkdir*` / `Remove*` (dont récursif distant) / `Rename*` /
  `Transfer` / `TransferFolder` / `EditRemote` / `ResolveConflict` / `CancelTransfer`).
- Transfert de dossiers : récursif, sync différentielle (cache + hash distant via
  `terminus-walk`), remote→remote (archive), politique de conflits, annulation.
- UX : clic droit (Open, **Edit** sur fichier remote, Upload/Download/Copy, Rename,
  New folder, Delete, Refresh), DnD fichier pane↔pane, Host\|Host via
  « Open in other pane », barre de progression, breadcrumbs.
- **Edit remote** : télécharge vers `temp/terminus-sftp-edit/…`, ouvre avec l’app
  par défaut (`xdg-open` / `open` / `ShellExecute`), poll mtime → réupload.
- Limites connues : worker série (pas de queue), pas de retry.

### Tunnels

- Onglet Tunnels : liste de cartes, formulaire créer / éditer / supprimer
  (Local `-L`, Remote `-R`, Dynamic `-D`), validation des ports, sonde de port
  local libre, toggle start/stop, messages d’erreur lisibles, badge sur l’onglet.
- Processus `ssh -N` supervisés par `TunnelRegistry` ; tués à la fermeture de
  l’app ou à la suppression de l’hôte.

### Au-delà de la roadmap initiale

- **Snippets** : vue, formulaire d’ajout, persistance (`CreateSnippet`).
- **Historique shell** : `history_worker` + `shell_integration` (hooks shell
  locaux), vue Historique, action « Clear History ».
- **Palette** : Open Host…, Open SFTP, Filter Servers, Show Files / Tunnels /
  Snippets / History, Check for Updates / Install Update / Restart to Update.
- **Auto-update** (`terminus-update`, `updater.rs`).
- **CI** : build + test + rustfmt + clippy à chaque PR (workspace clippy-clean).

### Infra

- Hot-reload / loops de dev documentés (`DEVELOPMENT.md`).
- Tests unitaires denses sur `terminus-ui` (géométrie), tunnels, SFTP et core.

---

## Phase 1 — Fondations (clôturer le MVP shell)

**Objectif :** sessions et découverte d’hôtes au niveau « produit quotidien ».

| # | Jalon | Reste à faire | Critère de done |
| :--- | :--- | :--- | :--- |
| 1.2b | **Bridge unifié** | Extraire / formaliser `TerminusCoreService` depuis `hosts.rs` ; exposer list/connect/vault/sync | Un seul runtime Arc ; rioterm n’embed plus le worker ad hoc |
| 1.4-debt | **Shell russh (Option B, plus tard)** | Remplacer `ssh_shell` CLI (et les tunnels `ssh -N`) par `SshTransport` + `SessionSpec::Ssh` + modal TOFU | Un seul stack SSH (shell = SFTP = forwards) |
| 1.5b | **Sidebar resize** | Poignée de resize + persist largeur (aujourd’hui constante `SIDEBAR_WIDTH = 260`) | Largeur utilisateur persistée |
| 1.5c | **États hôte** | `HostStatus` n’a que Idle / Running / Stopped / Active ; ajouter `connected` / `connecting` / `error` dérivés des sessions live (le shimmer connecting est un indicateur séparé) | Trois états visibles et justes |
| 1.7b | **Fade vault** | Animation d’apparition 120 ms (optionnel ; `anim.rs` fournit `Tween` mais rioterm ne l’utilise pas pour les dialogs ; lié à 4.2 reduced-motion) | Fade respectant reduced-motion |
| 1.8 | **Session lifecycle** | Étendre la détection de drop au-delà de exit 255 et aux onglets multi-panneaux ; brancher ou supprimer `SessionManager` / `SessionState::Disconnected` | Pas d’onglet zombie ; message clair pour toute sortie non propre |

---

## Phase 2 — Ops (forwards + SFTP mature)

**Objectif :** hub fichiers / tunnels sans quitter la fenêtre.

| # | Jalon | Reste à faire | Critère de done |
| :--- | :--- | :--- | :--- |
| 2.1 | **Port forwarding (finition)** | Stats (connexions / octets / uptime) ; actions palette start/stop ; décider du sort de `ForwardRuntime` (direct-tcpip russh) vs processus `ssh` (voir 1.4-debt) | Tunnel observable et pilotable sans souris |
| 2.2b | **SFTP polish** | Breadcrumbs : clic sur un segment navigue vers ce segment (aujourd’hui = un niveau up) | Parité « explorateur » basique |
| 2.3 | **Transferts robustes** | Queue (worker série aujourd’hui), retry sur échec, vue de file | Plusieurs gros transferts sans bloquer l’UI ni se perdre sur erreur |
| 2.4 | **Palette ops** | Syntaxe `>sftp`, `>forward …`, actions tunnel dans la palette | Actions sans souris |
| 2.5 | **Onboarding** | Empty sidebar 3 étapes avec CTA « Add first host » (aujourd’hui : un hint texte) | Premier host < 60 s |

---

## Phase 3 — Scale, sync & bulk

| # | Jalon | Détails | Critère de done |
| :--- | :--- | :--- | :--- |
| 3.1 | **Open All / kill group** | Entrées dans le context menu groupe | Flotte ouverte / fermée en un geste |
| 3.2 | **Sync-aware chrome** | Badge last-synced avec horodatage, pending, review des conflits (`SyncReport.conflicts` existe déjà) | Sync visible hors Settings |
| 3.3 | **Host Inspector** | Overlay compact OS / tags / actions | Actions rapides sans ouvrir Settings |
| 3.4b | **Recall Store** | Reprise basée sur le Store / `SessionManager` si besoin au-delà du fichier `saved_tabs` (layouts, panneaux, SFTP) | Restauration fidèle des panneaux |
| 3.5 | **WSL reconnect** | Auto-relance dist après drop | Distro WSL resilient |

---

## Phase 4 — Polish & GA

| # | Jalon | Détails | Critère de done |
| :--- | :--- | :--- | :--- |
| 4.1 | **Thèmes Terminus** | 7 presets + tokens chrome (2 aujourd’hui) | Thèmes switchables |
| 4.2 | **A11y contraste** | High-contrast + reduced-motion ; réutiliser le ratio WCAG de `theme.rs` | Presets WCAG |
| 4.3 | **Keyboard audit** | Doc raccourcis + focus rings | Parcours sans souris documenté |
| 4.4 | **Perf** | Benches 100+ hôtes (`criterion` est déjà en dev-dep), mémoire ; virtualiser la liste d’hôtes | Seuils dans CI ou script |
| 4.5 | **Release Terminus** | Workflow GitHub de release (aujourd’hui `scripts/release.sh` manuel) ; packaging macOS (`.dmg` « Terminus ») ; les artefacts Linux (tarball, deb, rpm) et Windows (NSIS, MSI) existent déjà dans `release.sh` | Artefacts « Terminus » publiés par la CI, macOS inclus |

---

## Priorités (recalées)

| Priorité | Focus | Pourquoi |
| :--- | :--- | :--- |
| **P0** | Session lifecycle (au-delà de exit 255), queue + retry des transferts | Bloque la confiance « daily driver » |
| **P0** | Tunnels : stats + palette, décision `ForwardRuntime` vs `ssh -N` | MVP livré, reste l’observabilité |
| **P1** | États hôte (1.5c), bridge unifié, sync chrome, Open All, onboarding, breadcrumbs | Scale + dette archi |
| **P1** | 1.4-debt shell russh unifié | Deux piles SSH (CLI pour shell + tunnels, russh pour SFTP) ; à trancher quand l’auth unique devient bloquante |
| **P2** | Thèmes, a11y, perf, workflow de release | GA |

---

## Risques (à jour)

| Risque | Impact | État mitigation |
| :--- | :--- | :--- |
| Deux piles SSH (OpenSSH CLI pour shell + tunnels, russh pour SFTP) | Auth / host-key divergents entre CLI et SFTP | **Accepté (Option A)** ; unification = 1.4-debt |
| Code mort côté core (`ForwardRuntime`, `SessionManager`, `TerminusCoreService`) | Dette, faux sentiment de couverture | 1.2b / 2.1 / 1.8 : brancher ou supprimer |
| Bridge stub + workers dans rioterm | Dette de structure | 1.2b |
| Transferts SFTP sans queue ni retry | Ops limitées, perte silencieuse sur erreur | 2.3 |
| Détection de perte limitée à SSH exit 255 | Onglets fermés sans explication pour les autres sorties | 1.8 |
| Liste d’hôtes non virtualisée | Lag 100+ | Toujours à faire (4.4 / sidebar) |
| Pas de workflow de release CI | Release manuelle (`release.sh`) | 4.5 |
| Vault Argon2id | Freeze UI si mal placé | Worker dédié ✅ |

---

## Hors scope actuel (ne pas confondre avec « manquant »)

- Remplacer Rio en tant que terminal générique (héritage upstream OK).
- WASM / librio comme surface Terminus.
- Parité feature-complete Termius/SecureCRT avant Phase 4.
