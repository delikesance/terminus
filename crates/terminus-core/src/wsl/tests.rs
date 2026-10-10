use super::*;
use std::path::PathBuf;

/// Windows Terminal reports UTF-16LE; this is the shape `wsl -l -v` has
/// always had, header and all.
const VERBOSE: &str = "\r\n  NAME            STATE           VERSION\r\n* NixOS           Running         2\r\n  Ubuntu-24.04    Stopped         2\r\n  Debian          Installing      2\r\n";
const PLAIN: &str = "\r\nUbuntu-24.04\r\nNixOS\r\n";

/// Trimmed from a real Windows Terminal profile list: the Store profile,
/// the hidden registry profile behind it, and the unrelated
/// `Windows.Terminal.*` sources that must not be mistaken for distros.
const PROFILES: &str = r#"{
      "defaultProfile": "{61683bbc-3c0b-5397-985a-4b9aa31e5ea9}",
      "profiles": {
        "list": [
          { "guid": "{61c54bbd-c2c6-5271-96e7-009a87ff44bf}", "hidden": false, "name": "Windows PowerShell" },
          { "guid": "{b453ae62-4e3d-5e58-b989-0a998ec441b8}", "hidden": false, "name": "Azure Cloud Shell", "source": "Windows.Terminal.Azure" },
          { "guid": "{574e775e-4f2a-5b96-ac1e-a2962a402336}", "hidden": false, "name": "PowerShell", "source": "Windows.Terminal.PowershellCore" },
          { "guid": "{acbafd15-cbbb-5bb3-8a61-bed446ff4b83}", "hidden": false, "name": "Ubuntu 24.04 LTS", "source": "CanonicalGroupLimited.Ubuntu24.04LTS_79rhkp1fndgsc" },
          { "guid": "{963ff2f7-6aed-5ce3-9d91-90d99571f53a}", "hidden": true, "name": "Ubuntu-24.04", "source": "Windows.Terminal.Wsl" },
          { "guid": "{d8e96812-b789-5068-a5ae-10b2fb53e95f}", "hidden": false, "name": "Ubuntu 24.04.1 LTS", "source": "CanonicalGroupLimited.Ubuntu24.04LTS_79rhkp1fndgsc" },
          { "guid": "{61683bbc-3c0b-5397-985a-4b9aa31e5ea9}", "hidden": false, "name": "NixOS", "source": "Microsoft.WSL" },
          { "guid": "{5b93f4be-fad1-5578-baab-323904db4cce}", "hidden": false, "name": "Developer Command Prompt for VS 2022", "source": "Windows.Terminal.VisualStudio" }
        ]
      }
    }"#;

#[test]
fn verbose_list_parses_state_and_default() {
    let distros = parse_interop_list(VERBOSE);

    assert_eq!(distros.len(), 3);
    assert_eq!(distros[0].name, "NixOS");
    assert_eq!(distros[0].running, Some(true));
    assert!(distros[0].is_default);
    assert_eq!(distros[1].name, "Ubuntu-24.04");
    assert_eq!(distros[1].running, Some(false));
    assert!(!distros[1].is_default);
    assert_eq!(distros[2].name, "Debian");
    assert_eq!(distros[2].running, Some(false));
    assert_eq!(distros[2].source, Source::Interop);
}

#[test]
fn plain_list_parses_names_without_state() {
    let distros = parse_interop_list(PLAIN);

    assert_eq!(distros.len(), 2);
    assert_eq!(distros[0].name, "Ubuntu-24.04");
    assert_eq!(distros[0].running, None);
    assert_eq!(distros[0].source, Source::Interop);
}

#[test]
fn a_name_with_spaces_is_not_three_columns() {
    let distros =
        parse_interop_list("\n  NAME STATE VERSION\n  Alpine Linux  Stopped  2\n");

    assert_eq!(distros.len(), 1);
    assert_eq!(distros[0].name, "Alpine Linux");
    assert_eq!(distros[0].running, Some(false));
}

#[test]
fn the_header_is_not_a_distro() {
    assert!(parse_interop_list("  NAME  STATE  VERSION\n").is_empty());
    assert!(parse_interop_list("").is_empty());
    assert!(parse_interop_list("\r\n \r\n").is_empty());
}

#[test]
fn terminal_profiles_keep_only_wsl_distros() {
    let profiles = parse_terminal_profiles(PROFILES);

    let names: Vec<&str> = profiles.names.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["Ubuntu-24.04", "NixOS"]);

    let displays: Vec<&str> = profiles.displays.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(displays, vec!["Ubuntu 24.04 LTS", "Ubuntu 24.04.1 LTS"]);

    // Azure, PowerShell and Visual Studio share the `source` key but are
    // not distros.
    assert!(!names.contains(&"Azure Cloud Shell"));
    assert!(!displays.iter().any(|name| name.contains("Developer")));

    // The default profile is the NixOS one.
    assert!(profiles
        .names
        .iter()
        .any(|p| p.name == "NixOS" && p.is_default));
}

#[test]
fn store_listing_supplies_the_friendly_name() {
    let profiles = parse_terminal_profiles(PROFILES);
    assert_eq!(
        best_display("Ubuntu-24.04", &profiles).as_deref(),
        Some("Ubuntu 24.04 LTS")
    );
    // A distro with no Store listing keeps its registered name.
    assert_eq!(best_display("NixOS", &profiles), None);
}

#[test]
fn disks_are_matched_by_package_family() {
    let profiles = parse_terminal_profiles(PROFILES);

    let ubuntu = DiskDistro {
        package: Some(short_package_name(
            "CanonicalGroupLimited.Ubuntu24.04LTS_79rhkp1fndgsc",
        )),
        path: PathBuf::from("/c/Ubuntu/ext4.vhdx"),
    };
    let nameless = DiskDistro {
        package: None,
        path: PathBuf::from("/c/wsl/{a007b793}/ext4.vhdx"),
    };

    let discovery = merge(&[], &profiles, &[ubuntu, nameless]);

    assert_eq!(discovery.distros.len(), 2);
    assert_eq!(discovery.distros[0].display, "Ubuntu 24.04 LTS");
    assert_eq!(discovery.distros[0].name, "Ubuntu-24.04");
    assert_eq!(discovery.distros[1].name, "NixOS");
    // The Ubuntu disk is accounted for; the bare id folder is not.
    assert_eq!(discovery.unnamed, 1);
}

#[test]
fn interop_wins_and_keeps_its_state() {
    let profiles = parse_terminal_profiles(PROFILES);
    let interop = parse_interop_list(VERBOSE);

    let discovery = merge(&interop, &profiles, &[]);

    assert_eq!(discovery.distros.len(), 3);
    assert_eq!(discovery.distros[0].name, "NixOS");
    assert_eq!(discovery.distros[0].running, Some(true));
    assert_eq!(discovery.distros[0].source, Source::Interop);
    // Interop's names still get the Store name for display.
    let ubuntu = discovery
        .distros
        .iter()
        .find(|distro| distro.name == "Ubuntu-24.04")
        .expect("Ubuntu-24.04 in the merged list");
    assert_eq!(ubuntu.display, "Ubuntu 24.04 LTS");
}

#[test]
fn a_profile_interop_never_mentioned_is_still_listed() {
    let profiles = parse_terminal_profiles(PROFILES);
    let interop = parse_interop_list("\n  NAME STATE VERSION\n* NixOS  Running  2\n");

    let discovery = merge(&interop, &profiles, &[]);
    let names: Vec<&str> = discovery
        .distros
        .iter()
        .map(|distro| distro.name.as_str())
        .collect();

    assert_eq!(names, vec!["NixOS", "Ubuntu-24.04"]);
    assert_eq!(discovery.distros[1].source, Source::WindowsTerminal);
}

#[test]
fn windows_output_is_decoded_from_utf16() {
    let utf16: Vec<u8> = "\r\nUbuntu-24.04\r\n"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    assert_eq!(decode_windows_output(&utf16), "\r\nUbuntu-24.04\r\n");
    assert_eq!(decode_windows_output(b"Ubuntu-24.04\n"), "Ubuntu-24.04\n");
    assert_eq!(decode_windows_output(&[]), "");
}

#[test]
fn launch_args_are_what_wsl_expects() {
    let distro = WslDistro {
        name: "Ubuntu-24.04".to_string(),
        display: "Ubuntu 24.04 LTS".to_string(),
        source: Source::Interop,
        running: None,
        is_default: false,
    };

    assert_eq!(distro.launch_args(), vec!["-d", "Ubuntu-24.04"]);
}

#[test]
fn package_hashes_are_dropped_but_real_underscores_are_kept() {
    assert_eq!(
        short_package_name("CanonicalGroupLimited.Ubuntu24.04LTS_79rhkp1fndgsc"),
        "CanonicalGroupLimited.Ubuntu24.04LTS"
    );
    assert_eq!(
        short_package_name("Ubuntu_24.04"),
        "Ubuntu_24.04",
        "a short suffix is part of the name, not a version hash"
    );
    assert_eq!(short_package_name("NixOS"), "NixOS");
}

#[test]
fn roots_without_a_windows_drive_are_absent() {
    assert!(WindowsRoots::from_mount("/definitely/not/here", vec![])
        .wsl_exe()
        .is_none());
}

#[test]
fn garbage_json_is_not_a_panic() {
    assert!(parse_terminal_profiles("not json").is_empty());
    assert!(parse_terminal_profiles("{}").is_empty());
    assert!(parse_terminal_profiles(r#"{"profiles": {"list": []}}"#).is_empty());
}

#[test]
fn interop_hint_names_the_culprit() {
    // This test runs on a machine that may or may not have interop; the
    // hint must always say something actionable.
    let hint = interop_hint();
    assert!(hint.contains("WSL interop"));
}

/// Not a unit test: it prints what the *real* Windows side of whatever
/// machine it runs on answers. Fixtures can drift from the formats
/// `wsl.exe` and Windows Terminal actually emit, and this is the only
/// check that would notice.
///
///     cargo test -p terminus-core -- --ignored --nocapture discovery_on_this_machine
#[test]
#[ignore]
fn discovery_on_this_machine() {
    let Some(roots) = WindowsRoots::detect() else {
        println!("no Windows drive mounted: this is not a WSL machine");
        return;
    };

    println!("windows mount   {:?}", roots.mount);
    println!("user profiles   {:?}", roots.profiles);
    println!("wsl.exe         {:?}", roots.wsl_exe());

    match roots.wsl_exe() {
        Some(exe) => match interop_list(&exe) {
            Some(list) => println!("interop         {list:?}"),
            None => println!("interop         unavailable ({})", interop_hint()),
        },
        None => println!("interop         no wsl.exe on the Windows drive"),
    }

    let profiles = roots
            .profiles
            .iter()
            .filter_map(|profile| {
                let path = profile.join(
                    "AppData/Local/Packages/Microsoft.WindowsTerminal_8wekyb3d8bbwe/LocalState/settings.json",
                );
                std::fs::read_to_string(path).ok()
            })
            .next();
    println!(
        "terminal        {:?}",
        profiles
            .as_deref()
            .map(|json| parse_terminal_profiles(json).names)
    );

    let discovered = discover(&roots);
    println!("on disk         {:?}", distro_disks(&roots));
    for distro in &discovered.distros {
        println!(
            "distro          {} ({}) [{:?} running={:?} default={}]",
            distro.display, distro.name, distro.source, distro.running, distro.is_default
        );
    }
    println!("unnamed         {}", discovered.unnamed);
    println!("current         {:?}", crate::machine::detect().wsl_distro);
}

/// `interop_ready` must not re-run `wsl.exe` on every session open: the
/// probe cold-starts the WSL service, and session opens are interactive.
/// The fake `wsl.exe` here counts its own invocations, so this fails if the
/// memoisation is ever dropped.
#[cfg(unix)]
#[test]
fn interop_is_probed_once_per_process() {
    use std::os::unix::fs::PermissionsExt;

    let dir =
        std::env::temp_dir().join(format!("terminus-wsl-probe-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let counter = dir.join("count");
    let script = dir.join("wsl");

    std::fs::write(
        &script,
        format!("#!/bin/sh\necho run >> '{}'\nexit 0\n", counter.display()),
    )
    .expect("script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
        .expect("chmod");

    assert_eq!(interop_ready(&script), Ok(()));
    assert_eq!(interop_ready(&script), Ok(()));

    let runs = std::fs::read_to_string(&counter).unwrap_or_default();
    assert_eq!(
        runs.lines().count(),
        1,
        "the probe ran {} times, not once",
        runs.lines().count()
    );

    let _ = std::fs::remove_dir_all(&dir);
}
