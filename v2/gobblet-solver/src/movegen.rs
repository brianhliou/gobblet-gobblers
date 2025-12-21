//! Lazy move generator for memory-efficient solving.
//!
//! Instead of generating all moves upfront, this iterator produces moves
//! one at a time, tracking its state to resume where it left off.

use gobblet_core::{Board, Move, Player, Pos, Size};

/// Lazy move generator that produces moves on demand.
///
/// Generates moves in order:
/// 1. Reserve placements (Small → Medium → Large, matching V1 for pruning efficiency)
/// 2. Board moves (from each cell)
pub struct MoveGenerator {
    /// Current phase of generation
    phase: MoveGenPhase,
    /// Current player
    player: Player,
    /// Reserve counts [small, medium, large]
    reserves: [u8; 3],
    /// Current size index for reserve phase (0=Small, 1=Medium, 2=Large)
    reserve_size_idx: i8,
    /// Current destination for reserve placement
    reserve_dest_idx: u8,
    /// Current source cell for board moves
    board_from_idx: u8,
    /// Current destination for board moves
    board_to_idx: u8,
    /// Size of piece being moved (for board moves)
    moving_size: Option<Size>,
    /// Restricted destinations (for reveal rule)
    restricted_dests: Option<Vec<Pos>>,
}

#[derive(Clone, Copy, PartialEq)]
enum MoveGenPhase {
    ReservePlacements,
    BoardMoves,
    Done,
}

impl MoveGenerator {
    /// Create a new move generator for the given board.
    pub fn new(board: &Board) -> Self {
        let player = board.current_player();
        let reserves = board.reserves(player);

        Self {
            phase: MoveGenPhase::ReservePlacements,
            player,
            reserves,
            reserve_size_idx: 0, // Start with Small (matches V1 for pruning efficiency)
            reserve_dest_idx: 0,
            board_from_idx: 0,
            board_to_idx: 0,
            moving_size: None,
            restricted_dests: None,
        }
    }

    /// Get the next legal move, or None if exhausted.
    pub fn next(&mut self, board: &Board) -> Option<Move> {
        loop {
            match self.phase {
                MoveGenPhase::ReservePlacements => {
                    if let Some(mov) = self.next_reserve_move(board) {
                        return Some(mov);
                    }
                    // Move to board moves phase
                    self.phase = MoveGenPhase::BoardMoves;
                    self.board_from_idx = 0;
                }
                MoveGenPhase::BoardMoves => {
                    if let Some(mov) = self.next_board_move(board) {
                        return Some(mov);
                    }
                    self.phase = MoveGenPhase::Done;
                    return None;
                }
                MoveGenPhase::Done => return None,
            }
        }
    }

    fn next_reserve_move(&mut self, board: &Board) -> Option<Move> {
        while self.reserve_size_idx <= 2 {
            let size_idx = self.reserve_size_idx as usize;
            let size = match size_idx {
                0 => Size::Small,
                1 => Size::Medium,
                2 => Size::Large,
                _ => unreachable!(),
            };

            // Check if we have this piece in reserve
            if self.reserves[size_idx] > 0 {
                // Try destinations
                while self.reserve_dest_idx < 9 {
                    let dest = Pos(self.reserve_dest_idx);
                    self.reserve_dest_idx += 1;

                    if board.can_place(size, dest) {
                        return Some(Move::Place { size, to: dest });
                    }
                }
            }

            // Move to next size (Small → Medium → Large)
            self.reserve_size_idx += 1;
            self.reserve_dest_idx = 0;
        }
        None
    }

    fn next_board_move(&mut self, board: &Board) -> Option<Move> {
        while self.board_from_idx < 9 {
            let from = Pos(self.board_from_idx);

            // Check if we need to initialize for this source cell
            if self.moving_size.is_none() {
                if let Some((piece_player, size)) = board.top_piece(from) {
                    if piece_player == self.player {
                        self.moving_size = Some(size);
                        self.board_to_idx = 0;

                        // Check reveal rule
                        self.restricted_dests = self.check_reveal(board, from);
                    }
                }

                if self.moving_size.is_none() {
                    // No piece to move from this cell
                    self.board_from_idx += 1;
                    continue;
                }
            }

            let size = self.moving_size.unwrap();

            // Try destinations
            while self.board_to_idx < 9 {
                let to = Pos(self.board_to_idx);
                self.board_to_idx += 1;

                // Same-square restriction
                if to == from {
                    continue;
                }

                // Check if valid destination
                if !board.can_place(size, to) {
                    continue;
                }

                // Check reveal restriction
                if let Some(ref dests) = self.restricted_dests {
                    if !dests.contains(&to) {
                        continue;
                    }
                }

                return Some(Move::Slide { from, to });
            }

            // Done with this source cell
            self.board_from_idx += 1;
            self.moving_size = None;
            self.restricted_dests = None;
        }
        None
    }

    /// Check if lifting from `from` reveals opponent win.
    /// Returns Some(valid_destinations) if restricted, None if unrestricted.
    fn check_reveal(&self, board: &Board, from: Pos) -> Option<Vec<Pos>> {
        // Temporarily remove top piece to check
        let mut test_board = Board::from_u64(board.to_u64());

        // Get and remove the piece we're lifting
        let (_, size) = test_board.pop_top(from)?;

        // Check if opponent now has a winning line
        let opponent = self.player.opponent();
        if let Some(line) = test_board.winning_line(opponent) {
            // Must gobble into the winning line
            let valid: Vec<Pos> = line
                .iter()
                .filter(|&&pos| pos != from && board.can_place(size, pos))
                .copied()
                .collect();
            Some(valid)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_position_move_count() {
        let board = Board::new();
        let mut gen = MoveGenerator::new(&board);
        let mut count = 0;
        while gen.next(&board).is_some() {
            count += 1;
        }
        // Initial position has 27 moves (3 sizes × 9 destinations)
        assert_eq!(count, 27);
    }

    #[test]
    fn test_generator_vs_legal_moves() {
        let board = Board::new();

        // Get moves from generator
        let mut gen = MoveGenerator::new(&board);
        let mut gen_moves = Vec::new();
        while let Some(mov) = gen.next(&board) {
            gen_moves.push(mov);
        }

        // Get moves from Board::legal_moves
        let legal_moves = board.legal_moves();

        // Same count
        assert_eq!(gen_moves.len(), legal_moves.len());

        // All generator moves should be in legal_moves
        for mov in &gen_moves {
            assert!(
                legal_moves.contains(mov),
                "Generator produced illegal move: {:?}",
                mov
            );
        }
    }
}
