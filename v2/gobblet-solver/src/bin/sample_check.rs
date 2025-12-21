use gobblet_core::Board;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};

fn main() {
    let path = "data/checkpoint.bin";
    let file = File::open(path).expect("Cannot open checkpoint");
    let mut reader = BufReader::new(file);
    
    // Read header
    let mut header = [0u8; 32];
    reader.read_exact(&mut header).unwrap();
    let count = u64::from_le_bytes(header[8..16].try_into().unwrap()) as usize;
    
    println!("Checking if stored positions are canonical...\n");
    
    // Sample positions at various offsets
    let samples = [0, 1000, 10000, 100000, 1000000, 50000000, 100000000, 150000000];
    let mut non_canonical = 0;
    
    for &idx in &samples {
        if idx >= count {
            continue;
        }
        
        // Seek to position
        let offset = 32 + (idx as u64) * 9;
        reader.seek(SeekFrom::Start(offset)).unwrap();
        
        let mut entry = [0u8; 9];
        reader.read_exact(&mut entry).unwrap();
        
        let stored_canonical = u64::from_le_bytes(entry[0..8].try_into().unwrap());
        let outcome = entry[8] as i8;
        
        // Check if it's actually canonical
        let board = Board::from_u64(stored_canonical);
        let computed_canonical = board.canonical();
        
        let is_canonical = stored_canonical == computed_canonical;
        if !is_canonical {
            non_canonical += 1;
        }
        
        println!("Entry {}: stored={:016x} computed_canonical={:016x} outcome={} {}",
            idx, stored_canonical, computed_canonical, outcome,
            if is_canonical { "✓" } else { "NOT CANONICAL!" });
    }
    
    println!("\nNon-canonical entries found in sample: {}", non_canonical);
    
    // Do a broader check on first 10000 entries
    println!("\n--- Checking first 10000 entries ---");
    reader.seek(SeekFrom::Start(32)).unwrap();
    let mut non_canon_count = 0;
    for i in 0..10000.min(count) {
        let mut entry = [0u8; 9];
        reader.read_exact(&mut entry).unwrap();
        let stored = u64::from_le_bytes(entry[0..8].try_into().unwrap());
        let board = Board::from_u64(stored);
        if stored != board.canonical() {
            non_canon_count += 1;
            if non_canon_count <= 5 {
                println!("  Entry {}: stored={:016x} != canonical={:016x}", 
                    i, stored, board.canonical());
            }
        }
    }
    println!("Non-canonical in first 10000: {}", non_canon_count);
}
