use super::*;

/// Folds the three sources into one list.
pub(super) fn merge(
    interop: &[WslDistro],
    profiles: &TerminalProfiles,
    disks: &[DiskDistro],
) -> Discovery {
    let mut distros: Vec<WslDistro> = Vec::new();

    // Interop first: it is the only source that also knows about state.
    for distro in interop {
        distros.push(distro.clone());
    }

    // Then whatever Windows Terminal knows that interop did not already name.
    for profile in &profiles.names {
        if distros
            .iter()
            .any(|distro| distro.name.eq_ignore_ascii_case(&profile.name))
        {
            continue;
        }

        distros.push(WslDistro {
            name: profile.name.clone(),
            display: profile.name.clone(),
            source: Source::WindowsTerminal,
            running: None,
            is_default: profile.is_default,
        });
    }

    // Store listings only ever improve a name we already have.
    for distro in &mut distros {
        if let Some(display) = best_display(&distro.name, profiles) {
            distro.display = display;
        }
    }

    // A disk is only worth reporting when nothing named it: a distro we cannot
    // pass to `wsl -d` would be a row that cannot open anything.
    let named_packages: Vec<String> = distros
        .iter()
        .flat_map(|distro| package_for(&distro.name, profiles))
        .collect();

    let unnamed = disks
        .iter()
        .filter(|disk| match &disk.package {
            Some(package) => !named_packages.iter().any(|named| named == package),
            // Store WSL keeps its disks in bare id folders; without a name
            // there is nothing to say about them beyond "not launchable".
            None => true,
        })
        .count();

    Discovery { distros, unnamed }
}

/// The Store listing's name for a registered distro, if it has one.
pub(super) fn best_display(name: &str, profiles: &TerminalProfiles) -> Option<String> {
    let wanted = normalize_name(name);

    let mut candidates: Vec<&ProfileName> = profiles
        .displays
        .iter()
        .filter(|display| normalize_name(&display.name).starts_with(&wanted))
        .collect();

    // The friendly name and the registered name of one distro can differ by a
    // service-pack suffix ("Ubuntu 24.04" vs "Ubuntu 24.04.1 LTS"); the shorter
    // one is the name of the product rather than of the build.
    candidates.sort_by_key(|candidate| (candidate.hidden, candidate.name.len()));

    candidates.first().map(|candidate| candidate.name.clone())
}

/// The package families that could plausibly be this distro's, so its disk can
/// be recognised.
pub(super) fn package_for(name: &str, profiles: &TerminalProfiles) -> Option<String> {
    let wanted = normalize_name(name);

    profiles
        .displays
        .iter()
        .find(|display| normalize_name(&display.name).starts_with(&wanted))
        .and_then(|display| display.package.clone())
}

pub(super) fn normalize_name(name: &str) -> String {
    name.chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(|character| character.to_lowercase())
        .collect()
}
