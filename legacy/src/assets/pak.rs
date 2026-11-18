//! Binary Asset Package (.gpak) format and VFS archive builder.
//!
//! A `.gpak` file is a flat binary archive of encrypted asset entries.
//! Layout:
//!   4 bytes  – magic `GPAK`
//!   2 bytes  – version (u16 LE)
//!   4 bytes  – entry count (u32 LE)
//!   for each entry:
//!     2 bytes  – path length (u16 LE)
//!     N bytes  – UTF-8 relative path
//!     4 bytes  – data length (u32 LE)
//!     M bytes  – encrypted payload (see `crypto::encrypt_asset`)

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

pub const GPAK_MAGIC: &[u8; 4] = b"GPAK";
pub const GPAK_VERSION: u16 = 1;

pub struct GpakArchive {
    pub entries: HashMap<String, Vec<u8>>,
}

impl GpakArchive {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Pack an entire directory into an encrypted `.gpak` binary bundle.
    pub fn pack_directory<P: AsRef<Path>>(dir: P, output_path: P) -> std::io::Result<()> {
        let mut entries = HashMap::new();
        Self::collect_files(dir.as_ref(), dir.as_ref(), &mut entries)?;

        let mut file = File::create(output_path)?;
        file.write_all(GPAK_MAGIC)?;
        file.write_all(&GPAK_VERSION.to_le_bytes())?;

        let entry_count = entries.len() as u32;
        file.write_all(&entry_count.to_le_bytes())?;

        for (path, bytes) in &entries {
            let path_bytes = path.as_bytes();
            let path_len = path_bytes.len() as u16;
            file.write_all(&path_len.to_le_bytes())?;
            file.write_all(path_bytes)?;

            // Encrypt entry payload before writing.
            let encrypted = super::crypto::encrypt_asset(path, bytes);
            let data_len = encrypted.len() as u32;
            file.write_all(&data_len.to_le_bytes())?;
            file.write_all(&encrypted)?;
        }

        Ok(())
    }

    /// Read and decrypt an encrypted `.gpak` file into memory.
    pub fn open<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        let mut file = File::open(path)?;

        let mut magic = [0u8; 4];
        file.read_exact(&mut magic)?;
        if &magic != GPAK_MAGIC {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid GPAK magic header",
            ));
        }

        let mut ver_buf = [0u8; 2];
        file.read_exact(&mut ver_buf)?;
        // Version field reserved for future format changes.
        let _version = u16::from_le_bytes(ver_buf);

        let mut count_buf = [0u8; 4];
        file.read_exact(&mut count_buf)?;
        let count = u32::from_le_bytes(count_buf);

        let mut entries = HashMap::new();

        for _ in 0..count {
            let mut path_len_buf = [0u8; 2];
            file.read_exact(&mut path_len_buf)?;
            let path_len = u16::from_le_bytes(path_len_buf) as usize;

            let mut path_buf = vec![0u8; path_len];
            file.read_exact(&mut path_buf)?;
            let asset_path = String::from_utf8_lossy(&path_buf).to_string();

            let mut data_len_buf = [0u8; 4];
            file.read_exact(&mut data_len_buf)?;
            let data_len = u32::from_le_bytes(data_len_buf) as usize;

            let mut encrypted_data = vec![0u8; data_len];
            file.read_exact(&mut encrypted_data)?;

            if let Some(decrypted) = super::crypto::decrypt_asset(&asset_path, &encrypted_data) {
                entries.insert(asset_path, decrypted);
            }
        }

        Ok(Self { entries })
    }

    fn collect_files(
        root: &Path,
        current: &Path,
        out: &mut HashMap<String, Vec<u8>>,
    ) -> std::io::Result<()> {
        if current.is_dir() {
            for entry in std::fs::read_dir(current)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    Self::collect_files(root, &path, out)?;
                } else {
                    let rel_path = path
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/");

                    let mut bytes = std::fs::read(&path)?;

                    // Minify .gos scripts before packing.
                    if rel_path.ends_with(".gos") {
                        if let Ok(src) = std::str::from_utf8(&bytes) {
                            let minified = super::crypto::minify_goscript(src);
                            bytes = minified.into_bytes();
                        }
                    }

                    out.insert(rel_path, bytes);
                }
            }
        }
        Ok(())
    }
}

impl Default for GpakArchive {
    fn default() -> Self {
        Self::new()
    }
}
