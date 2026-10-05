//! On-disk container for the Vulkan pipeline cache.
//!
//! The driver's blob is handed back on the next launch, and a damaged blob
//! (write cut short by a kill or suspend, a different GPU or driver, a file
//! shared with another build) can miscompile the text pipeline. Persisting
//! it raw means the damage survives every restart, so the blob is wrapped
//! in a header that pins the GPU and carries a length and checksum, and is
//! written through a temporary file so a reader never sees a partial one.

use std::io;
use std::path::Path;

const MAGIC: &[u8; 8] = b"TRMPCv1\0";
const HEADER_LEN: usize = 8 + 4 + 4 + 16 + 8 + 8;

/// Identity of the GPU/driver the blob was produced by
/// (`VkPhysicalDeviceProperties`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceId {
    pub vendor_id: u32,
    pub device_id: u32,
    pub cache_uuid: [u8; 16],
}

/// FNV-1a, 64 bit. Stable across Rust versions, unlike `DefaultHasher`.
fn checksum(data: &[u8]) -> u64 {
    data.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// Wraps the driver's `blob` for storage.
pub fn encode(blob: &[u8], id: &DeviceId) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + blob.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&id.vendor_id.to_le_bytes());
    out.extend_from_slice(&id.device_id.to_le_bytes());
    out.extend_from_slice(&id.cache_uuid);
    out.extend_from_slice(&(blob.len() as u64).to_le_bytes());
    out.extend_from_slice(&checksum(blob).to_le_bytes());
    out.extend_from_slice(blob);
    out
}

/// The driver blob inside `file`, or `None` when the file is not ours,
/// was written for another GPU/driver, or is truncated or corrupt.
pub fn decode<'a>(file: &'a [u8], id: &DeviceId) -> Option<&'a [u8]> {
    let header = file.get(..HEADER_LEN)?;
    if &header[..8] != MAGIC {
        return None;
    }
    let vendor_id = u32::from_le_bytes(header[8..12].try_into().ok()?);
    let device_id = u32::from_le_bytes(header[12..16].try_into().ok()?);
    let cache_uuid: [u8; 16] = header[16..32].try_into().ok()?;
    if (DeviceId {
        vendor_id,
        device_id,
        cache_uuid,
    }) != *id
    {
        return None;
    }
    let len =
        usize::try_from(u64::from_le_bytes(header[32..40].try_into().ok()?)).ok()?;
    let expected = u64::from_le_bytes(header[40..48].try_into().ok()?);
    let blob = file.get(HEADER_LEN..)?;
    if blob.len() != len || checksum(blob) != expected {
        return None;
    }
    Some(blob)
}

/// Writes `data` to `path` through a sibling temporary file and a rename,
/// so a concurrent or later reader sees the old file or the new one, never
/// a half-written one.
pub fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    let mut tmp_name = path.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(format!(".{}.tmp", std::process::id()));
    let tmp = path.with_file_name(tmp_name);
    let result = std::fs::write(&tmp, data).and_then(|()| std::fs::rename(&tmp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const GPU: DeviceId = DeviceId {
        vendor_id: 0x1002,
        device_id: 0x73bf,
        cache_uuid: [7; 16],
    };

    #[test]
    fn a_stored_blob_round_trips() {
        let file = encode(b"driver blob", &GPU);
        assert_eq!(decode(&file, &GPU), Some(&b"driver blob"[..]));
    }

    #[test]
    fn an_empty_blob_round_trips() {
        let file = encode(b"", &GPU);
        assert_eq!(decode(&file, &GPU), Some(&b""[..]));
    }

    #[test]
    fn a_truncated_file_is_rejected() {
        let file = encode(b"driver blob", &GPU);
        for cut in [0, 5, HEADER_LEN - 1, HEADER_LEN, file.len() - 1] {
            assert_eq!(decode(&file[..cut], &GPU), None, "cut at {cut}");
        }
    }

    #[test]
    fn a_flipped_byte_is_rejected() {
        let mut file = encode(b"driver blob", &GPU);
        *file.last_mut().unwrap() ^= 1;
        assert_eq!(decode(&file, &GPU), None);
    }

    #[test]
    fn trailing_garbage_is_rejected() {
        let mut file = encode(b"driver blob", &GPU);
        file.push(0);
        assert_eq!(decode(&file, &GPU), None);
    }

    #[test]
    fn another_gpu_or_driver_is_rejected() {
        let file = encode(b"driver blob", &GPU);
        for other in [
            DeviceId {
                vendor_id: 0x10de,
                ..GPU
            },
            DeviceId {
                device_id: 1,
                ..GPU
            },
            DeviceId {
                cache_uuid: [8; 16],
                ..GPU
            },
        ] {
            assert_eq!(decode(&file, &other), None);
        }
    }

    #[test]
    fn a_raw_driver_blob_from_the_old_format_is_rejected() {
        // What `rio` and earlier builds stored: the Vulkan header, no wrapper.
        let raw = [32u8, 0, 0, 0, 1, 0, 0, 0, 0xde, 0x10, 0, 0, 1, 0, 0, 0];
        assert_eq!(decode(&raw, &GPU), None);
        assert_eq!(decode(&[0u8; 200], &GPU), None);
    }

    #[test]
    fn write_atomic_replaces_the_file_and_leaves_no_temp() {
        let dir = std::env::temp_dir().join(format!("sl-pc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cache");
        write_atomic(&path, b"one").unwrap();
        write_atomic(&path, b"two").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
        let names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("cache")]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn write_atomic_into_a_missing_dir_fails_cleanly() {
        let path = std::env::temp_dir().join("sl-pc-missing-dir/x/cache");
        assert!(write_atomic(&path, b"x").is_err());
    }
}
