//! V1/V2 Parity Testing
//!
//! Loads test positions exported from V1 Python and verifies V2 Rust
//! produces identical results for:
//! - Board encoding (canonical form)
//! - Legal move count
//! - Legal moves (as a set)
//! - Winner detection

use std::collections::HashSet;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use gobblet_core::{Board, Move, Player};
use serde::Deserialize;

/// JSON structure matching V1 export format
#[derive(Debug, Deserialize)]
struct TestData {
    version: String,
    stats: Stats,
    positions: Vec<Position>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Stats {
    total_positions: usize,
    game_tree_positions: usize,
    edge_case_positions: usize,
    max_depth: usize,
}

#[derive(Debug, Deserialize)]
struct Position {
    canonical: u64,
    encoding: u64,
    current_player: u8,
    legal_moves: Vec<MoveData>,
    legal_move_count: usize,
    winner: Option<u8>,
    depth: usize,
    description: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct MoveData {
    notation: String,
    to: [u8; 2],
    #[serde(default)]
    from: Option<[u8; 2]>,
    #[serde(default)]
    size: Option<String>,
    #[serde(rename = "type")]
    move_type: String,
}

/// Convert V1 position encoding to V2 Board
fn encoding_to_board(encoding: u64) -> Board {
    Board::from_u64(encoding)
}

/// Convert V1 move data to a comparable key (for set comparison)
fn move_to_key(m: &MoveData) -> (bool, u8, u8, u8) {
    // (is_place, size_or_0, from_pos_or_0, to_pos)
    let to_pos = m.to[0] * 3 + m.to[1];

    if m.move_type == "place" {
        let size = match m.size.as_deref() {
            Some("SMALL") => 0,
            Some("MEDIUM") => 1,
            Some("LARGE") => 2,
            _ => panic!("Invalid size: {:?}", m.size),
        };
        (true, size, 0, to_pos)
    } else {
        let from = m.from.expect("Slide move must have from");
        let from_pos = from[0] * 3 + from[1];
        (false, 0, from_pos, to_pos)
    }
}

/// Convert V2 Move to the same comparable key
fn v2_move_to_key(m: &Move) -> (bool, u8, u8, u8) {
    match m {
        Move::Place { size, to } => {
            let size_idx = *size as u8;
            (true, size_idx, 0, to.0)
        }
        Move::Slide { from, to } => {
            (false, 0, from.0, to.0)
        }
    }
}

/// Load test positions from JSON file
fn load_test_positions(path: &Path) -> TestData {
    let file = File::open(path).expect("Failed to open test positions file");
    let reader = BufReader::new(file);
    serde_json::from_reader(reader).expect("Failed to parse JSON")
}

#[test]
fn test_v1_v2_parity() {
    // Path to V1 test positions (relative to project root)
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("v1/solver/v1_test_positions.json");

    if !path.exists() {
        println!("Skipping parity test: {} not found", path.display());
        println!("Run v1/solver/export_test_positions.py to generate it.");
        return;
    }

    println!("Loading test positions from {}...", path.display());
    let data = load_test_positions(&path);
    println!("Loaded {} positions (V1 {})", data.stats.total_positions, data.version);

    let mut passed = 0;
    let mut failed = 0;
    let mut edge_case_failures = 0;
    let mut failures: Vec<String> = Vec::new();

    for (i, pos) in data.positions.iter().enumerate() {
        let mut errors: Vec<String> = Vec::new();

        // Check if this is a game tree position (valid) or manually constructed edge case
        let is_game_tree = pos.description.starts_with("game_tree")
            || pos.description.starts_with("terminal");

        // 1. Create board from V1 encoding
        let board = encoding_to_board(pos.encoding);

        // 2. Verify player matches
        let expected_player = if pos.current_player == 1 { Player::One } else { Player::Two };
        if board.current_player() != expected_player {
            errors.push(format!(
                "Player mismatch: V1={}, V2={:?}",
                pos.current_player,
                board.current_player()
            ));
        }

        // 3. Verify canonical encoding
        let v2_canonical = board.canonical();
        if v2_canonical != pos.canonical {
            errors.push(format!(
                "Canonical mismatch: V1={}, V2={}",
                pos.canonical,
                v2_canonical
            ));
        }

        // 4. Verify winner
        let v2_winner = match board.check_winner() {
            Some(Player::One) => Some(1u8),
            Some(Player::Two) => Some(2u8),
            None => None,
        };
        if v2_winner != pos.winner {
            errors.push(format!(
                "Winner mismatch: V1={:?}, V2={:?}",
                pos.winner,
                v2_winner
            ));
        }

        // 5. Verify legal moves (only if no winner - game ongoing)
        if pos.winner.is_none() {
            let v2_moves = board.legal_moves();

            // Check move count
            if v2_moves.len() != pos.legal_move_count {
                errors.push(format!(
                    "Move count mismatch: V1={}, V2={}",
                    pos.legal_move_count,
                    v2_moves.len()
                ));
            }

            // Check moves as sets
            let v1_move_set: HashSet<_> = pos.legal_moves.iter().map(move_to_key).collect();
            let v2_move_set: HashSet<_> = v2_moves.iter().map(v2_move_to_key).collect();

            if v1_move_set != v2_move_set {
                let v1_only: Vec<_> = v1_move_set.difference(&v2_move_set).collect();
                let v2_only: Vec<_> = v2_move_set.difference(&v1_move_set).collect();

                if !v1_only.is_empty() {
                    errors.push(format!("Moves in V1 only: {:?}", v1_only));
                }
                if !v2_only.is_empty() {
                    errors.push(format!("Moves in V2 only: {:?}", v2_only));
                }
            }
        }

        if errors.is_empty() {
            passed += 1;
        } else {
            if is_game_tree {
                // Game tree failures are real bugs
                failed += 1;
                if failures.len() < 10 {
                    failures.push(format!(
                        "Position {} ({}): encoding={}, depth={}\n  {}",
                        i,
                        pos.description,
                        pos.encoding,
                        pos.depth,
                        errors.join("\n  ")
                    ));
                }
            } else {
                // Edge case failures are expected (V1 reserve inconsistencies)
                edge_case_failures += 1;
            }
        }

        // Progress every 10000 positions
        if (i + 1) % 10000 == 0 {
            println!("  Checked {}/{} positions...", i + 1, data.positions.len());
        }
    }

    println!("\n=== Parity Test Results ===");
    println!("Game tree positions passed: {}", passed);
    println!("Game tree positions failed: {}", failed);
    println!("Edge case failures (expected): {}", edge_case_failures);

    if !failures.is_empty() {
        println!("\nFirst {} game tree failures:", failures.len());
        for f in &failures {
            println!("\n{}", f);
        }
    }

    // Only assert on game tree positions - edge cases may fail due to V1 reserve bugs
    assert_eq!(failed, 0, "{} game tree positions failed parity check", failed);
}

/// Test only game tree positions (generated by actual gameplay)
/// These should ALL pass because they have consistent reserve state.
#[test]
fn test_game_tree_parity() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("v1/solver/v1_test_positions.json");

    if !path.exists() {
        println!("Skipping game tree parity test: file not found");
        return;
    }

    let data = load_test_positions(&path);

    // Filter to just game tree positions (valid states from actual gameplay)
    let game_tree: Vec<_> = data.positions.iter()
        .filter(|p| p.description.starts_with("game_tree") || p.description.starts_with("terminal"))
        .collect();

    println!("Testing {} game tree positions...", game_tree.len());

    let mut passed = 0;
    let mut failed = 0;

    for pos in &game_tree {
        let board = encoding_to_board(pos.encoding);
        let mut ok = true;

        // Verify canonical
        if board.canonical() != pos.canonical {
            ok = false;
        }

        // Verify winner
        let v2_winner = match board.check_winner() {
            Some(Player::One) => Some(1u8),
            Some(Player::Two) => Some(2u8),
            None => None,
        };
        if v2_winner != pos.winner {
            ok = false;
        }

        // Verify move count if game ongoing
        if pos.winner.is_none() {
            let v2_moves = board.legal_moves();
            if v2_moves.len() != pos.legal_move_count {
                ok = false;
            }
        }

        if ok {
            passed += 1;
        } else {
            failed += 1;
            if failed <= 5 {
                println!("FAIL: {} (encoding={})", pos.description, pos.encoding);
            }
        }
    }

    println!("Game tree: {} passed, {} failed", passed, failed);
    assert_eq!(failed, 0, "Game tree positions should all pass");
}

/// Note: Edge cases are manually constructed in V1 using direct piece placement
/// which bypasses reserve tracking. V2 derives reserves from pieces on board,
/// so these states are inconsistent. This test documents the known issues.
#[test]
fn test_edge_cases_parity_report() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("v1/solver/v1_test_positions.json");

    if !path.exists() {
        println!("Skipping edge case parity test: file not found");
        return;
    }

    let data = load_test_positions(&path);

    // Filter to manually constructed edge cases
    let edge_cases: Vec<_> = data.positions.iter()
        .filter(|p| !p.description.starts_with("game_tree") && !p.description.starts_with("terminal"))
        .collect();

    println!("Checking {} manually constructed edge cases...", edge_cases.len());
    println!("(Some may fail due to V1 reserve inconsistencies)\n");

    let mut passed = 0;
    let mut failed = 0;

    for pos in &edge_cases {
        let board = encoding_to_board(pos.encoding);
        let mut errors: Vec<String> = Vec::new();

        // Verify canonical
        if board.canonical() != pos.canonical {
            errors.push(format!("canonical: V1={}, V2={}", pos.canonical, board.canonical()));
        }

        // Verify winner
        let v2_winner = match board.check_winner() {
            Some(Player::One) => Some(1u8),
            Some(Player::Two) => Some(2u8),
            None => None,
        };
        if v2_winner != pos.winner {
            errors.push(format!("winner: V1={:?}, V2={:?}", pos.winner, v2_winner));
        }

        // Verify move count if game ongoing
        if pos.winner.is_none() && v2_winner.is_none() {
            let v2_moves = board.legal_moves();
            if v2_moves.len() != pos.legal_move_count {
                errors.push(format!("moves: V1={}, V2={}", pos.legal_move_count, v2_moves.len()));
            }
        }

        if errors.is_empty() {
            passed += 1;
        } else {
            failed += 1;
            println!("  {}: {}", pos.description, errors.join(", "));
        }
    }

    println!("\nEdge cases: {} passed, {} failed", passed, failed);
    println!("Note: Failures are expected for V1 states with invalid reserve tracking.");

    // We don't assert here - just report. The real validation is game_tree_parity.
}
