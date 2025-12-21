//! Gobblet Gobblers Solver
//!
//! Solves the game using iterative minimax with optional alpha-beta pruning.

mod checkpoint;
mod movegen;
mod solver;
mod stats;

use std::env;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use gobblet_core::Board;

use crate::checkpoint::Checkpoint;
use crate::solver::Solver;

fn main() {
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    let prune = !args.contains(&"--no-prune".to_string());

    println!("Gobblet Gobblers Solver v2");
    println!("==========================");
    println!("Mode: {}", if prune { "Alpha-beta pruning" } else { "Full solve (no pruning)" });
    println!();

    // Set up SIGINT handler for graceful shutdown
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        println!("\n\nInterrupt received, saving checkpoint...");
        r.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");

    // Configuration - use different files for pruned vs full solve
    let checkpoint_path = if prune {
        PathBuf::from("data/pruned.bin")
    } else {
        PathBuf::from("data/full.bin")
    };
    let checkpoint_interval_secs = 60;
    let log_interval_secs = 5; // More frequent logging

    // Create data directory if needed
    if let Some(parent) = checkpoint_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    // Initialize solver
    let mut solver = Solver::new();

    // Load existing checkpoint if present
    if checkpoint_path.exists() {
        println!("Loading checkpoint from {:?}...", checkpoint_path);
        let start = Instant::now();
        match Checkpoint::load(&checkpoint_path) {
            Ok(checkpoint) => {
                let count = checkpoint.entries.len();
                for (canonical, outcome) in checkpoint.entries {
                    solver.table.insert(canonical, outcome);
                }
                println!(
                    "Loaded {} positions in {:.2}s\n",
                    count,
                    start.elapsed().as_secs_f64()
                );
            }
            Err(e) => {
                println!("Warning: Failed to load checkpoint: {}", e);
                println!("Starting fresh.\n");
            }
        }
    }

    // Solve from initial position
    let board = Board::new();
    println!("Starting solve from initial position...");
    println!("Checkpoint interval: {}s", checkpoint_interval_secs);
    println!("Log interval: {}s\n", log_interval_secs);

    let start = Instant::now();
    let outcome = solver.solve(
        board,
        prune,
        running.clone(),
        checkpoint_interval_secs,
        log_interval_secs,
        &checkpoint_path,
    );

    let elapsed = start.elapsed();

    // Final stats
    println!("\n==========================");
    println!("Solve complete!");
    println!("==========================");
    println!("Result: {:?}", outcome);
    println!("Time: {:.2}s", elapsed.as_secs_f64());
    println!();
    solver.stats.print_summary();

    // Save final checkpoint
    println!("\nSaving final checkpoint...");
    let save_start = Instant::now();
    match Checkpoint::save(&checkpoint_path, &solver.table) {
        Ok(count) => {
            println!(
                "Saved {} positions in {:.2}s",
                count,
                save_start.elapsed().as_secs_f64()
            );
        }
        Err(e) => {
            println!("Error saving checkpoint: {}", e);
        }
    }

    // Interpret result
    match outcome {
        Some(1) => println!("\nPlayer 1 wins with optimal play!"),
        Some(-1) => println!("\nPlayer 2 wins with optimal play!"),
        Some(0) => println!("\nGame is a draw with optimal play."),
        None => println!("\nSolve was interrupted before completion."),
        _ => println!("\nUnexpected outcome: {:?}", outcome),
    }
}
