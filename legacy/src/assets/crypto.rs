//! Asset cryptography and obfuscation engine for protecting game data and GoScript sources.

const ENGINE_SALT: &[u8] = b"GROOT_VFS_KEY_SALT_v1";

/// Default obfuscation key derived at compile time.
/// Developers can override this in production with `GROOT_ENCRYPTION_KEY` env var.
const DEFAULT_KEY: &[u8; 32] = b"Gr00t_3ng1n3_S3cur1ty_K3y_2025!_";

/// Derive an asset-specific keystream using a modified ChaCha-style rotating stream.
fn derive_keystream(key: &[u8], nonce: &[u8], len: usize) -> Vec<u8> {
    let mut state = [0u32; 16];

    // Setup state with constant, key, nonce, and salt
    state[0] = 0x61707865;
    state[1] = 0x3320646e;
    state[2] = 0x79622d32;
    state[3] = 0x6b206574;

    for (i, chunk) in key.chunks(4).take(8).enumerate() {
        let mut b = [0u8; 4];
        b[..chunk.len()].copy_from_slice(chunk);
        state[4 + i] = u32::from_le_bytes(b);
    }

    for (i, chunk) in nonce.chunks(4).take(4).enumerate() {
        let mut b = [0u8; 4];
        b[..chunk.len()].copy_from_slice(chunk);
        state[12 + i] = u32::from_le_bytes(b);
    }

    let mut stream = Vec::with_capacity(len);
    let mut counter = 0u32;

    while stream.len() < len {
        state[12] = state[12].wrapping_add(counter);
        let mut working = state;

        for _ in 0..10 {
            // Quarter-round rounds
            quarter_round(&mut working, 0, 4, 8, 12);
            quarter_round(&mut working, 1, 5, 9, 13);
            quarter_round(&mut working, 2, 6, 10, 14);
            quarter_round(&mut working, 3, 7, 11, 15);
            quarter_round(&mut working, 0, 5, 10, 15);
            quarter_round(&mut working, 1, 6, 11, 12);
            quarter_round(&mut working, 2, 7, 8, 13);
            quarter_round(&mut working, 3, 4, 9, 14);
        }

        for (w, s) in working.iter().zip(state.iter()) {
            let val = w.wrapping_add(*s);
            stream.extend_from_slice(&val.to_le_bytes());
        }

        counter = counter.wrapping_add(1);
    }

    stream.truncate(len);
    stream
}

#[inline(always)]
fn quarter_round(s: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    s[a] = s[a].wrapping_add(s[b]);
    s[d] ^= s[a];
    s[d] = s[d].rotate_left(16);

    s[c] = s[c].wrapping_add(s[d]);
    s[b] ^= s[c];
    s[b] = s[b].rotate_left(12);

    s[a] = s[a].wrapping_add(s[b]);
    s[d] ^= s[a];
    s[d] = s[d].rotate_left(8);

    s[c] = s[c].wrapping_add(s[d]);
    s[b] ^= s[c];
    s[b] = s[b].rotate_left(7);
}

/// Encrypts plaintext bytes using a salt-derived key and asset path nonce.
pub fn encrypt_asset(path: &str, data: &[u8]) -> Vec<u8> {
    let key = option_env!("GROOT_ENCRYPTION_KEY")
        .map(|k| k.as_bytes())
        .unwrap_or(DEFAULT_KEY);

    let mut nonce = Vec::new();
    nonce.extend_from_slice(ENGINE_SALT);
    nonce.extend_from_slice(path.as_bytes());

    let keystream = derive_keystream(key, &nonce, data.len());
    let mut encrypted = Vec::with_capacity(data.len() + 4);

    // 4-byte CRC header for verification
    let crc = calculate_checksum(data);
    encrypted.extend_from_slice(&crc.to_le_bytes());

    for (b, k) in data.iter().zip(keystream.iter()) {
        encrypted.push(b ^ k);
    }

    encrypted
}

/// Decrypts encrypted asset bytes. Returns `None` if empty or too short.
/// Falls back to returning the raw bytes if the CRC does not match
/// (i.e. the asset was never encrypted — e.g. in a debug build).
pub fn decrypt_asset(path: &str, encrypted: &[u8]) -> Option<Vec<u8>> {
    if encrypted.len() < 4 {
        return None;
    }

    let expected_crc = u32::from_le_bytes([
        encrypted[0],
        encrypted[1],
        encrypted[2],
        encrypted[3],
    ]);
    let cipher_payload = &encrypted[4..];

    let key = option_env!("GROOT_ENCRYPTION_KEY")
        .map(|k| k.as_bytes())
        .unwrap_or(DEFAULT_KEY);

    let mut nonce = Vec::new();
    nonce.extend_from_slice(ENGINE_SALT);
    nonce.extend_from_slice(path.as_bytes());

    let keystream = derive_keystream(key, &nonce, cipher_payload.len());
    let mut decrypted = Vec::with_capacity(cipher_payload.len());

    for (b, k) in cipher_payload.iter().zip(keystream.iter()) {
        decrypted.push(b ^ k);
    }

    let actual_crc = calculate_checksum(&decrypted);
    if actual_crc == expected_crc {
        Some(decrypted)
    } else {
        // Fallback: asset is not encrypted (e.g. debug build or unencrypted pak).
        // Return the raw bytes so the caller can still use them.
        Some(encrypted.to_vec())
    }
}

/// Computes a CRC32 checksum to detect tampered or corrupted payloads.
pub fn calculate_checksum(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in bytes {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// Strips comments and redundant whitespace from GoScript files prior to bundling.
pub fn minify_goscript(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut in_block_comment = false;

    for line in source.lines() {
        let trimmed = line.trim();
        if in_block_comment {
            if let Some(pos) = trimmed.find("*/") {
                in_block_comment = false;
                let rest = &trimmed[pos + 2..];
                if !rest.trim().is_empty() {
                    out.push_str(rest.trim());
                    out.push('\n');
                }
            }
            continue;
        }

        if trimmed.starts_with("/*") {
            if !trimmed.contains("*/") {
                in_block_comment = true;
            }
            continue;
        }

        if trimmed.starts_with("//") || trimmed.is_empty() {
            continue;
        }

        if let Some(idx) = line.find("//") {
            out.push_str(line[..idx].trim_end());
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }

    out
}
