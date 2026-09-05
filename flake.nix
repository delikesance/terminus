{
  description = "Terminus — open-source Termius alternative (Rust + Tauri)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      flake-utils,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [
            "rust-src"
            "rust-analyzer"
            "clippy"
            "rustfmt"
          ];
          targets = [ "x86_64-pc-windows-msvc" ];
        };

        linuxNative = with pkgs; [
          pkg-config
          wrapGAppsHook4
        ];

        linuxLibs = with pkgs; [
          webkitgtk_4_1
          gtk3
          cairo
          gdk-pixbuf
          glib
          dbus
          openssl
          libsoup_3
          librsvg
          pango
          harfbuzz
          at-spi2-atk
          glib-networking
          gsettings-desktop-schemas
        ];

        commonTools = with pkgs; [
          rustToolchain
          cargo-tauri
          nodejs_22
          python3
          pkg-config
          openssl
          git
          docker-compose
          openssh
        ]
        ;

        linuxHook = pkgs.lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
          export XDG_DATA_DIRS="${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:${pkgs.gtk3}/share/gsettings-schemas/${pkgs.gtk3.name}:''${XDG_DATA_DIRS:-}"
          export GIO_MODULE_DIR="${pkgs.glib-networking}/lib/gio/modules/"
          export WEBKIT_DISABLE_COMPOSITING_MODE="''${WEBKIT_DISABLE_COMPOSITING_MODE:-1}"
        '';
      in
      {
        devShells.default = pkgs.mkShell {
          name = "terminus";
          packages =
            commonTools ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux (linuxNative ++ linuxLibs);

          nativeBuildInputs = pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux linuxNative;
          buildInputs = pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux linuxLibs;

          shellHook = ''
            export CARGO_TARGET_DIR="''${CARGO_TARGET_DIR:-$PWD/target}"
            export RUST_BACKTRACE="''${RUST_BACKTRACE:-1}"
            unset CC CFLAGS
            ${linuxHook}
            echo "Terminus dev shell — rustc $(rustc --version) · node $(node --version)"
          '';
        };

        packages.default = self.packages.${system}.terminus-selftest;

        packages.terminus-selftest = pkgs.rustPlatform.buildRustPackage {
          pname = "terminus-selftest";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          buildAndTestSubdir = "crates/terminus-core";
          cargoBuildFlags = [
            "--bin"
            "terminus-selftest"
          ];
          doCheck = false;
          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = [ pkgs.openssl ];
        };
      }
    );
}
