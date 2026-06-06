//! Export binary checkpoint to SQLite database.
//!
//! Usage: export_sqlite [input.bin] [output.db]
//!
//! Converts the binary checkpoint format to a SQLite database for
//! efficient on-demand lookups in production.

use std::path::PathBuf;
use std::time::Instant;

use rusqlite::{Connection, params};
use gobblet_solver::checkpoint::Checkpoint;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let input_path = if args.len() > 1 {
        PathBuf::from(&args[1])
    } else {
        PathBuf::from("data/pruned.bin")
    };

    let output_path = if args.len() > 2 {
        PathBuf::from(&args[2])
    } else {
        PathBuf::from("data/tablebase.db")
    };

    println!("Binary to SQLite Exporter");
    println!("=========================");
    println!("Input:  {:?}", input_path);
    println!("Output: {:?}", output_path);
    println!();

    // Load binary checkpoint
    println!("Loading binary checkpoint...");
    let start = Instant::now();
    let checkpoint = match Checkpoint::load(&input_path) {
        Ok(cp) => cp,
        Err(e) => {
            eprintln!("Failed to load checkpoint: {}", e);
            std::process::exit(1);
        }
    };
    println!("Loaded {} positions in {:.2}s", checkpoint.entries.len(), start.elapsed().as_secs_f64());

    // Remove existing output file if present
    if output_path.exists() {
        std::fs::remove_file(&output_path).ok();
    }

    // Create SQLite database
    println!("\nCreating SQLite database...");
    let start = Instant::now();

    let conn = match Connection::open(&output_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to create database: {}", e);
            std::process::exit(1);
        }
    };

    // Create table
    conn.execute(
        "CREATE TABLE positions (
            canonical INTEGER PRIMARY KEY,
            outcome INTEGER NOT NULL
        )",
        [],
    ).expect("Failed to create table");

    // Insert entries in batches for performance
    println!("Inserting {} positions...", checkpoint.entries.len());

    let batch_size = 100_000;
    let total = checkpoint.entries.len();
    let mut inserted = 0;

    // Use a transaction for much faster inserts
    let tx = conn.unchecked_transaction().expect("Failed to start transaction");

    {
        let mut stmt = tx.prepare(
            "INSERT INTO positions (canonical, outcome) VALUES (?1, ?2)"
        ).expect("Failed to prepare statement");

        for (i, (canonical, outcome)) in checkpoint.entries.iter().enumerate() {
            stmt.execute(params![*canonical as i64, *outcome as i32])
                .expect("Failed to insert");

            inserted += 1;

            if (i + 1) % batch_size == 0 {
                let pct = 100.0 * inserted as f64 / total as f64;
                let elapsed = start.elapsed().as_secs_f64();
                let rate = inserted as f64 / elapsed;
                println!("  {:>3.0}% ({}/{}) - {:.0} rows/sec", pct, inserted, total, rate);
            }
        }
    }

    tx.commit().expect("Failed to commit transaction");

    let insert_time = start.elapsed().as_secs_f64();
    println!("Inserted {} positions in {:.2}s ({:.0} rows/sec)",
             inserted, insert_time, inserted as f64 / insert_time);

    // Create index (optional - PRIMARY KEY already creates one)
    // But let's verify the table is queryable
    println!("\nVerifying database...");

    // Check a few random lookups
    let test_positions: Vec<&(u64, i8)> = checkpoint.entries.iter()
        .step_by(checkpoint.entries.len() / 5)
        .take(5)
        .collect();

    for (canonical, expected_outcome) in test_positions {
        let result: i32 = conn.query_row(
            "SELECT outcome FROM positions WHERE canonical = ?1",
            params![*canonical as i64],
            |row| row.get(0),
        ).expect("Failed to query");

        assert_eq!(result, *expected_outcome as i32,
                   "Outcome mismatch for position {}", canonical);
    }
    println!("Verification passed!");

    // Report file sizes
    let input_size = std::fs::metadata(&input_path).map(|m| m.len()).unwrap_or(0);
    let output_size = std::fs::metadata(&output_path).map(|m| m.len()).unwrap_or(0);

    println!("\nFile sizes:");
    println!("  Binary: {:.1} MB", input_size as f64 / 1024.0 / 1024.0);
    println!("  SQLite: {:.1} MB", output_size as f64 / 1024.0 / 1024.0);

    println!("\nDone! Database created at {:?}", output_path);
}
