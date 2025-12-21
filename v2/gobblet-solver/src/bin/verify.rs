use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};

fn main() {
    let path = "data/checkpoint.bin";
    let file = File::open(path).expect("Cannot open checkpoint");
    let mut reader = BufReader::new(file);
    
    // Read header
    let mut header = [0u8; 32];
    reader.read_exact(&mut header).expect("Cannot read header");
    
    let count = u64::from_le_bytes(header[8..16].try_into().unwrap()) as usize;
    println!("Checkpoint has {} positions", count);
    
    // Check initial position (Board::new().canonical())
    println!("\n--- Loading all entries ---");
    
    let mut all_data = vec![0u8; count * 9];
    reader.read_exact(&mut all_data).unwrap();
    
    // Build hashmap
    let mut table: HashMap<u64, i8> = HashMap::new();
    for i in 0..count {
        let offset = i * 9;
        let canonical = u64::from_le_bytes(all_data[offset..offset+8].try_into().unwrap());
        let outcome = all_data[offset + 8] as i8;
        table.insert(canonical, outcome);
    }
    println!("Loaded {} unique entries", table.len());
    
    // Check canonical 0 (initial position - empty board, P1 to move)
    println!("\n--- Checking initial position ---");
    if let Some(&outcome) = table.get(&0) {
        let outcome_str = match outcome {
            1 => "P1 wins",
            0 => "Draw", 
            -1 => "P2 wins",
            _ => "Unknown",
        };
        println!("Initial position (canonical=0): {}", outcome_str);
    } else {
        println!("Initial position NOT FOUND in table!");
    }
    
    // Count outcomes
    let mut p1_wins = 0usize;
    let mut draws = 0usize;
    let mut p2_wins = 0usize;
    for &outcome in table.values() {
        match outcome {
            1 => p1_wins += 1,
            0 => draws += 1,
            -1 => p2_wins += 1,
            _ => {}
        }
    }
    println!("\nOutcome distribution:");
    println!("  P1 wins: {} ({:.1}%)", p1_wins, 100.0 * p1_wins as f64 / count as f64);
    println!("  Draws:   {} ({:.1}%)", draws, 100.0 * draws as f64 / count as f64);
    println!("  P2 wins: {} ({:.1}%)", p2_wins, 100.0 * p2_wins as f64 / count as f64);
}
