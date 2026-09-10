# Contributing to Terminus

Thanks for wanting to help. Terminus is **MIT-licensed** — anyone can use, modify, and contribute.

## Ways to contribute

- **Bug reports & ideas** — [GitHub Issues](https://github.com/delikesance/terminus/issues)
- **Pull requests** — fix bugs, improve UX, docs, tests, or tooling
- **Discussions** — design questions and feature proposals on the issue tracker

You do **not** need to ask permission to open a PR. Small, focused changes are easiest to review.

## Development setup

This project expects a [Nix](https://nixos.org) flake (recommended) so Rust, Node, and tooling match CI:

```bash
nix develop
npm install
python3 scripts/gen-icons.py
docker compose up -d --build   # optional: local SSH + Postgres fixtures
cargo run -p terminus-core --bin terminus-selftest
npm run tauri -- dev
```

Without Nix, install a recent Rust toolchain, Node 22+, and system deps for Tauri 2 on your OS.

### Useful checks

```bash
npm run selftest          # core selftest + JS unit suites
npm run test:host-drag-ghost
npm run build             # tsc + vite
npm run test:e2e          # Playwright (needs a prior build / preview)
```

## Pull request guidelines

1. Branch from an up-to-date `main` (prefer `feat/#N-short-slug` when an issue exists).
2. Keep the PR focused — one concern per PR when practical.
3. Prefer tests for behavior changes (unit under `src/*.test.js`, e2e under `e2e/`).
4. Do not commit secrets, local DB files, or `node_modules` / `target` / `dist`.
5. Describe **why** in the PR body; link `Closes #N` when applicable.

## Code of conduct

Be respectful and constructive. Harassment or bad-faith behavior is not welcome. Maintainers may close issues/PRs that violate that bar.

## License

By contributing, you agree that your contributions are licensed under the same **MIT License** as the project (`LICENSE`).
