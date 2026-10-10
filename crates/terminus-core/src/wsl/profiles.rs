use super::*;

/// Everything Windows Terminal knows that `wsl.exe` did not already give us.
pub(super) fn terminal_profiles(roots: &WindowsRoots) -> TerminalProfiles {
    let mut found = TerminalProfiles::default();

    for profile in roots.local_app_data() {
        let settings = profile.join("Packages");

        let Ok(entries) = std::fs::read_dir(&settings) else {
            continue;
        };

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();

            if !name.starts_with("Microsoft.WindowsTerminal") {
                continue;
            }

            let settings = entry.path().join("LocalState/settings.json");

            if let Ok(json) = std::fs::read_to_string(&settings) {
                found.merge(parse_terminal_profiles(&json));
            }
        }
    }

    found
}

/// The names Windows Terminal has for the WSL distros it is configured with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TerminalProfiles {
    /// Distros Windows registered, in `wsl -l` vocabulary.
    pub(super) names: Vec<ProfileName>,
    /// Names from the Store listings, keyed by package family.
    pub(super) displays: Vec<ProfileName>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProfileName {
    pub(super) name: String,
    pub(super) hidden: bool,
    pub(super) package: Option<String>,
    pub(super) is_default: bool,
}

impl TerminalProfiles {
    pub(super) fn merge(&mut self, other: Self) {
        self.names.extend(other.names);
        self.displays.extend(other.displays);
    }

    /// Only the tests ask: the production path merges first and filters
    /// after, so it never needs to know whether the result is empty.
    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.names.is_empty() && self.displays.is_empty()
    }
}

/// Parses Windows Terminal's `settings.json`.
///
/// Only the profiles that *are* WSL distros survive: `Windows.Terminal.Wsl`
/// (the registry entries, often hidden behind a Store profile of the same
/// distro) and `Microsoft.WSL` (store WSL registers these directly). Everything
/// else under `Windows.Terminal.*` — Azure, PowerShell, Visual Studio — is a
/// different kind of thing wearing the same JSON.
pub fn parse_terminal_profiles(json: &str) -> TerminalProfiles {
    let Ok(document) = serde_json::from_str::<serde_json::Value>(json) else {
        return TerminalProfiles::default();
    };

    let default_profile = document
        .get("defaultProfile")
        .and_then(|value| value.as_str())
        .map(|guid| guid.to_lowercase());

    let Some(profiles) = document
        .get("profiles")
        .and_then(|profiles| profiles.get("list"))
        .and_then(|list| list.as_array())
    else {
        return TerminalProfiles::default();
    };

    let mut found = TerminalProfiles::default();

    for profile in profiles {
        let Some(name) = profile.get("name").and_then(|value| value.as_str()) else {
            continue;
        };

        let Some(source) = profile.get("source").and_then(|value| value.as_str()) else {
            continue;
        };

        let hidden = profile
            .get("hidden")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);

        let is_default = profile
            .get("guid")
            .and_then(|value| value.as_str())
            .map(|guid| guid.to_lowercase())
            .zip(default_profile.as_deref())
            .is_some_and(|(guid, default)| guid == default);

        let name = name.trim();

        if name.is_empty() {
            continue;
        }

        match source {
            "Windows.Terminal.Wsl" | "Microsoft.WSL" => found.names.push(ProfileName {
                name: name.to_string(),
                hidden,
                package: None,
                is_default,
            }),
            other if other.starts_with("Windows.Terminal.") => {}
            other => found.displays.push(ProfileName {
                name: name.to_string(),
                hidden,
                package: Some(short_package_name(other)),
                is_default,
            }),
        }
    }

    found
}

/// Drops the version hash Windows appends to a package family name, so a
/// Store profile (`CanonicalGroupLimited.Ubuntu24.04LTS_79rhkp1fndgsc`) matches
/// the folder on disk (`CanonicalGroupLimited.Ubuntu24.04LTS_79rhkp1fndgsc`).
pub(super) fn short_package_name(package: &str) -> String {
    match package.rsplit_once('_') {
        Some((family, hash))
            if hash.len() >= 8 && hash.chars().all(|c| c.is_ascii_alphanumeric()) =>
        {
            family.to_string()
        }
        _ => package.to_string(),
    }
}
