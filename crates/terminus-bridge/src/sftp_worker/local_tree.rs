use super::*;

pub(super) fn sanitize_temp_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub(super) fn zip_options() -> zip::write::SimpleFileOptions {
    zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644)
}

pub(super) fn zip_local_tree(
    src: &Path,
    root_name: &str,
    zip_path: &Path,
) -> Result<(), String> {
    let file = std::fs::File::create(zip_path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip_options();
    zip.add_directory(format!("{root_name}/"), options)
        .map_err(|e| e.to_string())?;
    zip_local_dir_recursive(&mut zip, src, root_name, options)?;
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

/// Pack `src` as `{root_name}/…` into a gzipped tar (remote extract via `tar -xzf`).
pub(super) fn tar_gz_local_tree(
    src: &Path,
    root_name: &str,
    out: &Path,
) -> Result<(), String> {
    let src = src
        .canonicalize()
        .map_err(|e| format!("canonicalize {}: {e}", src.display()))?;
    let parent = src
        .parent()
        .ok_or_else(|| format!("no parent for {}", src.display()))?;
    let base = src
        .file_name()
        .ok_or_else(|| format!("no basename for {}", src.display()))?
        .to_string_lossy()
        .into_owned();

    // Match zip layout: archive top folder must be `root_name`.
    let status = if base == root_name {
        std::process::Command::new("tar")
            .arg("-C")
            .arg(parent)
            .arg("-czf")
            .arg(out)
            .arg(&base)
            .status()
            .map_err(|e| format!("local tar create: {e}"))?
    } else {
        // Rename via transform so extract still lands under `root_name/`.
        std::process::Command::new("tar")
            .arg("-C")
            .arg(parent)
            .arg("-czf")
            .arg(out)
            .arg(format!("--transform=s,^{base},{root_name},"))
            .arg(&base)
            .status()
            .map_err(|e| format!("local tar create: {e}"))?
    };
    if status.success() {
        Ok(())
    } else {
        Err(format!("local tar create failed ({status})"))
    }
}

pub(super) fn zip_local_dir_recursive(
    zip: &mut zip::ZipWriter<std::fs::File>,
    dir: &Path,
    prefix: &str,
    options: zip::write::SimpleFileOptions,
) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let rel = format!("{prefix}/{name}");
        let path = entry.path();
        let ft = entry.file_type().map_err(|e| e.to_string())?;
        if ft.is_dir() {
            zip.add_directory(format!("{rel}/"), options)
                .map_err(|e| e.to_string())?;
            zip_local_dir_recursive(zip, &path, &rel, options)?;
        } else if ft.is_file() || ft.is_symlink() {
            // Follow symlinks (npm .bin) so the zip stores real file bytes —
            // Windows extract cannot recreate Unix symlinks.
            let mut input = std::fs::File::open(&path).map_err(|e| e.to_string())?;
            zip.start_file(&rel, options).map_err(|e| e.to_string())?;
            std::io::copy(&mut input, zip).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Local zip extract (client-side dest).
pub(super) fn unzip_local(zip_path: &Path, dest_cwd: &Path) -> Result<(), String> {
    let file = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = entry.enclosed_name().map(|p| p.to_path_buf()) else {
            continue;
        };
        let out = dest_cwd.join(&rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut outfile = std::fs::File::create(&out).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut outfile).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
