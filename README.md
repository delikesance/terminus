<p align="center">
  <img src="docs/assets/hero.png" alt="Terminus — the terminal that knows your servers" width="100%">
</p>

<p align="center">
  <a href="https://github.com/delikesance/terminus/releases">Download</a>
  ·
  <a href="#features">Features</a>
  ·
  <a href="DEVELOPMENT.md">Development</a>
  ·
  <a href="milestone.md">Roadmap</a>
</p>

Terminus is a terminal with a built-in SSH / SFTP / WSL host manager. It is a fork of
[Rio](https://github.com/raphamorim/rio) (GPU-rendered, cross-platform) with its own
interface on top.

## Features

- **Host manager**: save servers, organise them in groups and open a tab in one click.
- **SFTP file browser**: dual-pane transfers between your machine and a host, or between two hosts.
- **WSL aware**: distros are discovered automatically and listed next to your local shell.
- **Encrypted vault**: credentials stay locked behind an Argon2id-derived key.
- **Command palette**: fuzzy-search hosts and commands from the keyboard; tabs and splits included.
- **Signed updates**: releases are verified with minisign and SHA-256 before they are applied.

<p align="center">
  <img src="docs/assets/features.png" alt="Terminus features" width="100%">
</p>

## Screenshots

| Command palette | Add a server | Settings |
| :---: | :---: | :---: |
| <img src="docs/assets/screenshot-palette.png" alt="Command palette"> | <img src="docs/assets/screenshot-addserver.png" alt="Add a server wizard"> | <img src="docs/assets/screenshot-settings.png" alt="Settings"> |

## Install

Packages for Linux (`.deb`, `.rpm`, tarball, Nix) and Windows (installer, `.msi`, zip) are on the
[releases page](https://github.com/delikesance/terminus/releases). To build from source, see
[DEVELOPMENT.md](DEVELOPMENT.md).

## Credits

Terminus builds on [Rio](https://rioterm.com) by Raphael Amorim (MIT). Rio's crates
(`sugarloaf`, `rio-backend`, `rio-window`, `teletypewriter`, …) are kept as upstream code.

## Minimal stable rust version

Rio's MSRV is 1.96.1.
