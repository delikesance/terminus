# Linux distribution releases

Run `nix run .#release -- --build-only --linux-only` from the repository root.
The release shell supplies packaging tools; Docker must be running and accessible
to the user. Without Nix, install Docker, Python 3, binutils, nfpm, tar and Nix
(for generating the separate NixOS package expression), then run
`bash scripts/release.sh --build-only --linux-only`.

Linux compilation happens inside `misc/linux-release.Dockerfile` using Ubuntu
22.04 libraries and the Rust version in `rust-toolchain.toml`. Host compiler
flags, library paths and development build outputs are not passed to Docker.
The source is mounted read-only, compilation runs as the invoking user, and
build/cache files live in `target/linux-release` and `.dev/linux-release-cargo`.
The first build needs network access to install the toolchain and dependencies.
The static musl `terminus-walk` helper is built first so the application embeds
the remote file exploration/sync helper even with an empty build cache.

`scripts/check-linux-release.py` rejects a binary unless it uses
`/lib64/ld-linux-x86-64.so.2`, has no library search paths or absolute shared
library dependencies, and requires at most glibc 2.35 and GCC 11's libstdc++
symbols. The container also runs `terminus --version` before packaging. RPM,
DEB and tar.gz all consume the validated `dist/linux/terminus` copy, never
`target/release/terminus`. Native packages require glibc >= 2.35 and libstdc++
>= 11; older distributions need a build against their own system libraries.

NixOS users still install via the generated `terminus.nix`, whose
`autoPatchelfHook` adapts the distribution binary to their local Nix libraries.
Nix paths belong in that installed derivation, not in the downloaded RPM/DEB.

Run the regression checks with:

```sh
python3 scripts/test-linux-release.py
bash scripts/test-release-signing.sh
# After producing the packages (requires Docker and network access):
bash scripts/test-linux-packages.sh
```

The Linux tests compile small ELF fixtures with gcc, including a Nix interpreter,
Nix/build-directory search paths and a newer glibc requirement. They also exercise
the release script with simulated build/package tools, checking that rejected
binaries never reach packaging and all three formats use the same valid binary.
The package installation test runs the DEB in clean Ubuntu 22.04 and the RPM in
clean Fedora 44, verifying `terminus --version` and shared library resolution.
RPM Wayland dependencies use library ABI provides rather than a distribution's
package names, which can change when libraries are split into separate packages.
