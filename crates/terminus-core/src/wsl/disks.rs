use std::path::PathBuf;

use super::*;

/// Distro disks (`ext4.vhdx`), with the package family that named them.
pub(super) fn distro_disks(roots: &WindowsRoots) -> Vec<DiskDistro> {
    let mut disks = Vec::new();
    let mut roots_to_scan = Vec::new();

    for local_app_data in roots.local_app_data() {
        // Store distros: one package family each.
        if let Ok(entries) = std::fs::read_dir(local_app_data.join("Packages")) {
            for entry in entries.flatten() {
                let disk = entry.path().join("LocalState/ext4.vhdx");

                if disk.is_file() {
                    roots_to_scan.push(DiskDistro {
                        package: Some(short_package_name(
                            &entry.file_name().to_string_lossy(),
                        )),
                        path: disk,
                    });
                }
            }
        }

        // Distros registered outside the Store live in per-id folders.
        if let Ok(entries) = std::fs::read_dir(local_app_data.join("wsl")) {
            for entry in entries.flatten() {
                let disk = entry.path().join("ext4.vhdx");

                if disk.is_file() {
                    roots_to_scan.push(DiskDistro {
                        package: None,
                        path: disk,
                    });
                }
            }
        }
    }

    disks.extend(roots_to_scan);
    disks.sort_by(|a, b| a.path.cmp(&b.path));
    disks
}

/// A distro disk and, when it came from the Store, the package that owns it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskDistro {
    /// Package family without its version hash, e.g.
    /// `CanonicalGroupLimited.Ubuntu24.04LTS`.
    pub package: Option<String>,
    pub path: PathBuf,
}
