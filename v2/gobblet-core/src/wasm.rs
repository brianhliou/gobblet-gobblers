//! WASM bindings for gobblet-core
//!
//! Provides a JavaScript-friendly API for the game logic.

use wasm_bindgen::prelude::*;
use crate::{Board, Move, Player, Pos, Size};

/// WASM-friendly wrapper around Board
#[wasm_bindgen]
pub struct WasmBoard {
    inner: Board,
}

#[wasm_bindgen]
impl WasmBoard {
    /// Create a new empty board
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmBoard {
        WasmBoard { inner: Board::new() }
    }

    /// Create board from u64 encoding
    #[wasm_bindgen(js_name = fromU64)]
    pub fn from_u64(bits: u64) -> WasmBoard {
        WasmBoard { inner: Board::from_u64(bits) }
    }

    /// Get u64 encoding of board
    #[wasm_bindgen(js_name = toU64)]
    pub fn to_u64(&self) -> u64 {
        self.inner.to_u64()
    }

    /// Get canonical position encoding (for tablebase lookups)
    pub fn canonical(&self) -> u64 {
        self.inner.canonical()
    }

    /// Current player (1 or 2)
    #[wasm_bindgen(js_name = currentPlayer)]
    pub fn current_player(&self) -> u8 {
        self.inner.current_player() as u8
    }

    /// Check for winner. Returns 0 (none), 1 (P1), or 2 (P2)
    #[wasm_bindgen(js_name = checkWinner)]
    pub fn check_winner(&self) -> u8 {
        match self.inner.check_winner() {
            None => 0,
            Some(Player::One) => 1,
            Some(Player::Two) => 2,
        }
    }

    /// Get winning line as array of positions [row, col, row, col, row, col]
    /// Returns empty array if no winner
    #[wasm_bindgen(js_name = winningLine)]
    pub fn winning_line(&self) -> Vec<u8> {
        if let Some(winner) = self.inner.check_winner() {
            if let Some(line) = self.inner.winning_line(winner) {
                return line.iter()
                    .flat_map(|pos| [pos.row(), pos.col()])
                    .collect();
            }
        }
        vec![]
    }

    /// Get reserves for a player as [small, medium, large]
    pub fn reserves(&self, player: u8) -> Vec<u8> {
        let p = if player == 1 { Player::One } else { Player::Two };
        self.inner.reserves(p).to_vec()
    }

    /// Get legal moves as JSON array
    /// Each move is { to: [row, col], from: [row, col] | null, size: 1|2|3 | null }
    #[wasm_bindgen(js_name = legalMoves)]
    pub fn legal_moves(&self) -> JsValue {
        let moves: Vec<WasmMove> = self.inner.legal_moves()
            .into_iter()
            .map(WasmMove::from)
            .collect();
        serde_wasm_bindgen::to_value(&moves).unwrap()
    }

    /// Apply a move. Returns true if successful.
    /// For placement: apply(toRow, toCol, null, null, size)
    /// For slide: apply(toRow, toCol, fromRow, fromCol, null)
    #[wasm_bindgen(js_name = applyMove)]
    pub fn apply_move(
        &mut self,
        to_row: u8,
        to_col: u8,
        from_row: Option<u8>,
        from_col: Option<u8>,
        size: Option<u8>,
    ) -> bool {
        let mov = if let (Some(fr), Some(fc)) = (from_row, from_col) {
            Move::Slide {
                from: Pos::from_row_col(fr, fc),
                to: Pos::from_row_col(to_row, to_col),
            }
        } else if let Some(s) = size {
            let size = match s {
                1 => Size::Small,
                2 => Size::Medium,
                3 => Size::Large,
                _ => return false,
            };
            Move::Place {
                size,
                to: Pos::from_row_col(to_row, to_col),
            }
        } else {
            return false;
        };

        // Verify move is legal
        if !self.inner.legal_moves().contains(&mov) {
            return false;
        }

        self.inner.apply(mov);
        true
    }

    /// Get cell stack at position as array of [player, size, player, size, ...]
    /// Bottom to top order
    #[wasm_bindgen(js_name = cellStack)]
    pub fn cell_stack(&self, row: u8, col: u8) -> Vec<u8> {
        let pos = Pos::from_row_col(row, col);
        let cell = self.inner.cell(pos);
        let mut stack = vec![];

        for size_idx in 0..3u8 {
            let owner = ((cell >> (size_idx * 2)) & 0b11) as u8;
            if owner != 0 {
                stack.push(owner);           // player (1 or 2)
                stack.push(size_idx + 1);    // size (1=S, 2=M, 3=L)
            }
        }
        stack
    }

    /// Check if game is over (has winner or no legal moves)
    #[wasm_bindgen(js_name = isGameOver)]
    pub fn is_game_over(&self) -> bool {
        self.inner.check_winner().is_some() || self.inner.legal_moves().is_empty()
    }

    /// Get game result: "ongoing", "player_one_wins", "player_two_wins", or "draw"
    pub fn result(&self) -> String {
        if let Some(winner) = self.inner.check_winner() {
            match winner {
                Player::One => "player_one_wins".to_string(),
                Player::Two => "player_two_wins".to_string(),
            }
        } else if self.inner.legal_moves().is_empty() {
            // Zugzwang - current player loses
            match self.inner.current_player() {
                Player::One => "player_two_wins".to_string(),
                Player::Two => "player_one_wins".to_string(),
            }
        } else {
            "ongoing".to_string()
        }
    }

    /// Clone the board
    #[wasm_bindgen(js_name = clone)]
    pub fn clone_board(&self) -> WasmBoard {
        WasmBoard { inner: Board::from_u64(self.inner.to_u64()) }
    }
}

impl Default for WasmBoard {
    fn default() -> Self {
        Self::new()
    }
}

/// Serializable move for JavaScript
#[derive(serde::Serialize)]
struct WasmMove {
    to: [u8; 2],
    from: Option<[u8; 2]>,
    size: Option<u8>,
}

impl From<Move> for WasmMove {
    fn from(mov: Move) -> Self {
        match mov {
            Move::Place { size, to } => WasmMove {
                to: [to.row(), to.col()],
                from: None,
                size: Some(size as u8 + 1),
            },
            Move::Slide { from, to } => WasmMove {
                to: [to.row(), to.col()],
                from: Some([from.row(), from.col()]),
                size: None,
            },
        }
    }
}
