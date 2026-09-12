# Shared Terminus desktop package for the flake and nixpkgs porting.
# Call with pkgs.callPackage ./nix/package.nix { inherit rustPlatform; src = ...; }
{
  lib,
  stdenv,
  rustPlatform,
  fetchNpmDeps,
  cargo-tauri,
  nodejs_22,
  npmHooks,
  pkg-config,
  patchelf,
  openssl,
  wrapGAppsHook4,
  clang,
  llvmPackages,
  webkitgtk_4_1,
  gtk3,
  cairo,
  gdk-pixbuf,
  glib,
  dbus,
  libsoup_3,
  librsvg,
  pango,
  harfbuzz,
  at-spi2-atk,
  glib-networking,
  gsettings-desktop-schemas,
  krb5,
  src,
  version ? "0.1.0",
}:

rustPlatform.buildRustPackage (
  finalAttrs: {
    pname = "terminus";
    inherit version src;

    cargoLock.lockFile = src + "/Cargo.lock";

    npmDeps = fetchNpmDeps {
      name = "${finalAttrs.pname}-${finalAttrs.version}-npm-deps";
      inherit (finalAttrs) src;
      hash = "sha256-EILWqrsEF6NCjRiiqG+I9nDMXNZfcG1Lpx8sz8PH8Go=";
    };

    nativeBuildInputs =
      [
        cargo-tauri.hook
        nodejs_22
        npmHooks.npmConfigHook
        pkg-config
        patchelf
      ]
      ++ lib.optionals stdenv.hostPlatform.isLinux [
        wrapGAppsHook4
        clang
      ];

    buildInputs =
      [
        openssl
      ]
      ++ lib.optionals stdenv.hostPlatform.isLinux [
        webkitgtk_4_1
        gtk3
        cairo
        gdk-pixbuf
        glib
        dbus
        libsoup_3
        librsvg
        pango
        harfbuzz
        at-spi2-atk
        glib-networking
        gsettings-desktop-schemas
        krb5
        llvmPackages.libclang
      ];

    # Workspace root owns Cargo.lock; Tauri app lives in src-tauri.
    buildAndTestSubdir = "src-tauri";

    # cargo-tauri.hook defaults to a deb bundle and installs data/usr → $out.
    tauriBundleType = "deb";

    LIBCLANG_PATH = lib.optionalString stdenv.hostPlatform.isLinux "${llvmPackages.libclang.lib}/lib";

    doCheck = false;

    meta = {
      description = "Open-source Termius alternative — SSH/SFTP terminal";
      homepage = "https://github.com/delikesance/terminus";
      license = lib.licenses.mit;
      mainProgram = "terminus";
      platforms = lib.platforms.linux;
    };
  }
)
