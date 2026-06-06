//! Count the full game tree (no transposition compression, 2-fold repetition terminal).
//!
//! This tool enumerates all paths in the game tree, counting each node.
//! - Transpositions are NOT compressed (same position via different paths = distinct nodes)
//! - 2-fold repetition (position appears twice on same path) = terminal draw
//! - Uses actual position hash (not canonical) for repetition detection

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use gobblet_core::{Board, Move};

/// Stack frame for iterative DFS.
#[derive(Clone)]
struct Frame {
    /// Board state at this node
    board: Board,
    /// Actual (non-canonical) position hash
    actual_hash: u64,
    /// All legal moves from this position
    moves: Vec<Move>,
    /// Index of next move to explore
    move_idx: usize,
}

/// Checkpoint state for resuming.
#[derive(Default)]
struct CounterState {
    /// Total nodes visited
    nodes: u64,
    /// Terminal nodes: wins (for either player)
    terminal_wins: u64,
    /// Terminal nodes: 2-fold repetition draws
    terminal_draws: u64,
    /// Terminal nodes: zugzwang (no legal moves)
    terminal_zugzwang: u64,
    /// Maximum depth reached
    max_depth: u64,
}

/// Statistics for logging.
struct Stats {
    start_time: Instant,
    last_log_time: Instant,
    last_log_nodes: u64,
}

impl Stats {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            start_time: now,
            last_log_time: now,
            last_log_nodes: 0,
        }
    }

    fn log_progress(&mut self, state: &CounterState, stack_len: usize, path_len: usize) {
        let now = Instant::now();
        let elapsed_total = self.start_time.elapsed().as_secs();
        let elapsed_since_log = self.last_log_time.elapsed().as_secs_f64();

        let rate = if elapsed_since_log > 0.0 {
            (state.nodes - self.last_log_nodes) as f64 / elapsed_since_log
        } else {
            0.0
        };

        let total_rate = if elapsed_total > 0 {
            state.nodes as f64 / elapsed_total as f64
        } else {
            0.0
        };

        // Estimate memory: stack + path
        let frame_size = std::mem::size_of::<Frame>();
        let stack_mem = stack_len * frame_size;
        let path_mem = path_len * 8; // u64 per entry
        let total_mem = stack_mem + path_mem;

        println!(
            "[{:02}:{:02}:{:02}] nodes={} rate={:.0}/s (avg={:.0}/s) depth={} max_depth={}",
            elapsed_total / 3600,
            (elapsed_total % 3600) / 60,
            elapsed_total % 60,
            state.nodes,
            rate,
            total_rate,
            stack_len,
            state.max_depth,
        );
        println!(
            "           terminals: wins={} draws={} zugzwang={} | mem~{}MB",
            state.terminal_wins,
            state.terminal_draws,
            state.terminal_zugzwang,
            total_mem / (1024 * 1024),
        );

        self.last_log_time = now;
        self.last_log_nodes = state.nodes;
    }
}

/// Save checkpoint to file.
fn save_checkpoint(
    path: &PathBuf,
    state: &CounterState,
    stack: &[Frame],
    path_set: &HashSet<u64>,
) -> std::io::Result<()> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);

    // Write state
    writer.write_all(&state.nodes.to_le_bytes())?;
    writer.write_all(&state.terminal_wins.to_le_bytes())?;
    writer.write_all(&state.terminal_draws.to_le_bytes())?;
    writer.write_all(&state.terminal_zugzwang.to_le_bytes())?;
    writer.write_all(&state.max_depth.to_le_bytes())?;

    // Write stack length
    writer.write_all(&(stack.len() as u64).to_le_bytes())?;

    // Write each frame
    for frame in stack {
        writer.write_all(&frame.board.to_u64().to_le_bytes())?;
        writer.write_all(&frame.actual_hash.to_le_bytes())?;
        writer.write_all(&(frame.moves.len() as u32).to_le_bytes())?;
        for mov in &frame.moves {
            writer.write_all(&[move_to_u8(*mov)])?;
        }
        writer.write_all(&(frame.move_idx as u32).to_le_bytes())?;
    }

    // Write path set
    writer.write_all(&(path_set.len() as u64).to_le_bytes())?;
    for &hash in path_set {
        writer.write_all(&hash.to_le_bytes())?;
    }

    writer.flush()?;
    Ok(())
}

/// Load checkpoint from file.
fn load_checkpoint(
    path: &PathBuf,
) -> std::io::Result<(CounterState, Vec<Frame>, HashSet<u64>)> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    let mut buf8 = [0u8; 8];
    let mut buf4 = [0u8; 4];

    // Read state
    reader.read_exact(&mut buf8)?;
    let nodes = u64::from_le_bytes(buf8);
    reader.read_exact(&mut buf8)?;
    let terminal_wins = u64::from_le_bytes(buf8);
    reader.read_exact(&mut buf8)?;
    let terminal_draws = u64::from_le_bytes(buf8);
    reader.read_exact(&mut buf8)?;
    let terminal_zugzwang = u64::from_le_bytes(buf8);
    reader.read_exact(&mut buf8)?;
    let max_depth = u64::from_le_bytes(buf8);

    let state = CounterState {
        nodes,
        terminal_wins,
        terminal_draws,
        terminal_zugzwang,
        max_depth,
    };

    // Read stack length
    reader.read_exact(&mut buf8)?;
    let stack_len = u64::from_le_bytes(buf8) as usize;

    // Read frames
    let mut stack = Vec::with_capacity(stack_len);
    for _ in 0..stack_len {
        reader.read_exact(&mut buf8)?;
        let board = Board::from_u64(u64::from_le_bytes(buf8));
        reader.read_exact(&mut buf8)?;
        let actual_hash = u64::from_le_bytes(buf8);
        reader.read_exact(&mut buf4)?;
        let moves_len = u32::from_le_bytes(buf4) as usize;
        let mut moves = Vec::with_capacity(moves_len);
        for _ in 0..moves_len {
            let mut buf1 = [0u8; 1];
            reader.read_exact(&mut buf1)?;
            moves.push(u8_to_move(buf1[0]));
        }
        reader.read_exact(&mut buf4)?;
        let move_idx = u32::from_le_bytes(buf4) as usize;

        stack.push(Frame {
            board,
            actual_hash,
            moves,
            move_idx,
        });
    }

    // Read path set
    reader.read_exact(&mut buf8)?;
    let path_len = u64::from_le_bytes(buf8) as usize;
    let mut path_set = HashSet::with_capacity(path_len);
    for _ in 0..path_len {
        reader.read_exact(&mut buf8)?;
        path_set.insert(u64::from_le_bytes(buf8));
    }

    Ok((state, stack, path_set))
}

/// Encode a Move as u8.
fn move_to_u8(mov: Move) -> u8 {
    match mov {
        Move::Place { size, to } => {
            let src = 9 + size as u8; // 9=Small, 10=Medium, 11=Large
            (src << 4) | to.0
        }
        Move::Slide { from, to } => (from.0 << 4) | to.0,
    }
}

/// Decode u8 to Move.
fn u8_to_move(byte: u8) -> Move {
    use gobblet_core::{Pos, Size};
    let src = byte >> 4;
    let dst = byte & 0x0F;
    if src >= 9 {
        let size = match src {
            9 => Size::Small,
            10 => Size::Medium,
            11 => Size::Large,
            _ => panic!("Invalid move encoding"),
        };
        Move::Place {
            size,
            to: Pos(dst),
        }
    } else {
        Move::Slide {
            from: Pos(src),
            to: Pos(dst),
        }
    }
}

fn main() {
    println!("Full Game Tree Counter");
    println!("======================");
    println!("Counting all paths (transpositions distinct, 2-fold = terminal draw)");
    println!();

    // Set up SIGINT handler
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        println!("\n\nInterrupt received, saving checkpoint...");
        r.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");

    // Configuration
    let checkpoint_path = PathBuf::from("data/tree_count.checkpoint");
    let checkpoint_interval_secs = 60;
    let log_interval_secs = 5;

    // Create data directory if needed
    if let Some(parent) = checkpoint_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    // Initialize or load from checkpoint
    let (mut state, mut stack, mut path): (CounterState, Vec<Frame>, HashSet<u64>) =
        if checkpoint_path.exists() {
            println!("Loading checkpoint from {:?}...", checkpoint_path);
            match load_checkpoint(&checkpoint_path) {
                Ok((s, st, p)) => {
                    println!(
                        "Resumed: {} nodes, stack depth {}, path size {}",
                        s.nodes,
                        st.len(),
                        p.len()
                    );
                    (s, st, p)
                }
                Err(e) => {
                    println!("Failed to load checkpoint: {}", e);
                    println!("Starting fresh.");
                    (CounterState::default(), Vec::new(), HashSet::new())
                }
            }
        } else {
            (CounterState::default(), Vec::new(), HashSet::new())
        };

    // If stack is empty, initialize with root
    if stack.is_empty() {
        let board = Board::new();
        let actual_hash = board.to_u64();
        let moves = board.legal_moves();

        stack.push(Frame {
            board,
            actual_hash,
            moves,
            move_idx: 0,
        });
        path.insert(actual_hash);
        state.nodes = 1;
        state.max_depth = 1;
    }

    let mut stats = Stats::new();
    let mut last_checkpoint = Instant::now();
    let mut last_log = Instant::now();

    println!("Starting enumeration...\n");

    while !stack.is_empty() {
        // Check for interrupt
        if !running.load(Ordering::SeqCst) {
            println!("\nSaving checkpoint before exit...");
            if let Err(e) = save_checkpoint(&checkpoint_path, &state, &stack, &path) {
                println!("Failed to save checkpoint: {}", e);
            } else {
                println!("Checkpoint saved.");
            }
            break;
        }

        // Periodic checkpoint
        if last_checkpoint.elapsed().as_secs() >= checkpoint_interval_secs {
            println!("\nSaving checkpoint...");
            let start = Instant::now();
            if let Err(e) = save_checkpoint(&checkpoint_path, &state, &stack, &path) {
                println!("Failed to save checkpoint: {}", e);
            } else {
                println!(
                    "Checkpoint saved in {:.2}s\n",
                    start.elapsed().as_secs_f64()
                );
            }
            last_checkpoint = Instant::now();
        }

        // Periodic logging
        if last_log.elapsed().as_secs() >= log_interval_secs {
            stats.log_progress(&state, stack.len(), path.len());
            last_log = Instant::now();
        }

        let frame = stack.last_mut().unwrap();

        if frame.move_idx < frame.moves.len() {
            let mov = frame.moves[frame.move_idx];
            frame.move_idx += 1;

            // Apply move
            let mut child_board = frame.board;
            child_board.apply(mov);
            let child_hash = child_board.to_u64();

            // Count this node
            state.nodes += 1;

            // Check for 2-fold repetition (terminal draw)
            if path.contains(&child_hash) {
                state.terminal_draws += 1;
                continue;
            }

            // Check for win (terminal)
            if child_board.check_winner().is_some() {
                state.terminal_wins += 1;
                continue;
            }

            // Generate moves for child
            let child_moves = child_board.legal_moves();

            // Check for zugzwang (no legal moves = loss for current player)
            if child_moves.is_empty() {
                state.terminal_zugzwang += 1;
                continue;
            }

            // Push child frame
            path.insert(child_hash);
            stack.push(Frame {
                board: child_board,
                actual_hash: child_hash,
                moves: child_moves,
                move_idx: 0,
            });
            state.max_depth = state.max_depth.max(stack.len() as u64);
        } else {
            // Pop frame
            let frame = stack.pop().unwrap();
            path.remove(&frame.actual_hash);
        }
    }

    // Final stats
    if running.load(Ordering::SeqCst) {
        println!("\n======================");
        println!("Enumeration complete!");
        println!("======================");
        println!("Total nodes: {}", state.nodes);
        println!("Terminal wins: {}", state.terminal_wins);
        println!("Terminal draws (2-fold): {}", state.terminal_draws);
        println!("Terminal zugzwang: {}", state.terminal_zugzwang);
        println!("Max depth: {}", state.max_depth);
        println!(
            "Total time: {:.1}s",
            stats.start_time.elapsed().as_secs_f64()
        );

        // Clean up checkpoint file on successful completion
        if checkpoint_path.exists() {
            std::fs::remove_file(&checkpoint_path).ok();
        }
    }
}
