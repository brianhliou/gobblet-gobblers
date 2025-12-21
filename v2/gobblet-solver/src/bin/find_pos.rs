use std::fs::File;
use std::io::{BufReader, Read};
use gobblet_core::Board;

fn main() {
    // Check what canonical 9 represents
    let board = Board::from_u64(9);
    println!("Canonical 9 decoded:");
    println!("  raw u64: {}", board.to_u64());
    println!("  canonical: {}", board.canonical());
    
    // Check if 9 is in our checkpoint
    let path = "data/checkpoint.bin";
    let file = File::open(path).unwrap();
    let mut reader = BufReader::new(file);
    
    let mut header = [0u8; 32];
    reader.read_exact(&mut header).unwrap();
    let count = u64::from_le_bytes(header[8..16].try_into().unwrap()) as usize;
    
    // Binary search for canonical 9 (file is sorted)
    println!("\nSearching for canonical 9 in V2 checkpoint...");
    
    // Read first 1000 entries to see what's there
    println!("\nFirst 20 V2 entries:");
    for i in 0..20 {
        let mut entry = [0u8; 9];
        reader.read_exact(&mut entry).unwrap();
        let canon = u64::from_le_bytes(entry[0..8].try_into().unwrap());
        let outcome = entry[8] as i8;
        println!("  {}: canonical={} outcome={}", i, canon, outcome);
    }
}
