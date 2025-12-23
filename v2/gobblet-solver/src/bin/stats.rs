//! Compute interesting statistics from the tablebase for the blog post.
//!
//! Usage: cargo run --release --bin stats

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::time::Instant;

use gobblet_core::{Board, Move, Player};
use gobblet_solver::movegen::MoveGenerator;

const WIN_P1: i8 = 1;
const DRAW: i8 = 0;
const WIN_P2: i8 = -1;

/// Load tablebase from binary file
fn load_tablebase(path: &Path) -> HashMap<u64, i8> {
    println!("Loading tablebase from {:?}...", path);
    let start = Instant::now();

    let file = File::open(path).expect("Failed to open tablebase");
    let mut reader = BufReader::new(file);
    let mut table = HashMap::new();

    let mut buf = [0u8; 9];
    while reader.read_exact(&mut buf).is_ok() {
        let canonical = u64::from_le_bytes(buf[0..8].try_into().unwrap());
        let outcome = buf[8] as i8;
        table.insert(canonical, outcome);
    }

    println!("Loaded {} positions in {:.2}s\n", table.len(), start.elapsed().as_secs_f64());
    table
}

/// Generate all legal moves for a position
fn generate_moves(board: &Board) -> Vec<Move> {
    let mut moves = Vec::new();
    let mut gen = MoveGenerator::new(board);
    while let Some(m) = gen.next(board) {
        moves.push(m);
    }
    moves
}

/// Compute outcome distribution
fn outcome_distribution(table: &HashMap<u64, i8>) {
    println!("=== Outcome Distribution ===");

    let mut p1_wins = 0u64;
    let mut p2_wins = 0u64;
    let mut draws = 0u64;

    for &outcome in table.values() {
        match outcome {
            WIN_P1 => p1_wins += 1,
            WIN_P2 => p2_wins += 1,
            DRAW => draws += 1,
            _ => {}
        }
    }

    let total = table.len() as f64;
    println!("P1 wins: {} ({:.2}%)", p1_wins, 100.0 * p1_wins as f64 / total);
    println!("P2 wins: {} ({:.2}%)", p2_wins, 100.0 * p2_wins as f64 / total);
    println!("Draws:   {} ({:.2}%)", draws, 100.0 * draws as f64 / total);
    println!("Total:   {}", table.len());
    println!();
}

/// Compute branching factor statistics
fn branching_factor_stats(table: &HashMap<u64, i8>) {
    println!("=== Branching Factor ===");

    // Sample positions and count legal moves
    let mut total_moves = 0u64;
    let mut count = 0u64;
    let mut max_moves = 0usize;
    let mut min_moves = usize::MAX;

    // Check initial position
    let initial = Board::new();
    let initial_moves = generate_moves(&initial);
    println!("Initial position: {} legal moves", initial_moves.len());

    // Sample from tablebase
    let sample_size = 10000;
    let step = table.len() / sample_size;

    for (i, (&canonical, _)) in table.iter().enumerate() {
        if i % step != 0 { continue; }

        // Reconstruct board from canonical (we can't directly - canonical loses info)
        // Instead, let's do BFS from initial position
        // Actually, we can't reconstruct arbitrary positions easily
        // Let's just report initial position stats and do random game sampling
        count += 1;
        if count >= sample_size as u64 { break; }
    }

    // Random game sampling for branching factor
    println!("\nSampling branching factor via random games...");
    let mut rng_state = 12345u64;
    let mut total_bf = 0u64;
    let mut bf_count = 0u64;

    for _ in 0..1000 {
        let mut board = Board::new();
        for _ in 0..50 {
            let moves = generate_moves(&board);
            if moves.is_empty() { break; }

            total_bf += moves.len() as u64;
            bf_count += 1;

            if moves.len() > max_moves { max_moves = moves.len(); }
            if moves.len() < min_moves { min_moves = moves.len(); }

            // Simple PRNG
            rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let idx = (rng_state >> 32) as usize % moves.len();

            let undo = board.apply(moves[idx]);
            if board.check_winner().is_some() {
                board.undo(&undo);
                break;
            }
        }
    }

    println!("Average branching factor: {:.1}", total_bf as f64 / bf_count as f64);
    println!("Max branching factor: {}", max_moves);
    println!("Min branching factor: {}", min_moves);
    println!();
}

/// Compute depths iteratively using the tablebase directly
fn compute_all_depths(table: &HashMap<u64, i8>) -> HashMap<u64, u32> {
    println!("  Step 1: Initializing depths...");
    let start = std::time::Instant::now();

    let mut depths: HashMap<u64, u32> = HashMap::with_capacity(table.len());

    // Initialize all positions
    for (&canonical, &outcome) in table {
        let board = Board::from_u64(canonical);

        // Terminal: check_winner returns Some
        if let Some(winner) = board.check_winner() {
            depths.insert(canonical, if winner == Player::One { 0 } else { u32::MAX });
            continue;
        }

        // Non-P1-winning positions: infinite depth (P1 doesn't win)
        if outcome != WIN_P1 {
            depths.insert(canonical, u32::MAX);
        }
    }
    println!("    Initialized {} terminal/non-P1-win positions in {:.1}s",
             depths.len(), start.elapsed().as_secs_f64());

    // Step 2: Iteratively propagate depths for P1-winning positions
    println!("  Step 2: Propagating depths for P1-winning positions...");
    let p1_win_count = table.values().filter(|&&v| v == WIN_P1).count();
    println!("    {} P1-winning positions to solve", p1_win_count);

    let mut iteration = 0;
    loop {
        iteration += 1;
        let mut changed = 0u32;

        for (&canonical, &outcome) in table {
            // Only process unsolved P1-winning positions
            if outcome != WIN_P1 || depths.contains_key(&canonical) {
                continue;
            }

            let board = Board::from_u64(canonical);
            let is_p1 = board.current_player() == Player::One;
            let moves = generate_moves(&board);

            let mut best_depth = if is_p1 { u32::MAX } else { 0u32 };
            let mut can_solve = if is_p1 { false } else { true };

            for m in &moves {
                let mut child = board.clone();
                child.apply(*m);

                // First check if child is terminal (might not be in tablebase)
                let child_depth_opt = if let Some(winner) = child.check_winner() {
                    Some(if winner == Player::One { 0 } else { u32::MAX })
                } else {
                    let child_canonical = child.canonical();
                    depths.get(&child_canonical).copied()
                };

                match child_depth_opt {
                    Some(d) if d != u32::MAX => {
                        // Child has finite depth (P1 eventually wins from there)
                        let child_depth = d + 1;
                        if is_p1 {
                            best_depth = best_depth.min(child_depth);
                            can_solve = true; // P1 found at least one winning path
                        } else {
                            best_depth = best_depth.max(child_depth);
                        }
                    }
                    Some(_) => {
                        // Child has MAX depth - P1 doesn't win from there
                        // For P1: skip this move (it's a losing move)
                        // For P2: this shouldn't happen in a P1-winning position
                    }
                    None => {
                        // Child not yet solved
                        if !is_p1 {
                            can_solve = false; // P2 must wait for all children
                        }
                    }
                }
            }

            if can_solve && best_depth != u32::MAX {
                depths.insert(canonical, best_depth);
                changed += 1;
            }
        }

        if iteration % 5 == 0 || changed == 0 {
            println!("    Iteration {}: +{} solved, {}/{} total",
                     iteration, changed, depths.len(), table.len());
        }

        if changed == 0 {
            break;
        }
    }

    println!("  Done in {} iterations, {:.1}s",
             iteration, start.elapsed().as_secs_f64());

    // Debug: why are some P1-winning positions unsolved?
    let unsolved_p1: Vec<_> = table.iter()
        .filter(|(&c, &o)| o == WIN_P1 && !depths.contains_key(&c))
        .take(5)
        .collect();

    if !unsolved_p1.is_empty() {
        println!("  Debug: {} P1-winning positions unsolved. Samples:",
                 table.iter().filter(|(&c, &o)| o == WIN_P1 && !depths.contains_key(&c)).count());
        for (&canonical, _) in &unsolved_p1 {
            let board = Board::from_u64(canonical);
            let moves = generate_moves(&board);
            let is_p1 = board.current_player() == Player::One;
            println!("    Position {} (P{} to move, {} moves):", canonical, if is_p1 { 1 } else { 2 }, moves.len());
            for m in moves.iter().take(3) {
                let mut child = board.clone();
                child.apply(*m);
                let cc = child.canonical();
                match depths.get(&cc) {
                    Some(&d) => println!("      {:?} -> depth {}", m, d),
                    None => {
                        if table.contains_key(&cc) {
                            println!("      {:?} -> in table but no depth", m);
                        } else {
                            println!("      {:?} -> NOT in tablebase", m);
                        }
                    }
                }
            }
        }
    }

    depths
}


/// Iteratively compute distance-to-mate for all P1-winning positions
/// Returns a map from canonical position to (distance, best_move)
fn compute_distances(table: &HashMap<u64, i8>) -> HashMap<u64, (u32, Option<Move>)> {
    println!("Computing distance-to-mate for all P1-winning positions...");
    let start = Instant::now();

    let mut distances: HashMap<u64, (u32, Option<Move>)> = HashMap::new();

    // Step 1: Initialize terminal positions (P1 wins immediately)
    // These are positions where P1 has 3 in a row
    let mut initialized = 0u64;
    for &canonical in table.keys() {
        let board = Board::from_u64(canonical);
        if let Some(Player::One) = board.check_winner() {
            distances.insert(canonical, (0, None));
            initialized += 1;
        }
    }
    println!("  Initialized {} terminal P1-win positions", initialized);

    // Also find P1 wins via zugzwang (P2 has no moves)
    for (&canonical, &outcome) in table {
        if outcome != WIN_P1 { continue; }
        if distances.contains_key(&canonical) { continue; }

        let board = Board::from_u64(canonical);
        if board.current_player() == Player::Two {
            let moves = generate_moves(&board);
            if moves.is_empty() {
                distances.insert(canonical, (0, None));
                initialized += 1;
            }
        }
    }
    println!("  Plus {} zugzwang positions, total base: {}", distances.len() - initialized as usize, distances.len());

    // Step 2: Iteratively propagate distances
    // - For P1-turn positions: dist = 1 + min(dist of winning children)
    // - For P2-turn positions: dist = 1 + max(dist of winning children)
    let mut iteration = 0u32;
    loop {
        iteration += 1;
        let mut changed = 0u64;

        for (&canonical, &outcome) in table {
            // Only process unsolved P1-winning positions
            if outcome != WIN_P1 || distances.contains_key(&canonical) {
                continue;
            }

            let board = Board::from_u64(canonical);
            let is_p1 = board.current_player() == Player::One;
            let moves = generate_moves(&board);

            if is_p1 {
                // P1 picks min distance among winning children
                let mut best_dist = u32::MAX;
                let mut best_move = None;
                for m in &moves {
                    let mut child = board.clone();
                    child.apply(*m);

                    // Check child's distance
                    let child_dist = if let Some(Player::One) = child.check_winner() {
                        Some(0)
                    } else {
                        distances.get(&child.canonical()).map(|&(d, _)| d)
                    };

                    if let Some(d) = child_dist {
                        if d + 1 < best_dist {
                            best_dist = d + 1;
                            best_move = Some(*m);
                        }
                    }
                }
                if best_dist != u32::MAX {
                    distances.insert(canonical, (best_dist, best_move));
                    changed += 1;
                }
            } else {
                // P2 picks max distance among P1-winning children (to delay)
                // P2 must consider ALL children since any move leads to P1 win (position is P1-winning)
                let mut all_children_have_dist = true;
                let mut max_dist = 0u32;
                let mut max_move = None;

                for m in &moves {
                    let mut child = board.clone();
                    child.apply(*m);

                    // Is child a P1-winning position?
                    let child_is_p1_win = if let Some(Player::One) = child.check_winner() {
                        true
                    } else {
                        table.get(&child.canonical()) == Some(&WIN_P1)
                    };

                    if child_is_p1_win {
                        let child_dist = if let Some(Player::One) = child.check_winner() {
                            Some(0)
                        } else {
                            distances.get(&child.canonical()).map(|&(d, _)| d)
                        };

                        match child_dist {
                            Some(d) => {
                                if d + 1 > max_dist {
                                    max_dist = d + 1;
                                    max_move = Some(*m);
                                }
                            }
                            None => {
                                all_children_have_dist = false;
                            }
                        }
                    }
                    // Non-P1-winning children: P2 wouldn't choose these (they lead to P2 win or draw)
                }

                // Only set distance if all P1-winning children have been computed
                if all_children_have_dist && max_dist > 0 {
                    distances.insert(canonical, (max_dist, max_move));
                    changed += 1;
                }
            }
        }

        if iteration % 10 == 0 || changed == 0 {
            println!("  Iteration {}: +{} solved, {}/{} total ({:.1}s)",
                     iteration, changed, distances.len(), table.len(), start.elapsed().as_secs_f64());
        }

        if changed == 0 {
            break;
        }
    }

    println!("  Done in {} iterations, {:.1}s", iteration, start.elapsed().as_secs_f64());

    // Count unsolved P1-winning positions and run additional iterations for them
    let unsolved_count_before = table.iter()
        .filter(|(&c, &o)| o == WIN_P1 && !distances.contains_key(&c))
        .count();

    if unsolved_count_before > 0 {
        println!("  Warning: {} P1-winning positions unsolved, running fallback loop...", unsolved_count_before);

        // Run fallback iterations until no more progress
        let mut fallback_iter = 0u32;
        loop {
            fallback_iter += 1;
            let mut total_solved = 0u64;

            // For unsolved P2 positions, use max among available children
            for (&canonical, &outcome) in table.iter() {
                if outcome != WIN_P1 || distances.contains_key(&canonical) {
                    continue;
                }
                let board = Board::from_u64(canonical);
                if board.current_player() != Player::Two {
                    continue;
                }

                let moves = generate_moves(&board);
                let mut max_dist = 0u32;
                let mut max_move = None;
                let mut has_any_p1_win = false;

                for m in &moves {
                    let mut child = board.clone();
                    child.apply(*m);

                    let child_is_p1_win = if let Some(Player::One) = child.check_winner() {
                        true
                    } else {
                        table.get(&child.canonical()) == Some(&WIN_P1)
                    };

                    if child_is_p1_win {
                        has_any_p1_win = true;
                        let child_dist = if let Some(Player::One) = child.check_winner() {
                            Some(0)
                        } else {
                            distances.get(&child.canonical()).map(|&(d, _)| d)
                        };

                        if let Some(d) = child_dist {
                            if d + 1 > max_dist {
                                max_dist = d + 1;
                                max_move = Some(*m);
                            }
                        }
                    }
                }

                if has_any_p1_win && max_dist > 0 {
                    distances.insert(canonical, (max_dist, max_move));
                    total_solved += 1;
                }
            }

            // For unsolved P1 positions, use min among available children
            for (&canonical, &outcome) in table.iter() {
                if outcome != WIN_P1 || distances.contains_key(&canonical) {
                    continue;
                }
                let board = Board::from_u64(canonical);
                if board.current_player() != Player::One {
                    continue;
                }

                let moves = generate_moves(&board);
                let mut best_dist = u32::MAX;
                let mut best_move = None;

                for m in &moves {
                    let mut child = board.clone();
                    child.apply(*m);

                    let child_dist = if let Some(Player::One) = child.check_winner() {
                        Some(0)
                    } else {
                        distances.get(&child.canonical()).map(|&(d, _)| d)
                    };

                    if let Some(d) = child_dist {
                        if d + 1 < best_dist {
                            best_dist = d + 1;
                            best_move = Some(*m);
                        }
                    }
                }

                if best_dist != u32::MAX {
                    distances.insert(canonical, (best_dist, best_move));
                    total_solved += 1;
                }
            }

            println!("  Fallback iter {}: solved {} more", fallback_iter, total_solved);
            if total_solved == 0 {
                break;
            }
        }
    }

    let final_unsolved = table.iter()
        .filter(|(&c, &o)| o == WIN_P1 && !distances.contains_key(&c))
        .count();
    println!("  Final unsolved: {}", final_unsolved);

    // Debug: check the 1-piece position specifically
    let one_piece_canonical = 18014398509481985u64;  // S(0,0) canonical from earlier
    if let Some(&outcome) = table.get(&one_piece_canonical) {
        let has_dist = distances.get(&one_piece_canonical);
        println!("\n  1-piece position (canonical {}): outcome={}, dist={:?}", one_piece_canonical, outcome, has_dist);
        if has_dist.is_none() {
            // Debug why it wasn't solved
            let board = Board::from_u64(one_piece_canonical);
            let is_p1 = board.current_player() == Player::One;
            let moves = generate_moves(&board);
            println!("    P{} to move, {} moves:", if is_p1 { 1 } else { 2 }, moves.len());
            let mut missing_dist = 0;
            let mut p1_win_count = 0;
            for m in moves.iter() {
                let mut child = board.clone();
                child.apply(*m);
                let child_canonical = child.canonical();
                let in_table = table.get(&child_canonical);
                let has_dist = distances.get(&child_canonical);
                let is_win = child.check_winner() == Some(Player::One);
                let is_p1_win = is_win || in_table == Some(&WIN_P1);
                if is_p1_win {
                    p1_win_count += 1;
                    if has_dist.is_none() && !is_win {
                        missing_dist += 1;
                        println!("      MISSING: {:?} -> in_table={:?}, dist={:?}", m, in_table, has_dist);
                    }
                }
            }
            println!("    {} P1-winning children, {} missing distances", p1_win_count, missing_dist);
        }
    } else {
        println!("\n  1-piece position (canonical {}) NOT in tablebase!", one_piece_canonical);
    }

    distances
}

/// Compute optimal game lengths and trace the optimal resistance line
fn optimal_game_analysis(table: &HashMap<u64, i8>) {
    println!("=== Optimal Game Analysis ===");
    println!("Finding longest P1 win from initial position...\n");
    println!("P1 plays to win fast (min depth), P2 plays to delay (max depth)\n");

    // Use iterative distance computation
    let distances = compute_distances(table);

    // Find max distance (optimal resistance)
    let max_dist = distances.values().map(|&(d, _)| d).max().unwrap_or(0);
    println!("\nMax distance-to-mate across all positions: {}", max_dist);

    // Check initial position
    let initial = Board::new();
    let initial_canonical = initial.canonical();

    // Initial position isn't in tablebase (it's the starting position)
    // We need to compute distance for initial by looking at its children
    let initial_moves = generate_moves(&initial);
    let mut initial_dist = u32::MAX;
    let mut initial_best_move = None;

    for m in &initial_moves {
        let mut child = initial.clone();
        child.apply(*m);
        let child_canonical = child.canonical();

        if let Some(&(d, _)) = distances.get(&child_canonical) {
            if d + 1 < initial_dist {
                initial_dist = d + 1;
                initial_best_move = Some(*m);
            }
        }
    }

    println!("Distance from initial position (P1 plays optimally): {}", initial_dist);
    println!("\nOptimal resistance (longest P1 win): {} moves", initial_dist);

    // Now trace the path using distance values to find optimal moves
    println!("\n=== Tracing Optimal Resistance Line ===");
    let mut board = Board::new();
    let mut move_history: Vec<String> = Vec::new();
    let mut current_dist = initial_dist;

    loop {
        if let Some(winner) = board.check_winner() {
            println!("\n*** P{} wins after {} moves! ***",
                     if winner == Player::One { 1 } else { 2 }, move_history.len());
            break;
        }

        let moves = generate_moves(&board);
        if moves.is_empty() {
            let loser = board.current_player();
            println!("\n*** No moves - P{} loses after {} moves! ***",
                     if loser == Player::One { 1 } else { 2 }, move_history.len());
            break;
        }

        let is_p1 = board.current_player() == Player::One;

        // Find optimal move by checking children
        let mut chosen = None;
        let mut chosen_dist = if is_p1 { u32::MAX } else { 0 };

        for m in &moves {
            let mut child = board.clone();
            child.apply(*m);

            let child_dist = if let Some(Player::One) = child.check_winner() {
                Some(0)
            } else {
                distances.get(&child.canonical()).map(|&(d, _)| d)
            };

            if let Some(d) = child_dist {
                if is_p1 {
                    // P1 picks minimum
                    if d < chosen_dist {
                        chosen_dist = d;
                        chosen = Some(*m);
                    }
                } else {
                    // P2 picks maximum (to delay)
                    if d > chosen_dist {
                        chosen_dist = d;
                        chosen = Some(*m);
                    }
                }
            }
        }

        if chosen.is_none() {
            println!("No valid move found at move {}!", move_history.len() + 1);
            println!("Current position canonical: {}", board.canonical());
            println!("Available children:");
            for m in &moves {
                let mut child = board.clone();
                child.apply(*m);
                let child_dist = if let Some(Player::One) = child.check_winner() {
                    Some(0)
                } else {
                    distances.get(&child.canonical()).map(|&(d, _)| d)
                };
                println!("  {:?} -> dist: {:?}", m, child_dist);
            }
            break;
        }

        let the_move = chosen.unwrap();
        let notation = format_move(&board, the_move);
        let player = if is_p1 { "P1" } else { "P2" };
        println!("{}. {} {} (dist: {} -> {})",
                 move_history.len() + 1, player, notation, current_dist, chosen_dist);

        move_history.push(notation);
        board.apply(the_move);
        current_dist = chosen_dist;

        if move_history.len() > 200 {
            println!("\n*** Safety limit (200 moves) ***");
            break;
        }
    }

    println!("\n=== Summary ===");
    println!("Game length: {} moves", move_history.len());
    println!("Move sequence:");
    println!("{}", move_history.join(" "));
    println!();
}

/// Format a move as notation string
fn format_move(_board: &Board, m: Move) -> String {
    match m {
        Move::Place { size, to } => {
            let size_char = match size {
                gobblet_core::Size::Small => 'S',
                gobblet_core::Size::Medium => 'M',
                gobblet_core::Size::Large => 'L',
            };
            let to_idx = to.0;
            format!("{}({},{})", size_char, to_idx / 3, to_idx % 3)
        }
        Move::Slide { from, to } => {
            let from_idx = from.0;
            let to_idx = to.0;
            format!("({},{})->({},{})", from_idx / 3, from_idx % 3, to_idx / 3, to_idx % 3)
        }
    }
}

fn main() {
    let tablebase_path = Path::new("../frontend-wasm/api/tablebase.bin");

    // Load tablebase
    let table = load_tablebase(tablebase_path);

    // Compute statistics
    outcome_distribution(&table);
    branching_factor_stats(&table);

    // Run optimal game analysis (now uses iterative approach, no huge stack needed)
    optimal_game_analysis(&table);
}
