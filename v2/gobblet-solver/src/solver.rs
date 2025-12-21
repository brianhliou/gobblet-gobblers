//! Iterative minimax solver with optional alpha-beta pruning.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use gobblet_core::{Board, Move, Player};

use crate::checkpoint::Checkpoint;
use crate::movegen::MoveGenerator;
use crate::stats::SolverStats;

/// Outcome values
pub const WIN_P1: i8 = 1;
pub const DRAW: i8 = 0;
pub const WIN_P2: i8 = -1;

/// Stack frame for iterative minimax.
struct Frame {
    /// Canonical hash of this position
    canonical: u64,
    /// Undo info to restore parent state (None for root)
    undo: Option<gobblet_core::Undo>,
    /// Moves to explore (sorted by priority when pruning, natural order otherwise)
    moves: Vec<Move>,
    /// Index into moves
    move_idx: usize,
    /// Best outcome found so far
    best_outcome: i8,
    /// Whether this player is maximizing (P1) or minimizing (P2)
    is_maximizing: bool,
    /// Number of children evaluated
    children_evaluated: u32,
}

/// Minimax solver with transposition table.
pub struct Solver {
    /// Transposition table: canonical position -> outcome
    pub table: HashMap<u64, i8>,
    /// Solver statistics
    pub stats: SolverStats,
}

impl Solver {
    pub fn new() -> Self {
        Self {
            table: HashMap::new(),
            stats: SolverStats::new(),
        }
    }

    /// Create a new frame with V1-style move ordering (for pruned solve).
    /// Generates all moves and sorts: wins first, unknown/draws middle, losses last.
    /// Like V1, adds terminal positions to the table during scan for proper priority ordering.
    fn create_frame_with_ordering(
        &mut self,
        board: &Board,
        canonical: u64,
        undo: Option<gobblet_core::Undo>,
    ) -> Frame {
        let is_maximizing = board.current_player() == Player::One;
        let best_for_player = if is_maximizing { WIN_P1 } else { WIN_P2 };
        let worst_for_player = if is_maximizing { WIN_P2 } else { WIN_P1 };

        // Generate all moves with their priorities
        // Priority: 0 = known win, 1 = unknown/draw, 2 = known loss
        let mut moves_with_priority: Vec<(Move, u8)> = Vec::new();
        let mut scan_board = *board;
        let mut scan_gen = MoveGenerator::new(&scan_board);

        while let Some(mov) = scan_gen.next(&scan_board) {
            let scan_undo = scan_board.apply(mov);
            let child_canonical = scan_board.canonical();

            // V1-style: check for terminal positions and add to table BEFORE sorting
            // This ensures winning moves get priority 0 in the sort
            let priority = if let Some(&outcome) = self.table.get(&child_canonical) {
                if outcome == best_for_player {
                    0 // Known win - explore first
                } else if outcome == worst_for_player {
                    2 // Known loss - explore last
                } else {
                    1 // Draw - middle
                }
            } else if let Some(winner) = scan_board.check_winner() {
                // Terminal position - add to table and assign priority
                let outcome = if winner == Player::One { WIN_P1 } else { WIN_P2 };
                self.table.insert(child_canonical, outcome);
                self.stats.record_terminal(outcome);
                if outcome == best_for_player {
                    0 // Immediate win - explore first
                } else {
                    2 // Immediate loss - explore last
                }
            } else {
                1 // Unknown/ongoing - middle
            };

            moves_with_priority.push((mov, priority));
            scan_board.undo(&scan_undo);
        }

        // Sort by priority (stable sort preserves order within same priority)
        moves_with_priority.sort_by_key(|&(_, p)| p);

        let moves: Vec<Move> = moves_with_priority.into_iter().map(|(m, _)| m).collect();

        Frame {
            canonical,
            undo,
            moves,
            move_idx: 0,
            best_outcome: if is_maximizing { -2 } else { 2 },
            is_maximizing,
            children_evaluated: 0,
        }
    }

    /// Create a new frame without move ordering (for unpruned full solve).
    /// Just generates moves in natural order without any scanning overhead.
    fn create_frame_simple(
        &self,
        board: &Board,
        canonical: u64,
        undo: Option<gobblet_core::Undo>,
    ) -> Frame {
        let is_maximizing = board.current_player() == Player::One;

        // Generate all moves in natural order (no priority scanning)
        let mut moves = Vec::new();
        let mut gen = MoveGenerator::new(board);
        while let Some(mov) = gen.next(board) {
            moves.push(mov);
        }

        Frame {
            canonical,
            undo,
            moves,
            move_idx: 0,
            best_outcome: if is_maximizing { -2 } else { 2 },
            is_maximizing,
            children_evaluated: 0,
        }
    }

    /// Get the next move for a frame.
    #[inline]
    fn next_move(frame: &mut Frame) -> Option<Move> {
        if frame.move_idx < frame.moves.len() {
            let mov = frame.moves[frame.move_idx];
            frame.move_idx += 1;
            Some(mov)
        } else {
            None
        }
    }

    /// Solve from the given position.
    ///
    /// Args:
    ///   - `prune`: If true, use alpha-beta pruning with move ordering (faster for optimal play).
    ///              If false, explore all positions without pruning (for full game tree solve).
    ///
    /// Returns the outcome (WIN_P1, DRAW, WIN_P2) or None if interrupted.
    pub fn solve(
        &mut self,
        mut board: Board,
        prune: bool,
        running: Arc<AtomicBool>,
        checkpoint_interval_secs: u64,
        log_interval_secs: u64,
        checkpoint_path: &Path,
    ) -> Option<i8> {
        let initial_canonical = board.canonical();

        // Check if already solved
        if let Some(&outcome) = self.table.get(&initial_canonical) {
            return Some(outcome);
        }

        // Stack for iterative DFS
        let mut stack: Vec<Frame> = Vec::with_capacity(1000);

        // Shared path set for cycle detection (O(depth) memory)
        let mut path: HashSet<u64> = HashSet::new();

        // Timing for checkpoints and logging
        let mut last_checkpoint = Instant::now();
        let mut last_log = Instant::now();

        // Push initial frame
        let initial_frame = if prune {
            self.create_frame_with_ordering(&board, initial_canonical, None)
        } else {
            self.create_frame_simple(&board, initial_canonical, None)
        };
        stack.push(initial_frame);
        path.insert(initial_canonical);

        while !stack.is_empty() {
            // Check for interrupt
            if !running.load(Ordering::SeqCst) {
                return None;
            }

            // Periodic checkpoint
            if last_checkpoint.elapsed().as_secs() >= checkpoint_interval_secs {
                println!("\nSaving checkpoint...");
                let start = Instant::now();
                if let Ok(count) = Checkpoint::save(checkpoint_path, &self.table) {
                    println!(
                        "Saved {} positions in {:.2}s\n",
                        count,
                        start.elapsed().as_secs_f64()
                    );
                }
                last_checkpoint = Instant::now();
            }

            // Periodic logging
            if last_log.elapsed().as_secs() >= log_interval_secs {
                self.stats.log_progress(self.table.len());
                last_log = Instant::now();
            }

            let frame = stack.last_mut().unwrap();

            // Alpha-beta pruning: check if we can stop early (only when pruning enabled)
            if prune && frame.children_evaluated > 0 {
                // P1 (maximizer) found a win - no need to explore more
                if frame.is_maximizing && frame.best_outcome == WIN_P1 {
                    // Count remaining moves as pruned
                    let remaining = frame.moves.len() - frame.move_idx;
                    self.stats.branches_pruned += remaining as u64;
                    frame.move_idx = frame.moves.len(); // Skip all remaining
                }
                // P2 (minimizer) found a win - no need to explore more
                else if !frame.is_maximizing && frame.best_outcome == WIN_P2 {
                    let remaining = frame.moves.len() - frame.move_idx;
                    self.stats.branches_pruned += remaining as u64;
                    frame.move_idx = frame.moves.len();
                }
            }

            // Try to get next move
            if let Some(mov) = Self::next_move(frame) {
                // Apply move
                let undo = board.apply(mov);
                let child_canonical = board.canonical();

                // Cycle detection
                if path.contains(&child_canonical) {
                    self.stats.cycle_draws += 1;
                    self.update_best(frame, DRAW);
                    board.undo(&undo);
                    continue;
                }

                // Cache hit
                if let Some(&outcome) = self.table.get(&child_canonical) {
                    self.stats.cache_hits += 1;
                    self.update_best(frame, outcome);
                    board.undo(&undo);
                    continue;
                }

                // Terminal check - someone won?
                if let Some(winner) = board.check_winner() {
                    let outcome = if winner == Player::One { WIN_P1 } else { WIN_P2 };
                    self.table.insert(child_canonical, outcome);
                    self.stats.record_terminal(outcome);
                    self.update_best(frame, outcome);
                    board.undo(&undo);
                    continue;
                }

                // Check for zugzwang (no legal moves)
                // We'll detect this when the child frame has no moves

                // Push child frame
                let child_frame = if prune {
                    self.create_frame_with_ordering(&board, child_canonical, Some(undo))
                } else {
                    self.create_frame_simple(&board, child_canonical, Some(undo))
                };
                path.insert(child_canonical);
                stack.push(child_frame);
                self.stats.max_depth = self.stats.max_depth.max(stack.len() as u64);
            } else {
                // No more moves - pop frame and record outcome
                let frame = stack.pop().unwrap();
                path.remove(&frame.canonical);

                // Determine final outcome
                let outcome = if frame.children_evaluated == 0 {
                    // Zugzwang - no legal moves, current player loses
                    let loss = if frame.is_maximizing { WIN_P2 } else { WIN_P1 };
                    self.stats.record_terminal(loss);
                    loss
                } else {
                    frame.best_outcome
                };

                self.table.insert(frame.canonical, outcome);
                self.stats.positions_evaluated += 1;

                // Undo move and propagate to parent
                if let Some(undo) = frame.undo {
                    board.undo(&undo);
                }

                if let Some(parent) = stack.last_mut() {
                    self.update_best(parent, outcome);
                }
            }
        }

        self.table.get(&initial_canonical).copied()
    }

    /// Update frame's best outcome based on a child result.
    #[inline]
    fn update_best(&self, frame: &mut Frame, child_outcome: i8) {
        frame.children_evaluated += 1;
        if frame.is_maximizing {
            if child_outcome > frame.best_outcome {
                frame.best_outcome = child_outcome;
            }
        } else {
            if child_outcome < frame.best_outcome {
                frame.best_outcome = child_outcome;
            }
        }
    }
}

impl Default for Solver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    #[test]
    fn test_solve_initial_position() {
        let mut solver = Solver::new();
        let board = Board::new();
        let running = Arc::new(AtomicBool::new(true));

        let temp_dir = std::env::temp_dir();
        let checkpoint_path = temp_dir.join("test_solve_checkpoint.bin");

        // Run with pruning enabled (default mode)
        let outcome = solver.solve(
            board,
            true, // prune=true for alpha-beta pruning
            running,
            3600, // Don't checkpoint during test
            3600, // Don't log during test
            &checkpoint_path,
        );

        // Should complete and return WIN_P1
        assert_eq!(outcome, Some(WIN_P1));
        println!("Positions evaluated: {}", solver.stats.positions_evaluated);
        println!("Unique positions: {}", solver.table.len());
    }

    #[test]
    #[ignore] // Takes ~46 minutes - run manually with: cargo test test_solve_unpruned --release -- --ignored
    fn test_solve_unpruned() {
        let mut solver = Solver::new();
        let board = Board::new();
        let running = Arc::new(AtomicBool::new(true));

        let temp_dir = std::env::temp_dir();
        let checkpoint_path = temp_dir.join("test_solve_unpruned.bin");

        // Run WITHOUT pruning (full solve)
        let outcome = solver.solve(
            board,
            false, // prune=false for full solve
            running,
            3600,
            60,
            &checkpoint_path,
        );

        // Both pruned and unpruned should give same result!
        assert_eq!(outcome, Some(WIN_P1), "Unpruned solve should match pruned result!");
        println!("Positions evaluated: {}", solver.stats.positions_evaluated);
        println!("Unique positions: {}", solver.table.len());
        println!("Cycle draws: {}", solver.stats.cycle_draws);
    }
}
