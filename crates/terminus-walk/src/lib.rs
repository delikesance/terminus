//! Parallel directory walk helpers shared by the CLI and Terminus bridge.

use blake3::Hasher;
use jwalk::WalkDir;
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Bump when the on-wire meta/hash protocol changes.
pub const TOOL_VERSION: &str = "2";
pub const TOOL_NAME: &str = "terminus-walk";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetaEntry {
    pub size: u64,
    pub mtime_ns: Option<i128>,
}

pub type MetaMap = BTreeMap<String, MetaEntry>;
pub type DigestMap = BTreeMap<String, [u8; 32]>;

fn mtime_ns(meta: &std::fs::Metadata) -> Option<i128> {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as i128)
}

fn rel_key(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    if rel.as_os_str().is_empty() {
        return None;
    }
    Some(rel.to_string_lossy().replace('\\', "/"))
}

/// Parallel metadata snapshot of all files under `root` (no symlinks followed).
pub fn snapshot_meta(root: &Path) -> Result<MetaMap, String> {
    let root = root
        .canonicalize()
        .map_err(|e| format!("canonicalize {}: {e}", root.display()))?;
    let mut out = MetaMap::new();
    for entry in WalkDir::new(&root).follow_links(false) {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        if !meta.is_file() {
            continue;
        }
        let Some(rel) = rel_key(&root, &path) else {
            continue;
        };
        if rel == ".terminus-folder-sync" || rel.ends_with("/.terminus-folder-sync") {
            continue;
        }
        out.insert(
            rel,
            MetaEntry {
                size: meta.len(),
                mtime_ns: mtime_ns(&meta),
            },
        );
    }
    Ok(out)
}

fn hash_file(path: &Path) -> Result<[u8; 32], String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Hasher::new();
    // Heap buffer: a 1 MiB stack array + rayon worker stacks overflows on Linux
    // (remote `terminus-walk hash` aborted with "stack overflow" at ~3k files).
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(*hasher.finalize().as_bytes())
}

/// Hash listed relative paths under `root` in parallel (BLAKE3).
pub fn hash_paths(root: &Path, rels: &[String]) -> Result<DigestMap, String> {
    let root = root
        .canonicalize()
        .map_err(|e| format!("canonicalize {}: {e}", root.display()))?;
    let results: Result<Vec<_>, String> = rels
        .par_iter()
        .map(|rel| {
            if rel.starts_with('/')
                || rel.contains('\0')
                || rel.split('/').any(|p| p == "..")
            {
                return Err(format!("unsafe path: {rel}"));
            }
            let path = join_rel(&root, rel);
            let digest = hash_file(&path)?;
            Ok((rel.clone(), digest))
        })
        .collect();
    Ok(results?.into_iter().collect())
}

fn join_rel(root: &Path, relative: &str) -> PathBuf {
    let mut p = root.to_path_buf();
    for part in relative.split('/').filter(|s| !s.is_empty()) {
        p.push(part);
    }
    p
}

pub fn format_meta(map: &MetaMap) -> String {
    let mut s = format!("v{TOOL_VERSION}\n");
    for (rel, e) in map {
        match e.mtime_ns {
            Some(ns) => s.push_str(&format!("{}\t{ns}\t{rel}\n", e.size)),
            None => s.push_str(&format!("{}\t-\t{rel}\n", e.size)),
        }
    }
    s
}

pub fn parse_meta(text: &str) -> Result<MetaMap, String> {
    let mut lines = text.lines();
    let header = lines.next().unwrap_or("").trim();
    if header != format!("v{TOOL_VERSION}") {
        return Err(format!("unsupported meta header: {header}"));
    }
    let mut out = MetaMap::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(3, '\t');
        let (Some(sz), Some(mt), Some(rel)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        if rel.is_empty() {
            continue;
        }
        let size: u64 = sz
            .parse()
            .map_err(|e| format!("bad size in meta line: {e}"))?;
        let mtime_ns = if mt == "-" {
            None
        } else {
            Some(
                mt.parse::<i128>()
                    .map_err(|e| format!("bad mtime in meta line: {e}"))?,
            )
        };
        out.insert(rel.to_string(), MetaEntry { size, mtime_ns });
    }
    Ok(out)
}

pub fn format_digests(map: &DigestMap) -> String {
    let mut s = format!("v{TOOL_VERSION}\n");
    for (rel, d) in map {
        let hex: String = d.iter().map(|b| format!("{b:02x}")).collect();
        s.push_str(&format!("{hex}\t{rel}\n"));
    }
    s
}

pub fn parse_digests(text: &str) -> Result<DigestMap, String> {
    let mut lines = text.lines();
    let header = lines.next().unwrap_or("").trim();
    if header != format!("v{TOOL_VERSION}") {
        return Err(format!("unsupported hash header: {header}"));
    }
    let mut out = DigestMap::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(2, '\t');
        let (Some(hex), Some(rel)) = (parts.next(), parts.next()) else {
            continue;
        };
        if rel.is_empty() || hex.len() != 64 {
            continue;
        }
        let mut digest = [0u8; 32];
        for i in 0..32 {
            digest[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
                .map_err(|e| format!("bad digest hex: {e}"))?;
        }
        out.insert(rel.to_string(), digest);
    }
    Ok(out)
}

/// Write a NUL-separated relative path list for the remote `hash` command stdin.
pub fn encode_path_list(rels: &[String]) -> Vec<u8> {
    let mut out = Vec::new();
    for rel in rels {
        out.extend_from_slice(rel.as_bytes());
        out.push(0);
    }
    out
}

pub fn run_cli<I, S>(args: I, stdout: &mut dyn Write) -> Result<(), String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args: Vec<String> = args.into_iter().map(|s| s.as_ref().to_string()).collect();
    match args.get(1).map(String::as_str) {
        Some("--version") | Some("version") => {
            writeln!(stdout, "{TOOL_NAME} {TOOL_VERSION}").map_err(|e| e.to_string())?;
            Ok(())
        }
        Some("meta") => {
            let root = args.get(2).ok_or("usage: terminus-walk meta <root>")?;
            let map = snapshot_meta(Path::new(root))?;
            write!(stdout, "{}", format_meta(&map)).map_err(|e| e.to_string())?;
            Ok(())
        }
        Some("hash") => {
            let root = args
                .get(2)
                .ok_or("usage: terminus-walk hash <root> [rels... | --list file]")?;
            let rels: Vec<String> = if args.get(3).map(String::as_str) == Some("--list") {
                let list_path = args
                    .get(4)
                    .ok_or("usage: terminus-walk hash <root> --list <file>")?;
                let buf = std::fs::read(list_path).map_err(|e| e.to_string())?;
                buf.split(|&b| b == 0 || b == b'\n')
                    .filter(|s| !s.is_empty())
                    .map(|s| String::from_utf8_lossy(s).into_owned())
                    .collect()
            } else if args.len() > 3 {
                args[3..].to_vec()
            } else {
                let mut buf = Vec::new();
                std::io::stdin()
                    .read_to_end(&mut buf)
                    .map_err(|e| e.to_string())?;
                buf.split(|&b| b == 0)
                    .filter(|s| !s.is_empty())
                    .map(|s| String::from_utf8_lossy(s).into_owned())
                    .collect()
            };
            let map = hash_paths(Path::new(root), &rels)?;
            write!(stdout, "{}", format_digests(&map)).map_err(|e| e.to_string())?;
            Ok(())
        }
        _ => Err(
            "usage: terminus-walk (--version | meta <root> | hash <root> [rels...])"
                .into(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn meta_roundtrip_and_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.txt");
        let nested = dir.path().join("sub");
        std::fs::create_dir(&nested).unwrap();
        let b = nested.join("b.txt");
        std::fs::write(&a, b"hi").unwrap();
        std::fs::write(&b, b"there").unwrap();
        // Touch mtimes into a known state when possible.
        let map = snapshot_meta(dir.path()).unwrap();
        assert_eq!(map.len(), 2);
        assert!(map.contains_key("a.txt"));
        assert!(map.contains_key("sub/b.txt"));
        let text = format_meta(&map);
        let parsed = parse_meta(&text).unwrap();
        assert_eq!(parsed, map);
    }

    #[test]
    fn hash_paths_parallel_stable() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("x.bin"), b"abcdef").unwrap();
        let map = hash_paths(dir.path(), &["x.bin".into()]).unwrap();
        let again = hash_paths(dir.path(), &["x.bin".into()]).unwrap();
        assert_eq!(map, again);
        let text = format_digests(&map);
        assert_eq!(parse_digests(&text).unwrap(), map);
    }

    /// Regression: many parallel hash workers must not use huge stack frames
    /// (remote Linux aborted with stack overflow when hashing thousands of files).
    #[test]
    fn hash_paths_many_parallel_files() {
        let dir = tempfile::tempdir().unwrap();
        let mut rels = Vec::new();
        for i in 0..256 {
            let name = format!("f{i}.bin");
            std::fs::write(dir.path().join(&name), format!("payload-{i}")).unwrap();
            rels.push(name);
        }
        let map = hash_paths(dir.path(), &rels).unwrap();
        assert_eq!(map.len(), 256);
    }
}
