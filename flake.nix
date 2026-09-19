{
  description = "Terminus | A hardware-accelerated GPU terminal emulator";

  nixConfig = {
    extra-substituters = ["https://rioterm.cachix.org"];
    extra-trusted-public-keys = ["rioterm.cachix.org-1:cs/H9Jf0ZpHyR4WgjoNJZVBpkVi69Y4JASUK5ReEQPE="];
  };

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    systems.url = "github:nix-systems/default";
  };

  outputs = inputs @ {flake-parts, ...}:
    flake-parts.lib.mkFlake {inherit inputs;} {
      imports = [flake-parts.flakeModules.easyOverlay];

      systems = import inputs.systems;

      perSystem = {
        self',
        inputs',
        pkgs,
        system,
        lib,
        ...
      }: let
        # Defines a devshell using the `rust-toolchain`, allowing for
        # different versions of rust to be used.
        mkDevShell = rust-toolchain: let
          runtimeDeps = self'.packages.rio.runtimeDependencies;
          tools =
            self'.packages.rio.nativeBuildInputs
            ++ self'.packages.rio.buildInputs
            ++ [rust-toolchain]
            ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [pkgs.krb5 pkgs.pkg-config];
        in
          pkgs.mkShell {
            packages = [self'.formatter] ++ tools;
            LD_LIBRARY_PATH = "${lib.makeLibraryPath (runtimeDeps ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [pkgs.krb5.lib])}";
          };
        toolchains = rec {
          msrv = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          stable = pkgs.rust-bin.stable.latest.minimal;
          nightly = pkgs.rust-bin.selectLatestNightlyWith (toolchain: toolchain.minimal);
          rio = msrv;
          default = rio;
        };
        # Cross-compile to Windows from WSL. Keep this out of the default
        # shell so Linux `nix develop` / CI do not fetch rust-std-msvc.
        rustToolchainToml = builtins.fromTOML (builtins.readFile ./rust-toolchain.toml);
        windowsToolchain = pkgs.rust-bin.fromRustupToolchain {
          channel = rustToolchainToml.toolchain.channel;
          profile = rustToolchainToml.toolchain.profile or "minimal";
          components = rustToolchainToml.toolchain.components or [];
          targets = ["x86_64-pc-windows-msvc"];
        };
        windowsPackages = [
          self'.formatter
          windowsToolchain
          pkgs.cargo-xwin
          pkgs.clang
          pkgs.llvmPackages.clang-unwrapped
          pkgs.llvmPackages.bintools
          pkgs.llvmPackages.lld
          pkgs.llvmPackages.llvm
          pkgs.llvmPackages.libclang
          pkgs.nasm
          pkgs.cmake
          pkgs.pkg-config
          pkgs.shaderc
        ];
        windowsDevShell = pkgs.mkShell {
          packages = windowsPackages;
          LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
          shellHook = ''
            export XWIN_CACHE_DIR="''${XWIN_CACHE_DIR:-$PWD/.dev/xwin-cache}"
            mkdir -p "$XWIN_CACHE_DIR"
          '';
        };

        # Linux system deps needed by `cargo build` (bindgen/clang, shaderc,
        # fontconfig/krb5/X11/Wayland .pc files). The rust toolchains that
        # the `rio` Nix package embeds are filtered out: `windowsToolchain`
        # below is the only toolchain here and carries both the linux-gnu
        # host std and the windows-msvc target std.
        linuxBuildDeps =
          (lib.filter (p:
            !(lib.hasInfix "rust-minimal" (p.name or ""))
            && !(lib.hasInfix "auditable-rust-minimal" (p.name or "")))
            self'.packages.rio.nativeBuildInputs)
          ++ self'.packages.rio.buildInputs;

        # Release shell: Linux system deps + the Windows cross toolchain
        # + publishing tools. One shell builds both targets.
        releaseShell = pkgs.mkShell {
          packages =
            linuxBuildDeps
            ++ [pkgs.krb5 pkgs.pkg-config]
            ++ windowsPackages
            ++ [
              pkgs.gh
              pkgs.p7zip
              pkgs.gnutar
              pkgs.gzip
              pkgs.git
              pkgs.coreutils
            ];
          LD_LIBRARY_PATH = lib.makeLibraryPath (
            self'.packages.rio.runtimeDependencies
            ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [pkgs.krb5.lib]
          );
          LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
          shellHook = ''
            export XWIN_CACHE_DIR="''${XWIN_CACHE_DIR:-$PWD/.dev/xwin-cache}"
            mkdir -p "$XWIN_CACHE_DIR"
          '';
        };
      in {
        formatter = pkgs.alejandra;
        _module.args.pkgs = import inputs.nixpkgs {
          inherit system;
          overlays = [(import inputs.rust-overlay)];
        };

        # Create overlay to override `rio` with this flake's default
        overlayAttrs = {inherit (self'.packages) rio;};
        packages =
          lib.mapAttrs' (
            k: v: {
              name =
                if builtins.elem k ["rio" "default"]
                then k
                else "rio-${k}";
              value = pkgs.callPackage ./pkgRio.nix {rust-toolchain = v;};
            }
          )
          toolchains;
        # Different devshells for different rust versions, plus a Windows
        # cross shell used by `scripts/dev-win.sh` (`nix develop .#windows`).
        devShells =
          (lib.mapAttrs (_: v: mkDevShell v) toolchains)
          // {
            windows = windowsDevShell;
            release = releaseShell;
          };

        # Single-command release: builds Linux + Windows, then publishes
        # the artifacts as a GitHub release via `gh`.
        apps.release = {
          type = "app";
          program = toString (pkgs.writeShellScript "terminus-release" ''
            set -euo pipefail
            root="$(pwd)"
            if [[ ! -f "$root/flake.nix" || ! -f "$root/scripts/release.sh" ]]; then
              echo "terminus-release: run from the repository root (where flake.nix lives)" >&2
              exit 1
            fi
            exec nix develop "$root#release" --command bash "$root/scripts/release.sh" "$@"
          '');
        };
      };
    };
}
