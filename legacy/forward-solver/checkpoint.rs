//! Binary checkpoint format for solver state.
//!
//! Format:
//! - Header (32 bytes):
//!   - Magic: "GBL2" (4 bytes)
//!   - Version: u32 LE (4 bytes)
//!   - Entry count: u64 LE (8 bytes)
//!   - Checksum: u64 LE xxhash of data section (8 bytes)
//!   - Reserved: 8 bytes (zeros)
//! - Data section (entry_count × 9 bytes):
//!   - Canonical: u64 LE (8 bytes)
//!   - Outcome: i8 (1 byte)
//!
//! Entries are sorted by canonical for potential binary search.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::Path;

use xxhash_rust::xxh64::xxh64;

const MAGIC: &[u8; 4] = b"GBL2";
const VERSION: u32 = 1;
const HEADER_SIZE: usize = 32;
const ENTRY_SIZE: usize = 9;

pub struct Checkpoint {
    pub entries: Vec<(u64, i8)>,
}

impl Checkpoint {
    /// Save transposition table to binary checkpoint file.
    pub fn save(path: &Path, table: &HashMap<u64, i8>) -> io::Result<usize> {
        // Collect and sort entries
        let mut entries: Vec<(u64, i8)> = table.iter().map(|(&k, &v)| (k, v)).collect();
        entries.sort_by_key(|&(k, _)| k);

        let count = entries.len();

        // Build data section
        let mut data = Vec::with_capacity(count * ENTRY_SIZE);
        for (canonical, outcome) in &entries {
            data.extend_from_slice(&canonical.to_le_bytes());
            data.push(*outcome as u8);
        }

        // Compute checksum
        let checksum = xxh64(&data, 0);

        // Write file
        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);

        // Header
        writer.write_all(MAGIC)?;
        writer.write_all(&VERSION.to_le_bytes())?;
        writer.write_all(&(count as u64).to_le_bytes())?;
        writer.write_all(&checksum.to_le_bytes())?;
        writer.write_all(&[0u8; 8])?; // Reserved

        // Data
        writer.write_all(&data)?;
        writer.flush()?;

        Ok(count)
    }

    /// Load checkpoint from binary file.
    pub fn load(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);

        // Read header
        let mut header = [0u8; HEADER_SIZE];
        reader.read_exact(&mut header)?;

        // Validate magic
        if &header[0..4] != MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid checkpoint magic",
            ));
        }

        // Parse header
        let version = u32::from_le_bytes(header[4..8].try_into().unwrap());
        if version != VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Unsupported checkpoint version: {}", version),
            ));
        }

        let count = u64::from_le_bytes(header[8..16].try_into().unwrap()) as usize;
        let stored_checksum = u64::from_le_bytes(header[16..24].try_into().unwrap());

        // Read data section
        let mut data = vec![0u8; count * ENTRY_SIZE];
        reader.read_exact(&mut data)?;

        // Verify checksum
        let computed_checksum = xxh64(&data, 0);
        if computed_checksum != stored_checksum {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Checkpoint checksum mismatch",
            ));
        }

        // Parse entries
        let mut entries = Vec::with_capacity(count);
        for i in 0..count {
            let offset = i * ENTRY_SIZE;
            let canonical = u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap());
            let outcome = data[offset + 8] as i8;
            entries.push((canonical, outcome));
        }

        Ok(Checkpoint { entries })
    }

    /// Get file size estimate for a given number of entries.
    pub fn estimate_size(count: usize) -> usize {
        HEADER_SIZE + count * ENTRY_SIZE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_checkpoint_roundtrip() {
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join("test_checkpoint.bin");

        // Create test data
        let mut table = HashMap::new();
        table.insert(0u64, 1i8);
        table.insert(12345u64, -1i8);
        table.insert(999999u64, 0i8);

        // Save
        let saved = Checkpoint::save(&path, &table).unwrap();
        assert_eq!(saved, 3);

        // Load
        let loaded = Checkpoint::load(&path).unwrap();
        assert_eq!(loaded.entries.len(), 3);

        // Verify entries
        let loaded_map: HashMap<u64, i8> = loaded.entries.into_iter().collect();
        assert_eq!(loaded_map.get(&0), Some(&1));
        assert_eq!(loaded_map.get(&12345), Some(&-1));
        assert_eq!(loaded_map.get(&999999), Some(&0));

        // Cleanup
        std::fs::remove_file(&path).ok();
    }
}
