//! Streaming file hashes used across CLI and tool services.

#![deny(missing_docs)]

use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// SHA-256 of the file at `path`, formatted as lowercase hex.
pub fn sha256_file_hex(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|err| format!("open {}: {err}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|err| format!("read {}: {err}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write as _;

    #[test]
    fn matches_known_vector() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("payload");
        let mut file = fs::File::create(&path).unwrap();
        file.write_all(b"abc").unwrap();
        // SHA-256("abc")
        assert_eq!(
            sha256_file_hex(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
