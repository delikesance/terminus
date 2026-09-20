{
  # rust-overlay deps
  rust-toolchain,
  makeRustPlatform,
  # Normal deps
  lib,
  stdenv,
  darwin,
  autoPatchelfHook,
  cmake,
  ncurses,
  pkg-config,
  gcc-unwrapped,
  fontconfig,
  libGL,
  vulkan-loader,
  libxkbcommon,
  withX11 ? !stdenv.hostPlatform.isDarwin,
  libX11,
  libXcursor,
  libXi,
  libXrandr,
  libxcb,
  withWayland ? !stdenv.hostPlatform.isDarwin,
  wayland,
  withWgpu ? !stdenv.hostPlatform.isDarwin,
  shaderc,
  krb5,
  ...
}: let
  readTOML = f: builtins.fromTOML (builtins.readFile f);
  cargoToml = readTOML ./Cargo.toml;
  rustPlatform = makeRustPlatform {
    cargo = rust-toolchain;
    rustc = rust-toolchain;
  };
  rlinkLibs =
    lib.optionals stdenv.hostPlatform.isLinux [
      (lib.getLib gcc-unwrapped)
      fontconfig
      libGL
      libxkbcommon
      vulkan-loader
      krb5
    ]
    ++ lib.optionals withX11 [
      libX11
      libXcursor
      libXi
      libXrandr
      libxcb
    ]
    ++ lib.optionals withWayland [
      wayland
    ];

  inherit (lib.fileset) unions toSource;
in
  rustPlatform.buildRustPackage {
    inherit (cargoToml.workspace.package) version;
    pname = "terminus";
    src = toSource {
      root = ./.;
      fileset = unions ([
          ./Cargo.lock
          ./Cargo.toml
          ./misc # Extra desktop/terminfo files
        ]
        ++ (map (x: ./. + "/${x}") cargoToml.workspace.members));
    };
    cargoLock.lockFile = ./Cargo.lock;

    cargoBuildFlags = "-p rioterm";

    # Match scripts/release.sh: fat LTO from Cargo.toml makes sandboxed
    # links take an hour-plus on modest machines.
    CARGO_PROFILE_RELEASE_LTO = "false";
    CARGO_PROFILE_RELEASE_CODEGEN_UNITS = "16";

    buildInputs = rlinkLibs ++ (lib.optionals stdenv.hostPlatform.isDarwin [darwin.libutil]);
    runtimeDependencies = rlinkLibs;

    nativeBuildInputs =
      [
        rustPlatform.bindgenHook
        ncurses
        shaderc
      ]
      ++ lib.optionals stdenv.hostPlatform.isLinux [
        cmake
        pkg-config
        autoPatchelfHook
      ];

    outputs = [
      "out"
      "terminfo"
    ];

    postInstall = ''
      install -D -m 644 misc/terminus.desktop -t \
                        $out/share/applications
      install -D -m 644 misc/logo.svg \
                        $out/share/icons/hicolor/scalable/apps/terminus.svg

      # Install terminfo files (Rio-compatible names)
      install -dm 755 "$terminfo/share/terminfo/r/"
      tic -xe xterm-rio,rio,rio-direct -o "$terminfo/share/terminfo" misc/rio.terminfo
      mkdir -p $out/nix-support
      echo "$terminfo" >> $out/nix-support/propagated-user-env-packages
    '';

    buildNoDefaultFeatures = true;
    buildFeatures =
      (lib.optionals withX11 ["x11"])
      ++ (lib.optionals withWayland ["wayland"])
      ++ (lib.optionals withWgpu ["wgpu"]);

    checkType = "debug";
    # Fail to run in the Nix sandbox (same skips as nixpkgs rio).
    checkFlags = [
      "--skip=sys::unix::eventedfd::EventedFd"
    ];

    meta = {
      description = "Hardware-accelerated GPU terminal emulator";
      longDescription = ''
        Terminus is a hardware-accelerated GPU terminal emulator based on Rio,
        with additional UI chrome and host-management features.
      '';
      homepage = cargoToml.workspace.package.homepage;
      license = lib.licenses.mit;
      platforms = lib.platforms.unix;
      changelog = "${cargoToml.workspace.package.repository}/blob/main/CHANGELOG.md";
      mainProgram = "terminus";
    };
  }
