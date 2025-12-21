//! Gobblet Gobblers game logic with bit-based board representation.
//!
//! # Board Encoding (64-bit)
//!
//! ```text
//! Bits 0-53: Board state (9 cells × 6 bits per cell)
//! Bit 54:    Current player (0 = P1, 1 = P2)
//! Bits 55-63: Unused (zero for canonical form)
//!
//! Each cell (6 bits) - indexed by SIZE, not stack position:
//!   Bits 0-1: Small piece owner (0=empty, 1=P1, 2=P2)
//!   Bits 2-3: Medium piece owner
//!   Bits 4-5: Large piece owner
//!
//! Cell encoding: cell_bits = small | (medium << 2) | (large << 4)
//!
//! Cell indices (row-major order):
//!   (0,0)=0  (0,1)=1  (0,2)=2
//!   (1,0)=3  (1,1)=4  (1,2)=5
//!   (2,0)=6  (2,1)=7  (2,2)=8
//! ```
//!
//! # Move Encoding (8-bit)
//!
//! ```text
//! Bits 0-3: destination (0-8)
//! Bits 4-7: source (0-8 for board, 9=Small, 10=Medium, 11=Large reserve)
//! ```
//!
//! # Undo Encoding (32-bit)
//!
//! ```text
//! Bits 0-7:   move encoding
//! Bits 8-10:  captured piece (0=none, 1-6 = player*3 + size)
//! Bits 11-13: revealed piece (same encoding)
//! ```

#[cfg(feature = "wasm")]
pub mod wasm;

/// Player identifier.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
#[repr(u8)]
pub enum Player {
    One = 1,
    Two = 2,
}

impl Player {
    /// Get the opponent player.
    #[inline]
    pub fn opponent(self) -> Player {
        match self {
            Player::One => Player::Two,
            Player::Two => Player::One,
        }
    }

    /// Convert from u8 (1 or 2) to Player.
    #[inline]
    pub fn from_bits(bits: u8) -> Option<Player> {
        match bits {
            1 => Some(Player::One),
            2 => Some(Player::Two),
            _ => None,
        }
    }
}

/// Piece size.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum Size {
    Small = 0,
    Medium = 1,
    Large = 2,
}

impl Size {
    /// Check if this size can gobble (cover) another size.
    #[inline]
    pub fn can_gobble(self, other: Size) -> bool {
        (self as u8) > (other as u8)
    }

    /// Convert from index (0, 1, 2) to Size.
    #[inline]
    pub fn from_index(idx: usize) -> Option<Size> {
        match idx {
            0 => Some(Size::Small),
            1 => Some(Size::Medium),
            2 => Some(Size::Large),
            _ => None,
        }
    }

    /// Get all sizes as an iterator.
    pub fn all() -> impl Iterator<Item = Size> {
        [Size::Small, Size::Medium, Size::Large].into_iter()
    }
}

/// Position on the 3x3 board (0-8).
///
/// Layout:
/// ```text
///   0 1 2
///   3 4 5
///   6 7 8
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Pos(pub u8);

impl Pos {
    /// Create a position from row and column (0-2 each).
    #[inline]
    pub fn from_row_col(row: u8, col: u8) -> Pos {
        debug_assert!(row < 3 && col < 3);
        Pos(row * 3 + col)
    }

    /// Get the row (0-2).
    #[inline]
    pub fn row(self) -> u8 {
        self.0 / 3
    }

    /// Get the column (0-2).
    #[inline]
    pub fn col(self) -> u8 {
        self.0 % 3
    }

    /// Check if this is a valid position (0-8).
    #[inline]
    pub fn is_valid(self) -> bool {
        self.0 < 9
    }

    /// Iterate over all 9 positions.
    pub fn all() -> impl Iterator<Item = Pos> {
        (0..9).map(Pos)
    }
}

/// A move in the game.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Move {
    /// Place a piece from reserves onto the board.
    Place { size: Size, to: Pos },
    /// Move a piece from one position to another on the board.
    Slide { from: Pos, to: Pos },
}

impl Move {
    /// Get the destination position of the move.
    #[inline]
    pub fn to(&self) -> Pos {
        match self {
            Move::Place { to, .. } => *to,
            Move::Slide { to, .. } => *to,
        }
    }
}

/// Undo information for backtracking during search.
#[derive(Clone, Copy, Debug)]
pub struct Undo {
    /// The move that was applied.
    pub mov: Move,
    /// The size of the piece that was moved (needed for Slide undo).
    pub moved_size: Size,
    /// What was captured (covered) at the destination, if any.
    pub captured: Option<(Player, Size)>,
    /// What was revealed at the source after a Slide, if any.
    pub revealed: Option<(Player, Size)>,
}

// ============================================================================
// PACKED BIT TYPES - Zero-allocation move and undo representations
// ============================================================================

/// Packed move representation (8 bits).
///
/// Encoding:
/// - Bits 0-3: destination position (0-8)
/// - Bits 4-7: source (0-8 for board, 9=Small, 10=Medium, 11=Large reserve)
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PackedMove(pub u8);

impl PackedMove {
    const RESERVE_SMALL: u8 = 9;
    const RESERVE_MEDIUM: u8 = 10;
    const RESERVE_LARGE: u8 = 11;

    /// Create a placement move from reserves.
    #[inline]
    pub const fn place(size: Size, to: u8) -> PackedMove {
        let src = match size {
            Size::Small => Self::RESERVE_SMALL,
            Size::Medium => Self::RESERVE_MEDIUM,
            Size::Large => Self::RESERVE_LARGE,
        };
        PackedMove((src << 4) | to)
    }

    /// Create a slide move on the board.
    #[inline]
    pub const fn slide(from: u8, to: u8) -> PackedMove {
        PackedMove((from << 4) | to)
    }

    /// Get the destination position (0-8).
    #[inline]
    pub const fn to(self) -> u8 {
        self.0 & 0x0F
    }

    /// Get the source. Returns Some(pos) for board moves, None for reserve.
    #[inline]
    pub const fn from_pos(self) -> Option<u8> {
        let src = self.0 >> 4;
        if src < 9 { Some(src) } else { None }
    }

    /// Check if this is a placement from reserves.
    #[inline]
    pub const fn is_place(self) -> bool {
        (self.0 >> 4) >= 9
    }

    /// Get the size for reserve placements.
    #[inline]
    pub const fn reserve_size(self) -> Option<Size> {
        match self.0 >> 4 {
            9 => Some(Size::Small),
            10 => Some(Size::Medium),
            11 => Some(Size::Large),
            _ => None,
        }
    }

    /// Get the source position for board moves (0-8).
    #[inline]
    pub const fn source(self) -> u8 {
        self.0 >> 4
    }

    /// Convert to the enum Move type (for compatibility).
    pub fn to_move(self) -> Move {
        if self.is_place() {
            Move::Place {
                size: self.reserve_size().unwrap(),
                to: Pos(self.to()),
            }
        } else {
            Move::Slide {
                from: Pos(self.source()),
                to: Pos(self.to()),
            }
        }
    }

    /// Convert from the enum Move type.
    pub fn from_move(mov: Move) -> PackedMove {
        match mov {
            Move::Place { size, to } => PackedMove::place(size, to.0),
            Move::Slide { from, to } => PackedMove::slide(from.0, to.0),
        }
    }
}

impl std::fmt::Debug for PackedMove {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_place() {
            write!(f, "Place({:?} -> {})", self.reserve_size().unwrap(), self.to())
        } else {
            write!(f, "Slide({} -> {})", self.source(), self.to())
        }
    }
}

/// Packed undo information (16 bits).
///
/// Encoding:
/// - Bits 0-7:   packed move
/// - Bits 8-10:  captured piece (0=none, 1-6 = (player-1)*3 + size + 1)
/// - Bits 11-13: revealed piece (same encoding)
/// - Bits 14-15: unused
#[derive(Clone, Copy)]
pub struct PackedUndo(pub u16);

impl PackedUndo {
    /// Encode a piece as 3 bits (0 = none, 1-6 = piece).
    #[inline]
    const fn encode_piece(piece: Option<(Player, Size)>) -> u16 {
        match piece {
            None => 0,
            Some((player, size)) => (player as u16 - 1) * 3 + size as u16 + 1,
        }
    }

    /// Decode 3 bits to a piece.
    #[inline]
    const fn decode_piece(bits: u16) -> Option<(Player, Size)> {
        if bits == 0 {
            return None;
        }
        let bits = bits - 1;
        let player = if bits < 3 { Player::One } else { Player::Two };
        let size = match bits % 3 {
            0 => Size::Small,
            1 => Size::Medium,
            _ => Size::Large,
        };
        Some((player, size))
    }

    /// Create packed undo info.
    #[inline]
    pub const fn new(
        mov: PackedMove,
        captured: Option<(Player, Size)>,
        revealed: Option<(Player, Size)>,
    ) -> PackedUndo {
        let cap = Self::encode_piece(captured);
        let rev = Self::encode_piece(revealed);
        PackedUndo((mov.0 as u16) | (cap << 8) | (rev << 11))
    }

    /// Get the move.
    #[inline]
    pub const fn mov(self) -> PackedMove {
        PackedMove((self.0 & 0xFF) as u8)
    }

    /// Get the captured piece.
    #[inline]
    pub const fn captured(self) -> Option<(Player, Size)> {
        Self::decode_piece((self.0 >> 8) & 0x07)
    }

    /// Get the revealed piece.
    #[inline]
    pub const fn revealed(self) -> Option<(Player, Size)> {
        Self::decode_piece((self.0 >> 11) & 0x07)
    }

    /// Get the size of the moved piece (needed for undo).
    /// For Place moves, this is the reserve size.
    /// For Slide moves, we need to know what size was moved.
    #[inline]
    pub fn moved_size(self, board_before: &Board) -> Size {
        let mov = self.mov();
        if mov.is_place() {
            mov.reserve_size().unwrap()
        } else {
            // For slides, the moved piece is now at destination
            // But in undo context, we need the size from before
            // We can compute from destination after the move
            // Actually, we need to store this or derive it
            // Let's check destination in current board state
            board_before.top_piece(Pos(mov.to())).unwrap().1
        }
    }
}

impl std::fmt::Debug for PackedUndo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PackedUndo")
            .field("mov", &self.mov())
            .field("captured", &self.captured())
            .field("revealed", &self.revealed())
            .finish()
    }
}

/// Fixed-size array for moves (no heap allocation).
/// Max possible moves: 6 sizes × 9 positions + 9 pieces × 8 destinations = ~81
/// In practice, max is around 40-50.
pub const MAX_MOVES: usize = 64;

/// A fixed-size move list that avoids heap allocation.
#[derive(Clone, Copy)]
pub struct MoveList {
    moves: [PackedMove; MAX_MOVES],
    len: u8,
}

impl MoveList {
    /// Create an empty move list.
    #[inline]
    pub const fn new() -> MoveList {
        MoveList {
            moves: [PackedMove(0); MAX_MOVES],
            len: 0,
        }
    }

    /// Add a move to the list.
    #[inline]
    pub fn push(&mut self, mov: PackedMove) {
        debug_assert!((self.len as usize) < MAX_MOVES);
        self.moves[self.len as usize] = mov;
        self.len += 1;
    }

    /// Get the number of moves.
    #[inline]
    pub const fn len(&self) -> usize {
        self.len as usize
    }

    /// Check if empty.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Get a move by index.
    #[inline]
    pub const fn get(&self, idx: usize) -> PackedMove {
        self.moves[idx]
    }

    /// Iterate over moves.
    pub fn iter(&self) -> impl Iterator<Item = PackedMove> + '_ {
        self.moves[..self.len as usize].iter().copied()
    }
}

/// Compact board state - fits in a single u64.
///
/// See module documentation for encoding details.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Board(pub u64);

impl Board {
    /// Bits per cell (6 bits = 3 sizes × 2 bits each).
    const CELL_BITS: u32 = 6;
    /// Mask for a single cell (0b111111).
    const CELL_MASK: u64 = 0b111111;
    /// Mask for a single layer (2 bits for player: 0=empty, 1=P1, 2=P2).
    const LAYER_MASK: u64 = 0b11;
    /// Bit position for the current player bit.
    const PLAYER_BIT: u32 = 54;

    /// Create a new empty board with Player One to move.
    #[inline]
    pub fn new() -> Board {
        Board(0)
    }

    /// Create a board from a raw u64 encoding.
    #[inline]
    pub fn from_u64(bits: u64) -> Board {
        Board(bits)
    }

    /// Get the raw u64 encoding.
    #[inline]
    pub fn to_u64(self) -> u64 {
        self.0
    }

    /// Get the 6 bits for a cell at the given position.
    #[inline]
    pub fn cell(&self, pos: Pos) -> u64 {
        (self.0 >> (pos.0 as u32 * Self::CELL_BITS)) & Self::CELL_MASK
    }

    /// Set the 6 bits for a cell at the given position.
    #[inline]
    pub fn set_cell(&mut self, pos: Pos, value: u64) {
        let shift = pos.0 as u32 * Self::CELL_BITS;
        self.0 = (self.0 & !(Self::CELL_MASK << shift)) | ((value & Self::CELL_MASK) << shift);
    }

    /// Get the current player.
    #[inline]
    pub fn current_player(&self) -> Player {
        if (self.0 >> Self::PLAYER_BIT) & 1 == 0 {
            Player::One
        } else {
            Player::Two
        }
    }

    /// Switch the current player.
    #[inline]
    pub fn switch_player(&mut self) {
        self.0 ^= 1 << Self::PLAYER_BIT;
    }

    /// Get the owner of a specific size piece at a position.
    /// Returns None if no piece of that size at that position.
    #[inline]
    pub fn piece_owner(&self, pos: Pos, size: Size) -> Option<Player> {
        let cell = self.cell(pos);
        let layer_shift = (size as u32) * 2;
        let bits = (cell >> layer_shift) & Self::LAYER_MASK;
        Player::from_bits(bits as u8)
    }

    /// Get the top (visible) piece at a position.
    /// Returns None if the cell is empty.
    pub fn top_piece(&self, pos: Pos) -> Option<(Player, Size)> {
        let cell = self.cell(pos);
        // Check from largest to smallest (Large = layer 2, Medium = layer 1, Small = layer 0)
        for size_idx in (0..3).rev() {
            let bits = (cell >> (size_idx * 2)) & Self::LAYER_MASK;
            if bits != 0 {
                let player = if bits == 1 { Player::One } else { Player::Two };
                let size = Size::from_index(size_idx as usize).unwrap();
                return Some((player, size));
            }
        }
        None
    }

    /// Optimized top piece detection without loops.
    /// Checks Large, then Medium, then Small using direct conditionals.
    #[inline]
    pub fn top_piece_fast(&self, pos: Pos) -> Option<(Player, Size)> {
        let cell = self.cell(pos);
        let large = (cell >> 4) & Self::LAYER_MASK;
        let medium = (cell >> 2) & Self::LAYER_MASK;
        let small = cell & Self::LAYER_MASK;

        if large != 0 {
            let player = if large == 1 { Player::One } else { Player::Two };
            Some((player, Size::Large))
        } else if medium != 0 {
            let player = if medium == 1 { Player::One } else { Player::Two };
            Some((player, Size::Medium))
        } else if small != 0 {
            let player = if small == 1 { Player::One } else { Player::Two };
            Some((player, Size::Small))
        } else {
            None
        }
    }

    /// Check if a cell is empty.
    #[inline]
    pub fn is_empty(&self, pos: Pos) -> bool {
        self.cell(pos) == 0
    }

    // ========== Piece Operations (Milestone 1.3) ==========

    /// Add a piece to a cell (push onto stack).
    /// The piece becomes visible (on top).
    /// Does NOT validate - caller must ensure move is legal.
    #[inline]
    pub fn push_piece(&mut self, pos: Pos, player: Player, size: Size) {
        let cell = self.cell(pos);
        let layer_shift = (size as u32) * 2;
        let new_cell = (cell & !(Self::LAYER_MASK << layer_shift)) | ((player as u64) << layer_shift);
        self.set_cell(pos, new_cell);
    }

    /// Remove the top piece from a cell.
    /// Returns the piece that was removed, or None if cell was empty.
    /// Also returns what's now visible underneath (for undo tracking).
    pub fn pop_top(&mut self, pos: Pos) -> Option<(Player, Size)> {
        let cell = self.cell(pos);
        // Find the top piece (largest size that's present)
        for size_idx in (0..3).rev() {
            let layer_shift = size_idx * 2;
            let bits = (cell >> layer_shift) & Self::LAYER_MASK;
            if bits != 0 {
                let player = if bits == 1 { Player::One } else { Player::Two };
                let size = Size::from_index(size_idx as usize).unwrap();
                // Clear this layer
                let new_cell = cell & !(Self::LAYER_MASK << layer_shift);
                self.set_cell(pos, new_cell);
                return Some((player, size));
            }
        }
        None
    }

    /// Check if a piece of the given size can be placed at this position.
    /// A piece can be placed if the cell is empty or the top piece is smaller.
    #[inline]
    pub fn can_place(&self, size: Size, pos: Pos) -> bool {
        match self.top_piece(pos) {
            None => true, // Empty cell
            Some((_, top_size)) => size.can_gobble(top_size),
        }
    }

    /// Count pieces of each size on board for a player.
    /// Returns [small_count, medium_count, large_count].
    pub fn pieces_on_board(&self, player: Player) -> [u8; 3] {
        let mut counts = [0u8; 3];
        let player_bits = player as u64;

        for pos_idx in 0..9 {
            let cell = self.cell(Pos(pos_idx));
            for size_idx in 0..3 {
                let bits = (cell >> (size_idx * 2)) & Self::LAYER_MASK;
                if bits == player_bits {
                    counts[size_idx as usize] += 1;
                }
            }
        }
        counts
    }

    /// Get reserve counts for a player (pieces not on board).
    /// Each player starts with 2 of each size.
    /// Returns [small_reserve, medium_reserve, large_reserve].
    #[inline]
    pub fn reserves(&self, player: Player) -> [u8; 3] {
        let on_board = self.pieces_on_board(player);
        [2 - on_board[0], 2 - on_board[1], 2 - on_board[2]]
    }

    // ========== Win Detection (Milestone 1.4) ==========

    /// The 8 winning lines: 3 rows, 3 columns, 2 diagonals.
    const WIN_LINES: [[Pos; 3]; 8] = [
        [Pos(0), Pos(1), Pos(2)], // Row 0
        [Pos(3), Pos(4), Pos(5)], // Row 1
        [Pos(6), Pos(7), Pos(8)], // Row 2
        [Pos(0), Pos(3), Pos(6)], // Col 0
        [Pos(1), Pos(4), Pos(7)], // Col 1
        [Pos(2), Pos(5), Pos(8)], // Col 2
        [Pos(0), Pos(4), Pos(8)], // Main diagonal
        [Pos(2), Pos(4), Pos(6)], // Anti-diagonal
    ];

    /// Bitmasks for winning lines (for bitboard-style win detection).
    /// Each mask has 3 bits set for the cells in that line.
    const WIN_MASKS: [u16; 8] = [
        0b000_000_111, // Row 0: cells 0,1,2
        0b000_111_000, // Row 1: cells 3,4,5
        0b111_000_000, // Row 2: cells 6,7,8
        0b001_001_001, // Col 0: cells 0,3,6
        0b010_010_010, // Col 1: cells 1,4,7
        0b100_100_100, // Col 2: cells 2,5,8
        0b100_010_001, // Main diagonal: cells 0,4,8
        0b001_010_100, // Anti-diagonal: cells 2,4,6
    ];

    /// Check if the given player has won (3 in a row visible on top).
    pub fn has_won(&self, player: Player) -> bool {
        for line in &Self::WIN_LINES {
            let mut count = 0;
            for &pos in line {
                if let Some((p, _)) = self.top_piece(pos) {
                    if p == player {
                        count += 1;
                    }
                }
            }
            if count == 3 {
                return true;
            }
        }
        false
    }

    /// Get the winning line for a player, if any.
    /// Returns the first winning line found (for reveal rule checking).
    pub fn winning_line(&self, player: Player) -> Option<[Pos; 3]> {
        for line in &Self::WIN_LINES {
            let mut count = 0;
            for &pos in line {
                if let Some((p, _)) = self.top_piece(pos) {
                    if p == player {
                        count += 1;
                    }
                }
            }
            if count == 3 {
                return Some(*line);
            }
        }
        None
    }

    /// Check if either player has won.
    /// Returns the winning player, or None if game is ongoing.
    pub fn check_winner(&self) -> Option<Player> {
        if self.has_won(Player::One) {
            Some(Player::One)
        } else if self.has_won(Player::Two) {
            Some(Player::Two)
        } else {
            None
        }
    }

    // ========== Bitboard Win Detection (Optimized) ==========

    /// Get the top piece owner at a cell as 2 bits (0=empty, 1=P1, 2=P2).
    /// This is a branchless version optimized for win detection.
    #[inline]
    fn top_owner_bits(&self, pos: u8) -> u8 {
        let cell = (self.0 >> (pos as u32 * Self::CELL_BITS)) & Self::CELL_MASK;
        let large = (cell >> 4) & 3;
        let medium = (cell >> 2) & 3;
        let small = cell & 3;

        // Prefer large, then medium, then small
        if large != 0 {
            large as u8
        } else if medium != 0 {
            medium as u8
        } else {
            small as u8
        }
    }

    /// Compute visibility masks for both players.
    /// Returns (p1_mask, p2_mask) where bit i is set if that player is visible at cell i.
    #[inline]
    pub fn visibility_masks(&self) -> (u16, u16) {
        let mut p1_mask = 0u16;
        let mut p2_mask = 0u16;

        for pos in 0..9 {
            let owner = self.top_owner_bits(pos);
            if owner == 1 {
                p1_mask |= 1 << pos;
            } else if owner == 2 {
                p2_mask |= 1 << pos;
            }
        }

        (p1_mask, p2_mask)
    }

    /// Fast bitboard-style win detection.
    /// Returns the winner or None if no winner.
    #[inline]
    pub fn check_winner_fast(&self) -> Option<Player> {
        let (p1_mask, p2_mask) = self.visibility_masks();

        for &win_mask in &Self::WIN_MASKS {
            if (p1_mask & win_mask) == win_mask {
                return Some(Player::One);
            }
            if (p2_mask & win_mask) == win_mask {
                return Some(Player::Two);
            }
        }

        None
    }

    /// Fast check if a specific player has won using bitboards.
    #[inline]
    pub fn has_won_fast(&self, player: Player) -> bool {
        let (p1_mask, p2_mask) = self.visibility_masks();
        let player_mask = if player == Player::One { p1_mask } else { p2_mask };

        for &win_mask in &Self::WIN_MASKS {
            if (player_mask & win_mask) == win_mask {
                return true;
            }
        }

        false
    }

    // ========== Move Generation (Milestone 1.5) ==========

    /// Generate all legal moves for the current player.
    /// This is the simple version WITHOUT the reveal rule.
    /// Use `legal_moves()` for the full version with reveal rule.
    pub fn legal_moves_simple(&self) -> Vec<Move> {
        let player = self.current_player();
        let reserves = self.reserves(player);
        let mut moves = Vec::with_capacity(32);

        // Reserve placements
        for size in Size::all() {
            if reserves[size as usize] > 0 {
                for pos in Pos::all() {
                    if self.can_place(size, pos) {
                        moves.push(Move::Place { size, to: pos });
                    }
                }
            }
        }

        // Board moves (pieces current player owns that are visible)
        for from in Pos::all() {
            if let Some((piece_player, size)) = self.top_piece(from) {
                if piece_player == player {
                    // Can move to any valid destination
                    for to in Pos::all() {
                        if from != to && self.can_place(size, to) {
                            moves.push(Move::Slide { from, to });
                        }
                    }
                }
            }
        }

        moves
    }

    // ========== Reveal Rule (Milestone 1.6) ==========

    /// Check what happens when lifting a piece from the given position.
    ///
    /// If lifting reveals an opponent's winning line, returns the winning line.
    /// Returns None if no opponent win is revealed.
    pub fn check_reveal(&self, from: Pos) -> Option<[Pos; 3]> {
        // Create a temporary copy with the top piece removed
        let mut test = *self;
        test.pop_top(from);

        // Check if opponent now wins
        let opponent = self.current_player().opponent();
        test.winning_line(opponent)
    }

    /// Generate all legal moves for the current player, including reveal rule.
    ///
    /// The reveal rule states:
    /// - When you lift a piece, what's underneath is revealed
    /// - If lifting reveals opponent's winning line, you MUST gobble into that line
    /// - You cannot place back on the same square you lifted from
    /// - If no valid gobble target exists, the move is illegal
    pub fn legal_moves(&self) -> Vec<Move> {
        let player = self.current_player();
        let reserves = self.reserves(player);
        let mut moves = Vec::with_capacity(32);

        // Reserve placements are always legal (no reveal)
        for size in Size::all() {
            if reserves[size as usize] > 0 {
                for pos in Pos::all() {
                    if self.can_place(size, pos) {
                        moves.push(Move::Place { size, to: pos });
                    }
                }
            }
        }

        // Board moves with reveal rule
        for from in Pos::all() {
            if let Some((piece_player, size)) = self.top_piece(from) {
                if piece_player == player {
                    // Check if lifting this piece reveals opponent's win
                    if let Some(winning_line) = self.check_reveal(from) {
                        // REVEAL RULE: Must gobble into the winning line
                        // Cannot place back on same square
                        for &line_pos in &winning_line {
                            // Same-square restriction: cannot move back to where we came from
                            if line_pos != from && self.can_place(size, line_pos) {
                                moves.push(Move::Slide { from, to: line_pos });
                            }
                        }
                    } else {
                        // No reveal - normal moves (except same-square)
                        for to in Pos::all() {
                            if from != to && self.can_place(size, to) {
                                moves.push(Move::Slide { from, to });
                            }
                        }
                    }
                }
            }
        }

        moves
    }

    // ========== Apply & Undo (Milestone 1.7) ==========

    /// Apply a move to the board, returning undo information.
    ///
    /// This mutates the board in place and switches the current player.
    /// Use `undo()` with the returned `Undo` struct to reverse the move.
    pub fn apply(&mut self, mov: Move) -> Undo {
        let player = self.current_player();

        match mov {
            Move::Place { size, to } => {
                // Record what was under the destination (if anything)
                let captured = self.top_piece(to);

                // Place the piece
                self.push_piece(to, player, size);
                self.switch_player();

                Undo {
                    mov,
                    moved_size: size,
                    captured,
                    revealed: None, // No reveal for place moves
                }
            }
            Move::Slide { from, to } => {
                // Get the piece we're moving
                let (_, size) = self.top_piece(from).expect("No piece at source");

                // Remove piece from source (reveals what's underneath)
                self.pop_top(from);
                let revealed = self.top_piece(from); // What's now visible at source

                // Record what's at destination before we cover it
                let captured = self.top_piece(to);

                // Place piece at destination
                self.push_piece(to, player, size);
                self.switch_player();

                Undo {
                    mov,
                    moved_size: size,
                    captured,
                    revealed,
                }
            }
        }
    }

    /// Undo a move, restoring the board to its previous state.
    ///
    /// This is the inverse of `apply()`.
    pub fn undo(&mut self, undo: &Undo) {
        // First, switch player back
        self.switch_player();
        let player = self.current_player();

        match undo.mov {
            Move::Place { size: _, to } => {
                // Remove the placed piece
                self.pop_top(to);
                // The captured piece (if any) is still there underneath
            }
            Move::Slide { from, to } => {
                // Remove the piece from destination
                self.pop_top(to);

                // If there was a revealed piece at source, it's still there
                // We need to put our piece back on top at source
                self.push_piece(from, player, undo.moved_size);
            }
        }
    }

    // ========== Packed Move Operations (Zero-allocation) ==========

    /// Generate all legal moves as a packed MoveList (no heap allocation).
    ///
    /// This is the optimized version of `legal_moves()` for solver use.
    pub fn legal_moves_packed(&self) -> MoveList {
        let player = self.current_player();
        let reserves = self.reserves(player);
        let mut moves = MoveList::new();

        // Reserve placements are always legal (no reveal)
        for size_idx in 0..3 {
            if reserves[size_idx] > 0 {
                let size = Size::from_index(size_idx).unwrap();
                for pos in 0..9 {
                    if self.can_place(size, Pos(pos)) {
                        moves.push(PackedMove::place(size, pos));
                    }
                }
            }
        }

        // Board moves with reveal rule
        for from in 0u8..9 {
            if let Some((piece_player, size)) = self.top_piece(Pos(from)) {
                if piece_player == player {
                    // Check if lifting this piece reveals opponent's win
                    if let Some(winning_line) = self.check_reveal(Pos(from)) {
                        // REVEAL RULE: Must gobble into the winning line
                        for &line_pos in &winning_line {
                            if line_pos.0 != from && self.can_place(size, line_pos) {
                                moves.push(PackedMove::slide(from, line_pos.0));
                            }
                        }
                    } else {
                        // No reveal - normal moves (except same-square)
                        for to in 0u8..9 {
                            if from != to && self.can_place(size, Pos(to)) {
                                moves.push(PackedMove::slide(from, to));
                            }
                        }
                    }
                }
            }
        }

        moves
    }

    /// Apply a packed move, returning packed undo information.
    ///
    /// This is the optimized version for solver use (no heap allocation).
    pub fn apply_packed(&mut self, mov: PackedMove) -> PackedUndo {
        let player = self.current_player();
        let to = Pos(mov.to());

        if mov.is_place() {
            // Reserve placement
            let size = mov.reserve_size().unwrap();
            let captured = self.top_piece(to);
            self.push_piece(to, player, size);
            self.switch_player();
            PackedUndo::new(mov, captured, None)
        } else {
            // Board slide
            let from = Pos(mov.source());
            let (_, size) = self.top_piece(from).expect("No piece at source");

            // Remove from source
            self.pop_top(from);
            let revealed = self.top_piece(from);

            // Record and place at destination
            let captured = self.top_piece(to);
            self.push_piece(to, player, size);
            self.switch_player();

            PackedUndo::new(mov, captured, revealed)
        }
    }

    /// Undo a packed move, restoring the board state.
    ///
    /// This is the inverse of `apply_packed()`.
    pub fn undo_packed(&mut self, undo: PackedUndo) {
        self.switch_player();
        let player = self.current_player();
        let mov = undo.mov();
        let to = Pos(mov.to());

        if mov.is_place() {
            // Remove the placed piece
            self.pop_top(to);
        } else {
            // Slide: remove from destination, restore to source
            let (_, size) = self.pop_top(to).expect("No piece at destination");
            let from = Pos(mov.source());
            self.push_piece(from, player, size);
        }
    }

    // ========== Symmetry & Canonicalization (Milestone 1.8) ==========

    /// Position mapping for each of 8 D4 transformations.
    /// Each array maps new_pos -> old_pos for that transformation.
    ///
    /// Board layout:
    /// ```text
    ///   0 1 2
    ///   3 4 5
    ///   6 7 8
    /// ```
    const TRANSFORMS: [[u8; 9]; 8] = [
        [0, 1, 2, 3, 4, 5, 6, 7, 8], // Identity
        [6, 3, 0, 7, 4, 1, 8, 5, 2], // Rotate 90° clockwise
        [8, 7, 6, 5, 4, 3, 2, 1, 0], // Rotate 180°
        [2, 5, 8, 1, 4, 7, 0, 3, 6], // Rotate 270° clockwise
        [2, 1, 0, 5, 4, 3, 8, 7, 6], // Reflect horizontal (flip left-right)
        [6, 7, 8, 3, 4, 5, 0, 1, 2], // Reflect vertical (flip top-bottom)
        [0, 3, 6, 1, 4, 7, 2, 5, 8], // Reflect main diagonal
        [8, 5, 2, 7, 4, 1, 6, 3, 0], // Reflect anti-diagonal
    ];

    /// Apply a transformation to the board, returning the new encoding.
    ///
    /// The transformation index corresponds to `TRANSFORMS`.
    pub fn transform(&self, t: usize) -> u64 {
        let mapping = &Self::TRANSFORMS[t];
        let mut result = 0u64;

        for new_pos in 0..9 {
            let old_pos = mapping[new_pos] as u32;
            let cell = (self.0 >> (old_pos * Self::CELL_BITS)) & Self::CELL_MASK;
            result |= cell << (new_pos as u32 * Self::CELL_BITS);
        }

        // Preserve player bit
        result | (self.0 & (1 << Self::PLAYER_BIT))
    }

    /// Get the canonical form of this board state.
    ///
    /// The canonical form is the minimum encoding across all 8 D4 transformations.
    /// This ensures that symmetric positions map to the same value.
    pub fn canonical(&self) -> u64 {
        let mut min = self.0;
        for t in 1..8 {
            let transformed = self.transform(t);
            if transformed < min {
                min = transformed;
            }
        }
        min
    }

    /// Get all 8 symmetry transformations of this board.
    pub fn all_symmetries(&self) -> [u64; 8] {
        let mut result = [0u64; 8];
        for t in 0..8 {
            result[t] = self.transform(t);
        }
        result
    }
}

impl Default for Board {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_player_opponent() {
        assert_eq!(Player::One.opponent(), Player::Two);
        assert_eq!(Player::Two.opponent(), Player::One);
    }

    #[test]
    fn test_size_can_gobble() {
        assert!(!Size::Small.can_gobble(Size::Small));
        assert!(!Size::Small.can_gobble(Size::Medium));
        assert!(!Size::Small.can_gobble(Size::Large));

        assert!(Size::Medium.can_gobble(Size::Small));
        assert!(!Size::Medium.can_gobble(Size::Medium));
        assert!(!Size::Medium.can_gobble(Size::Large));

        assert!(Size::Large.can_gobble(Size::Small));
        assert!(Size::Large.can_gobble(Size::Medium));
        assert!(!Size::Large.can_gobble(Size::Large));
    }

    #[test]
    fn test_pos_from_row_col() {
        assert_eq!(Pos::from_row_col(0, 0), Pos(0));
        assert_eq!(Pos::from_row_col(0, 1), Pos(1));
        assert_eq!(Pos::from_row_col(0, 2), Pos(2));
        assert_eq!(Pos::from_row_col(1, 0), Pos(3));
        assert_eq!(Pos::from_row_col(1, 1), Pos(4));
        assert_eq!(Pos::from_row_col(2, 2), Pos(8));
    }

    #[test]
    fn test_pos_row_col() {
        for i in 0..9 {
            let pos = Pos(i);
            assert_eq!(Pos::from_row_col(pos.row(), pos.col()), pos);
        }
    }

    #[test]
    fn test_board_new() {
        let board = Board::new();
        assert_eq!(board.0, 0);
        assert_eq!(board.current_player(), Player::One);
    }

    #[test]
    fn test_board_switch_player() {
        let mut board = Board::new();
        assert_eq!(board.current_player(), Player::One);
        board.switch_player();
        assert_eq!(board.current_player(), Player::Two);
        board.switch_player();
        assert_eq!(board.current_player(), Player::One);
    }

    #[test]
    fn test_board_empty_cells() {
        let board = Board::new();
        for i in 0..9 {
            assert!(board.is_empty(Pos(i)));
            assert_eq!(board.top_piece(Pos(i)), None);
        }
    }

    #[test]
    fn test_board_cell_roundtrip() {
        let mut board = Board::new();

        // Set a cell value and read it back
        let test_value = 0b01_10_01; // P1 Small, P2 Medium, P1 Large
        board.set_cell(Pos(4), test_value);
        assert_eq!(board.cell(Pos(4)), test_value);

        // Other cells should still be empty
        assert_eq!(board.cell(Pos(0)), 0);
        assert_eq!(board.cell(Pos(8)), 0);
    }

    #[test]
    fn test_board_top_piece() {
        let mut board = Board::new();

        // Set cell with P1 Small, P2 Medium, P1 Large
        // cell_bits = small | (medium << 2) | (large << 4)
        //           = 1 | (2 << 2) | (1 << 4)
        //           = 1 | 8 | 16 = 25 = 0b011001
        let cell_value = 1 | (2 << 2) | (1 << 4);
        board.set_cell(Pos(0), cell_value);

        // Top piece should be the Large (P1)
        let top = board.top_piece(Pos(0));
        assert_eq!(top, Some((Player::One, Size::Large)));
    }

    #[test]
    fn test_board_piece_owner() {
        let mut board = Board::new();

        // P1 Small, P2 Medium at position 0
        let cell_value = 1 | (2 << 2); // No large
        board.set_cell(Pos(0), cell_value);

        assert_eq!(board.piece_owner(Pos(0), Size::Small), Some(Player::One));
        assert_eq!(board.piece_owner(Pos(0), Size::Medium), Some(Player::Two));
        assert_eq!(board.piece_owner(Pos(0), Size::Large), None);
    }

    #[test]
    fn test_board_encoding_preserves_player() {
        let mut board = Board::new();
        board.set_cell(Pos(4), 0b01); // P1 Small at center
        assert_eq!(board.current_player(), Player::One);
        board.switch_player();

        // Setting cells should not affect player bit
        board.set_cell(Pos(0), 0b10); // P2 Small
        assert_eq!(board.current_player(), Player::Two);
    }

    #[test]
    fn test_move_to() {
        let place = Move::Place { size: Size::Small, to: Pos(4) };
        let slide = Move::Slide { from: Pos(0), to: Pos(8) };

        assert_eq!(place.to(), Pos(4));
        assert_eq!(slide.to(), Pos(8));
    }

    // ========== Milestone 1.3: Piece Operations Tests ==========

    #[test]
    fn test_push_piece() {
        let mut board = Board::new();

        // Push P1 Small at (0,0)
        board.push_piece(Pos(0), Player::One, Size::Small);
        assert_eq!(board.top_piece(Pos(0)), Some((Player::One, Size::Small)));

        // Push P2 Medium at same position (gobble)
        board.push_piece(Pos(0), Player::Two, Size::Medium);
        assert_eq!(board.top_piece(Pos(0)), Some((Player::Two, Size::Medium)));

        // The small piece should still be there
        assert_eq!(board.piece_owner(Pos(0), Size::Small), Some(Player::One));
    }

    #[test]
    fn test_pop_top() {
        let mut board = Board::new();

        // Build a stack: P1 Small, P2 Medium, P1 Large
        board.push_piece(Pos(0), Player::One, Size::Small);
        board.push_piece(Pos(0), Player::Two, Size::Medium);
        board.push_piece(Pos(0), Player::One, Size::Large);

        // Pop should return Large first
        assert_eq!(board.pop_top(Pos(0)), Some((Player::One, Size::Large)));
        assert_eq!(board.top_piece(Pos(0)), Some((Player::Two, Size::Medium)));

        // Pop Medium
        assert_eq!(board.pop_top(Pos(0)), Some((Player::Two, Size::Medium)));
        assert_eq!(board.top_piece(Pos(0)), Some((Player::One, Size::Small)));

        // Pop Small
        assert_eq!(board.pop_top(Pos(0)), Some((Player::One, Size::Small)));
        assert_eq!(board.top_piece(Pos(0)), None);

        // Pop empty cell
        assert_eq!(board.pop_top(Pos(0)), None);
    }

    #[test]
    fn test_push_pop_roundtrip() {
        let mut board = Board::new();
        let original = board.0;

        // Push and pop should restore state
        board.push_piece(Pos(4), Player::One, Size::Medium);
        assert_ne!(board.0, original);
        board.pop_top(Pos(4));
        assert_eq!(board.0, original);
    }

    #[test]
    fn test_can_place() {
        let mut board = Board::new();

        // Empty cell - any size can be placed
        assert!(board.can_place(Size::Small, Pos(0)));
        assert!(board.can_place(Size::Medium, Pos(0)));
        assert!(board.can_place(Size::Large, Pos(0)));

        // Place a Small
        board.push_piece(Pos(0), Player::One, Size::Small);

        // Small cannot gobble Small
        assert!(!board.can_place(Size::Small, Pos(0)));
        // Medium can gobble Small
        assert!(board.can_place(Size::Medium, Pos(0)));
        // Large can gobble Small
        assert!(board.can_place(Size::Large, Pos(0)));

        // Place a Medium on top
        board.push_piece(Pos(0), Player::Two, Size::Medium);

        // Small cannot gobble Medium
        assert!(!board.can_place(Size::Small, Pos(0)));
        // Medium cannot gobble Medium
        assert!(!board.can_place(Size::Medium, Pos(0)));
        // Large can gobble Medium
        assert!(board.can_place(Size::Large, Pos(0)));

        // Place a Large on top
        board.push_piece(Pos(0), Player::One, Size::Large);

        // Nothing can gobble Large
        assert!(!board.can_place(Size::Small, Pos(0)));
        assert!(!board.can_place(Size::Medium, Pos(0)));
        assert!(!board.can_place(Size::Large, Pos(0)));
    }

    #[test]
    fn test_reserves_initial() {
        let board = Board::new();

        // Initial reserves: 2 of each size for each player
        assert_eq!(board.reserves(Player::One), [2, 2, 2]);
        assert_eq!(board.reserves(Player::Two), [2, 2, 2]);
    }

    #[test]
    fn test_reserves_after_placement() {
        let mut board = Board::new();

        // Place P1 Small
        board.push_piece(Pos(0), Player::One, Size::Small);
        assert_eq!(board.reserves(Player::One), [1, 2, 2]);
        assert_eq!(board.reserves(Player::Two), [2, 2, 2]);

        // Place P1 Medium
        board.push_piece(Pos(1), Player::One, Size::Medium);
        assert_eq!(board.reserves(Player::One), [1, 1, 2]);

        // Place P2 Large
        board.push_piece(Pos(2), Player::Two, Size::Large);
        assert_eq!(board.reserves(Player::Two), [2, 2, 1]);

        // Place another P1 Small (both smalls now on board)
        board.push_piece(Pos(3), Player::One, Size::Small);
        assert_eq!(board.reserves(Player::One), [0, 1, 2]);
    }

    #[test]
    fn test_pieces_on_board() {
        let mut board = Board::new();

        assert_eq!(board.pieces_on_board(Player::One), [0, 0, 0]);
        assert_eq!(board.pieces_on_board(Player::Two), [0, 0, 0]);

        board.push_piece(Pos(0), Player::One, Size::Small);
        board.push_piece(Pos(1), Player::One, Size::Small);
        board.push_piece(Pos(2), Player::One, Size::Large);

        assert_eq!(board.pieces_on_board(Player::One), [2, 0, 1]);
        assert_eq!(board.pieces_on_board(Player::Two), [0, 0, 0]);
    }

    #[test]
    fn test_gobbled_pieces_still_count() {
        let mut board = Board::new();

        // P1 Small at (0,0)
        board.push_piece(Pos(0), Player::One, Size::Small);
        // P2 Large gobbles it
        board.push_piece(Pos(0), Player::Two, Size::Large);

        // P1 still has 1 small on board (hidden)
        assert_eq!(board.pieces_on_board(Player::One), [1, 0, 0]);
        assert_eq!(board.reserves(Player::One), [1, 2, 2]);
    }

    // ========== Milestone 1.4: Win Detection Tests ==========

    #[test]
    fn test_no_winner_empty_board() {
        let board = Board::new();
        assert!(!board.has_won(Player::One));
        assert!(!board.has_won(Player::Two));
        assert_eq!(board.check_winner(), None);
    }

    #[test]
    fn test_horizontal_win() {
        let mut board = Board::new();

        // P1 wins row 0
        board.push_piece(Pos(0), Player::One, Size::Small);
        board.push_piece(Pos(1), Player::One, Size::Medium);
        board.push_piece(Pos(2), Player::One, Size::Large);

        assert!(board.has_won(Player::One));
        assert!(!board.has_won(Player::Two));
        assert_eq!(board.check_winner(), Some(Player::One));
        assert_eq!(board.winning_line(Player::One), Some([Pos(0), Pos(1), Pos(2)]));
    }

    #[test]
    fn test_vertical_win() {
        let mut board = Board::new();

        // P2 wins column 0
        board.push_piece(Pos(0), Player::Two, Size::Small);
        board.push_piece(Pos(3), Player::Two, Size::Medium);
        board.push_piece(Pos(6), Player::Two, Size::Large);

        assert!(!board.has_won(Player::One));
        assert!(board.has_won(Player::Two));
        assert_eq!(board.check_winner(), Some(Player::Two));
        assert_eq!(board.winning_line(Player::Two), Some([Pos(0), Pos(3), Pos(6)]));
    }

    #[test]
    fn test_diagonal_win() {
        let mut board = Board::new();

        // P1 wins main diagonal
        board.push_piece(Pos(0), Player::One, Size::Large);
        board.push_piece(Pos(4), Player::One, Size::Medium);
        board.push_piece(Pos(8), Player::One, Size::Small);

        assert!(board.has_won(Player::One));
        assert_eq!(board.winning_line(Player::One), Some([Pos(0), Pos(4), Pos(8)]));
    }

    #[test]
    fn test_anti_diagonal_win() {
        let mut board = Board::new();

        // P2 wins anti-diagonal
        board.push_piece(Pos(2), Player::Two, Size::Small);
        board.push_piece(Pos(4), Player::Two, Size::Medium);
        board.push_piece(Pos(6), Player::Two, Size::Large);

        assert!(board.has_won(Player::Two));
        assert_eq!(board.winning_line(Player::Two), Some([Pos(2), Pos(4), Pos(6)]));
    }

    #[test]
    fn test_hidden_piece_doesnt_count() {
        let mut board = Board::new();

        // P1 has pieces at (0,0), (0,1)
        board.push_piece(Pos(0), Player::One, Size::Small);
        board.push_piece(Pos(1), Player::One, Size::Small);

        // P1 also has a small at (0,2), but P2 gobbles it
        board.push_piece(Pos(2), Player::One, Size::Small);
        board.push_piece(Pos(2), Player::Two, Size::Large);

        // P1 should NOT have won (the (0,2) piece is hidden)
        assert!(!board.has_won(Player::One));
        assert_eq!(board.check_winner(), None);
    }

    #[test]
    fn test_mixed_pieces_no_win() {
        let mut board = Board::new();

        // Row 0: P1, P2, P1 (no win)
        board.push_piece(Pos(0), Player::One, Size::Small);
        board.push_piece(Pos(1), Player::Two, Size::Medium);
        board.push_piece(Pos(2), Player::One, Size::Large);

        assert!(!board.has_won(Player::One));
        assert!(!board.has_won(Player::Two));
        assert_eq!(board.check_winner(), None);
    }

    #[test]
    fn test_multiple_winning_lines() {
        let mut board = Board::new();

        // P1 wins both row 0 and column 0
        board.push_piece(Pos(0), Player::One, Size::Large);
        board.push_piece(Pos(1), Player::One, Size::Medium);
        board.push_piece(Pos(2), Player::One, Size::Small);
        board.push_piece(Pos(3), Player::One, Size::Medium);
        board.push_piece(Pos(6), Player::One, Size::Small);

        assert!(board.has_won(Player::One));
        // Should return first winning line found (row 0)
        assert_eq!(board.winning_line(Player::One), Some([Pos(0), Pos(1), Pos(2)]));
    }

    #[test]
    fn test_all_winning_lines() {
        // Test each of the 8 winning lines
        let lines = [
            [Pos(0), Pos(1), Pos(2)], // Row 0
            [Pos(3), Pos(4), Pos(5)], // Row 1
            [Pos(6), Pos(7), Pos(8)], // Row 2
            [Pos(0), Pos(3), Pos(6)], // Col 0
            [Pos(1), Pos(4), Pos(7)], // Col 1
            [Pos(2), Pos(5), Pos(8)], // Col 2
            [Pos(0), Pos(4), Pos(8)], // Main diagonal
            [Pos(2), Pos(4), Pos(6)], // Anti-diagonal
        ];

        for line in &lines {
            let mut board = Board::new();
            for &pos in line {
                board.push_piece(pos, Player::One, Size::Small);
            }
            assert!(board.has_won(Player::One), "Failed for line {:?}", line);
        }
    }

    // ========== Milestone 1.5: Move Generation Tests ==========

    #[test]
    fn test_initial_moves_count() {
        let board = Board::new();
        let moves = board.legal_moves_simple();

        // Initial: 3 sizes × 9 positions = 27 reserve placements
        // No pieces on board, so no board moves
        assert_eq!(moves.len(), 27);

        // All should be Place moves
        for m in &moves {
            match m {
                Move::Place { .. } => {}
                Move::Slide { .. } => panic!("No board moves should exist initially"),
            }
        }
    }

    #[test]
    fn test_moves_with_piece_on_board() {
        let mut board = Board::new();

        // P1 places Small at center
        board.push_piece(Pos(4), Player::One, Size::Small);
        board.switch_player(); // P2's turn

        let moves = board.legal_moves_simple();

        // P2 has 2 small, 2 medium, 2 large
        // Small can go to 8 empty cells = 8 moves
        // Medium can go to 9 cells (8 empty + gobble center) = 9 moves
        // Large can go to 9 cells = 9 moves
        // Total = 8 + 9 + 9 = 26 reserve moves
        // No P2 pieces on board, so no board moves

        let reserve_moves: Vec<_> = moves
            .iter()
            .filter(|m| matches!(m, Move::Place { .. }))
            .collect();
        assert_eq!(reserve_moves.len(), 26);
    }

    #[test]
    fn test_board_moves() {
        let mut board = Board::new();

        // P1 places Large at (0,0)
        board.push_piece(Pos(0), Player::One, Size::Large);
        // Still P1's turn (didn't switch)

        let moves = board.legal_moves_simple();

        // Reserve: 2 small×9 + 2 medium×9 + 1 large×9 = 45 reserve moves
        // Wait - Large can gobble empty, but center already has something?
        // No - Large at (0,0) only. So 1 Large left can go to 8 empty cells = 8 moves
        // 2 small × 9 = 18, 2 medium × 9 = 18, 1 large × 8 = 8, total reserve = 44

        // Hmm, need to recalculate. Large at (0,0):
        // - Remaining: 2S, 2M, 1L in reserve for P1
        // - Small can place on 8 empty = 8 × 2 = but wait, that's not how it works
        // Reserve counts: [2, 2, 1]
        // Small: 2 > 0, can place on 8 empty cells = 8 moves
        // Medium: 2 > 0, can place on 8 empty + 1 (can gobble P1 Large? No, Medium < Large)
        // Actually Medium cannot gobble Large. So 8 empty = 8 moves
        // Large: 1 > 0, can place on 8 empty = 8 moves
        // Reserve total = 8 + 8 + 8 = 24 moves

        // Board moves: P1 Large at (0,0) can move to 8 other positions
        // = 8 moves

        let reserve_count = moves
            .iter()
            .filter(|m| matches!(m, Move::Place { .. }))
            .count();
        let board_count = moves
            .iter()
            .filter(|m| matches!(m, Move::Slide { .. }))
            .count();

        assert_eq!(reserve_count, 24);
        assert_eq!(board_count, 8);
        assert_eq!(moves.len(), 32);
    }

    #[test]
    fn test_cannot_move_opponent_pieces() {
        let mut board = Board::new();

        // P1 places at (0,0)
        board.push_piece(Pos(0), Player::One, Size::Large);
        board.switch_player(); // P2's turn

        let moves = board.legal_moves_simple();

        // P2 should not be able to move the P1 piece
        let slide_moves: Vec<_> = moves
            .iter()
            .filter(|m| matches!(m, Move::Slide { .. }))
            .collect();
        assert_eq!(slide_moves.len(), 0);
    }

    #[test]
    fn test_gobble_moves() {
        let mut board = Board::new();

        // P1 Small at center
        board.push_piece(Pos(4), Player::One, Size::Small);

        let moves = board.legal_moves_simple();

        // Check that Medium and Large can target center (gobble)
        let can_gobble_center: Vec<_> = moves
            .iter()
            .filter(|m| match m {
                Move::Place { to, size } => *to == Pos(4) && *size != Size::Small,
                _ => false,
            })
            .collect();

        // Medium and Large can both target center = 2 moves
        assert_eq!(can_gobble_center.len(), 2);
    }

    #[test]
    fn test_cannot_gobble_equal_or_larger() {
        let mut board = Board::new();

        // P1 Medium at center
        board.push_piece(Pos(4), Player::One, Size::Medium);

        let moves = board.legal_moves_simple();

        // Small and Medium cannot gobble, only Large can
        let can_target_center: Vec<_> = moves
            .iter()
            .filter(|m| match m {
                Move::Place { to, .. } => *to == Pos(4),
                _ => false,
            })
            .collect();

        assert_eq!(can_target_center.len(), 1); // Only Large
    }

    #[test]
    fn test_no_self_gobble_with_larger() {
        let mut board = Board::new();

        // P1 Large at center - nothing can gobble it
        board.push_piece(Pos(4), Player::One, Size::Large);

        let moves = board.legal_moves_simple();

        // Nothing can target center (not even the Large on board, because from != to)
        let can_target_center: Vec<_> = moves
            .iter()
            .filter(|m| match m {
                Move::Place { to, .. } => *to == Pos(4),
                Move::Slide { to, .. } => *to == Pos(4),
            })
            .collect();

        assert_eq!(can_target_center.len(), 0);
    }

    #[test]
    fn test_exhausted_reserves() {
        let mut board = Board::new();

        // P1 places both smalls
        board.push_piece(Pos(0), Player::One, Size::Small);
        board.push_piece(Pos(1), Player::One, Size::Small);

        let moves = board.legal_moves_simple();

        // No more Small placements for P1
        let small_placements: Vec<_> = moves
            .iter()
            .filter(|m| matches!(m, Move::Place { size: Size::Small, .. }))
            .collect();
        assert_eq!(small_placements.len(), 0);

        // But Medium and Large still available
        let medium_placements: Vec<_> = moves
            .iter()
            .filter(|m| matches!(m, Move::Place { size: Size::Medium, .. }))
            .collect();
        assert!(medium_placements.len() > 0);
    }

    // ========== Milestone 1.6: Reveal Rule Tests ==========
    // These tests match the scenarios from game_logic_testing.md Category 4

    #[test]
    fn test_reveal_basic_restricted_destinations() {
        // Test 4.1: Basic Reveal - Restricted Destinations
        // P2 visible at (0,0), (0,1)
        // Stack at (0,2) = [P2 Small, P1 Large]
        // P1 to move
        let mut board = Board::new();
        board.push_piece(Pos(0), Player::Two, Size::Small);
        board.push_piece(Pos(1), Player::Two, Size::Medium);
        board.push_piece(Pos(2), Player::Two, Size::Small); // P2 under P1
        board.push_piece(Pos(2), Player::One, Size::Large); // P1 on top
        // P1's turn (default)

        let moves = board.legal_moves();

        // Lifting P1 Large from (0,2) reveals P2 Small → P2 wins row 0
        // Valid board move destinations: only (0,0), (0,1) where P1 can gobble
        // (0,2) is same square - not allowed
        let slide_from_2: Vec<_> = moves
            .iter()
            .filter(|m| matches!(m, Move::Slide { from, .. } if *from == Pos(2)))
            .collect();

        // Should only be able to slide to (0,0) or (0,1)
        for m in &slide_from_2 {
            if let Move::Slide { to, .. } = m {
                assert!(
                    *to == Pos(0) || *to == Pos(1),
                    "Invalid destination {:?} - must be in winning line",
                    to
                );
            }
        }

        // Large can gobble both Small and Medium
        assert_eq!(slide_from_2.len(), 2);
    }

    #[test]
    fn test_reveal_blocking_is_sufficient() {
        // Test 4.2: Blocking Is Sufficient
        // Same setup as 4.1
        let mut board = Board::new();
        board.push_piece(Pos(0), Player::Two, Size::Small);
        board.push_piece(Pos(1), Player::Two, Size::Medium);
        board.push_piece(Pos(2), Player::Two, Size::Small);
        board.push_piece(Pos(2), Player::One, Size::Large);

        let moves = board.legal_moves();

        // The move (0,2) → (0,1) should be legal (gobbles into winning line)
        let has_blocking_move = moves.iter().any(|m| {
            matches!(m, Move::Slide { from, to } if *from == Pos(2) && *to == Pos(1))
        });
        assert!(has_blocking_move, "Blocking move should be available");
    }

    #[test]
    fn test_reveal_no_save_possible() {
        // Test 4.3: No Save Possible (Piece Too Small)
        // Stack at (0,0) = P2 Large
        // Stack at (0,1) = P2 Large
        // Stack at (0,2) = [P2 Large, P1 Small]  (P1 Small on top of P2 Large)
        // Wait - that's invalid, Small can't gobble Large
        // Let me re-read...
        // Actually the test says P2 Large pieces in each cell, with P1 Small on top at (0,2)
        // But Small can't be placed on Large. Let me re-interpret:
        // The test describes a theoretical scenario. In practice, the setup would be:
        // - P2 Large visible at (0,0), (0,1)
        // - At (0,2): P2's piece (any size under), with P1 Small on top blocking
        // But Small can't be on top of Large...
        //
        // Looking more carefully at the test: it says "Stack at (0,2) = [P2 Large, P1 Small]"
        // which means P2 Large at bottom, P1 Small on top. But that's impossible since
        // Small can't cover Large.
        //
        // Let me use a valid setup that tests the same concept:
        // P2 visible at (0,0), (0,1) with Large pieces
        // P1 has a Small somewhere that would reveal P2's win

        let mut board = Board::new();
        // P2 Large at (0,0), (0,1) - visible, almost winning
        board.push_piece(Pos(0), Player::Two, Size::Large);
        board.push_piece(Pos(1), Player::Two, Size::Large);
        // P2 Medium at (0,2), P1 Small on top (blocks P2's row 0 win)
        board.push_piece(Pos(2), Player::Two, Size::Medium);
        board.push_piece(Pos(2), Player::One, Size::Small); // Small CAN gobble Medium? No!
        // Wait, Small cannot gobble Medium either.

        // Let me think about this differently:
        // The reveal scenario requires P1 to have a piece that's covering P2's potential win.
        // When P1 lifts that piece, P2 wins unless P1 can gobble back into the line.
        //
        // For "no save possible", we need P1's piece to be too small to gobble anything
        // in the revealed winning line.
        //
        // Setup: P2 has Medium pieces at (0,0), (0,1). At (0,2), there's a P2 Small
        // that P1's Medium is covering. If P1 lifts the Medium, P2 wins row 0.
        // P1 Medium can gobble P2's Smalls at (0,0), (0,1)? No, those are Mediums.
        // So P1 has no valid hail mary.

        // Actually, let me just set up what makes sense:
        let mut board = Board::new();
        // P2 Medium at (0,0), (0,1)
        board.push_piece(Pos(0), Player::Two, Size::Medium);
        board.push_piece(Pos(1), Player::Two, Size::Medium);
        // P2 Small at (0,2), with P1 Small on top (oops, Small can't gobble Small either)

        // OK I need to think about this more carefully. The only way to have a piece
        // that covers another is if the covering piece is LARGER.
        // So at (0,2), we could have P2 Small under P1 Medium/Large.
        // If P1 lifts, P2 wins row 0 (with 2 mediums + 1 small).
        // P1 can gobble: Medium can gobble Small. So P1 Medium could go to (0,2) but
        // that's same square. Can it gobble (0,0) or (0,1)? Medium can't gobble Medium.
        // So P1 Medium has no valid hail mary destinations!

        let mut board = Board::new();
        board.push_piece(Pos(0), Player::Two, Size::Medium);
        board.push_piece(Pos(1), Player::Two, Size::Medium);
        board.push_piece(Pos(2), Player::Two, Size::Small);
        board.push_piece(Pos(2), Player::One, Size::Medium); // P1 Medium covers P2 Small

        let moves = board.legal_moves();

        // P1 Medium at (0,2) - lifting reveals P2 row 0 win
        // P1 Medium cannot gobble Medium at (0,0) or (0,1)
        // P1 Medium could gobble Small at (0,2) but that's same-square
        // So there should be NO slide moves from (0,2)

        let slide_from_2: Vec<_> = moves
            .iter()
            .filter(|m| matches!(m, Move::Slide { from, .. } if *from == Pos(2)))
            .collect();

        assert_eq!(slide_from_2.len(), 0, "No valid hail mary should exist");
    }

    #[test]
    fn test_same_square_restriction() {
        // Test 4.4: Same-Square Restriction
        // Even if the source square is in the winning line, cannot return to it
        let mut board = Board::new();
        board.push_piece(Pos(0), Player::Two, Size::Small);
        board.push_piece(Pos(1), Player::Two, Size::Small);
        // At (0,2): P2 Small under P1 Large
        board.push_piece(Pos(2), Player::Two, Size::Small);
        board.push_piece(Pos(2), Player::One, Size::Large);

        let moves = board.legal_moves();

        // P1 Large at (0,2) - lifting reveals P2 row 0
        // (0,2) is IN the winning line, but P1 cannot place back there
        let slide_to_2: Vec<_> = moves
            .iter()
            .filter(|m| matches!(m, Move::Slide { from, to } if *from == Pos(2) && *to == Pos(2)))
            .collect();

        assert_eq!(slide_to_2.len(), 0, "Same-square moves should be blocked");
    }

    #[test]
    fn test_reveal_reserve_placements_still_legal() {
        // Test 4.8: Reserve Placement During Reveal Situation
        // Reserve placements don't trigger reveal and are always valid
        let mut board = Board::new();
        board.push_piece(Pos(0), Player::Two, Size::Small);
        board.push_piece(Pos(1), Player::Two, Size::Small);
        board.push_piece(Pos(2), Player::Two, Size::Small);
        board.push_piece(Pos(2), Player::One, Size::Large);

        let moves = board.legal_moves();

        // P1 should still have reserve placement moves
        let reserve_moves: Vec<_> = moves
            .iter()
            .filter(|m| matches!(m, Move::Place { .. }))
            .collect();

        // P1 still has reserves (1 Large used, has 2S, 2M, 1L left)
        // Can place S on 6 empty, M on 9, L on 6 (can gobble 3 smalls + 3 empty? no wait)
        // Actually: 6 empty cells, plus can gobble P2 Smalls with M or L
        // Let's just verify there are reserve moves
        assert!(reserve_moves.len() > 0, "Reserve placements should be available");
    }

    #[test]
    fn test_no_reveal_normal_moves() {
        // When there's no reveal, all normal board moves should be available
        let mut board = Board::new();
        board.push_piece(Pos(0), Player::One, Size::Large);
        board.push_piece(Pos(4), Player::Two, Size::Small);

        let moves = board.legal_moves();

        // P1 Large at (0,0) can move to 8 other positions
        // (no reveal since P2 only has 1 piece)
        let slide_moves: Vec<_> = moves
            .iter()
            .filter(|m| matches!(m, Move::Slide { .. }))
            .collect();

        assert_eq!(slide_moves.len(), 8);
    }

    #[test]
    fn test_legal_moves_equals_simple_no_reveal() {
        // When there's no reveal situation, legal_moves and legal_moves_simple should match
        let mut board = Board::new();
        board.push_piece(Pos(0), Player::One, Size::Large);
        board.push_piece(Pos(8), Player::Two, Size::Small);

        let full_moves = board.legal_moves();
        let simple_moves = board.legal_moves_simple();

        assert_eq!(full_moves.len(), simple_moves.len());
    }

    #[test]
    fn test_check_reveal() {
        let mut board = Board::new();
        // P2 about to win row 0 if P1 lifts from (0,2)
        board.push_piece(Pos(0), Player::Two, Size::Small);
        board.push_piece(Pos(1), Player::Two, Size::Small);
        board.push_piece(Pos(2), Player::Two, Size::Small);
        board.push_piece(Pos(2), Player::One, Size::Large);

        // Check reveal at (0,2) - should reveal P2's winning line
        let revealed = board.check_reveal(Pos(2));
        assert!(revealed.is_some());
        assert_eq!(revealed.unwrap(), [Pos(0), Pos(1), Pos(2)]);

        // Check reveal at (0,0) - P1 has no piece there, but function still works
        // Actually P1 doesn't have a piece at (0,0), so check_reveal wouldn't be called
        // Let's test a different position
        let revealed_empty = board.check_reveal(Pos(4));
        assert!(revealed_empty.is_none()); // No piece to reveal
    }

    #[test]
    fn test_reveal_multiple_pieces_one_restricted() {
        // P1 has multiple pieces, only one triggers reveal
        let mut board = Board::new();
        // P2 almost wins row 0
        board.push_piece(Pos(0), Player::Two, Size::Small);
        board.push_piece(Pos(1), Player::Two, Size::Small);
        board.push_piece(Pos(2), Player::Two, Size::Small);
        board.push_piece(Pos(2), Player::One, Size::Large); // Covers (0,2)

        // P1 also has a piece at (4) with no reveal issue
        board.push_piece(Pos(4), Player::One, Size::Small);

        let moves = board.legal_moves();

        // P1 Small at (4) should have full movement options
        let slide_from_4: Vec<_> = moves
            .iter()
            .filter(|m| matches!(m, Move::Slide { from, .. } if *from == Pos(4)))
            .collect();

        // Small at (4) can move to 8 positions (anywhere except where there's a larger piece)
        // Actually: (0), (1), (2) have P2 Small, but P1 Small can't gobble equal
        // (4) is current position
        // So destinations: (3), (5), (6), (7), (8) = 5 moves
        // Wait, and can it gobble P2 at (0), (1), (2)? No, Small can't gobble Small.
        assert_eq!(slide_from_4.len(), 5);

        // P1 Large at (2) should be restricted to hail mary
        let slide_from_2: Vec<_> = moves
            .iter()
            .filter(|m| matches!(m, Move::Slide { from, .. } if *from == Pos(2)))
            .collect();

        // Large can gobble Small at (0) and (1), but not (2) (same square)
        assert_eq!(slide_from_2.len(), 2);
    }

    // ========== Milestone 1.7: Apply & Undo Tests ==========

    #[test]
    fn test_apply_place_move() {
        let mut board = Board::new();
        assert_eq!(board.current_player(), Player::One);

        let mov = Move::Place { size: Size::Small, to: Pos(4) };
        let undo = board.apply(mov);

        // Board state changed
        assert_eq!(board.top_piece(Pos(4)), Some((Player::One, Size::Small)));
        assert_eq!(board.current_player(), Player::Two);

        // Undo info correct
        assert_eq!(undo.moved_size, Size::Small);
        assert_eq!(undo.captured, None);
        assert_eq!(undo.revealed, None);
    }

    #[test]
    fn test_apply_undo_place_roundtrip() {
        let mut board = Board::new();
        let original = board.0;

        let mov = Move::Place { size: Size::Medium, to: Pos(0) };
        let undo = board.apply(mov);

        assert_ne!(board.0, original);

        board.undo(&undo);
        assert_eq!(board.0, original);
    }

    #[test]
    fn test_apply_slide_move() {
        let mut board = Board::new();
        board.push_piece(Pos(0), Player::One, Size::Large);
        // Still P1's turn

        let original = board.0;
        let mov = Move::Slide { from: Pos(0), to: Pos(4) };
        let undo = board.apply(mov);

        // Piece moved
        assert_eq!(board.top_piece(Pos(0)), None);
        assert_eq!(board.top_piece(Pos(4)), Some((Player::One, Size::Large)));
        assert_eq!(board.current_player(), Player::Two);

        // Undo restores
        board.undo(&undo);
        assert_eq!(board.0, original);
    }

    #[test]
    fn test_apply_gobble_move() {
        let mut board = Board::new();
        board.push_piece(Pos(4), Player::Two, Size::Small);
        // P1's turn

        let mov = Move::Place { size: Size::Large, to: Pos(4) };
        let undo = board.apply(mov);

        // P1 Large on top, P2 Small underneath
        assert_eq!(board.top_piece(Pos(4)), Some((Player::One, Size::Large)));
        assert_eq!(board.piece_owner(Pos(4), Size::Small), Some(Player::Two));

        // Undo info recorded the captured piece
        assert_eq!(undo.captured, Some((Player::Two, Size::Small)));

        // Undo restores
        board.undo(&undo);
        assert_eq!(board.top_piece(Pos(4)), Some((Player::Two, Size::Small)));
        assert_eq!(board.current_player(), Player::One);
    }

    #[test]
    fn test_apply_slide_reveals_piece() {
        let mut board = Board::new();
        // Stack: P2 Small under P1 Large at (0,0)
        board.push_piece(Pos(0), Player::Two, Size::Small);
        board.push_piece(Pos(0), Player::One, Size::Large);
        // P1's turn

        let original = board.0;
        let mov = Move::Slide { from: Pos(0), to: Pos(4) };
        let undo = board.apply(mov);

        // Source now shows P2 Small
        assert_eq!(board.top_piece(Pos(0)), Some((Player::Two, Size::Small)));
        // Destination shows P1 Large
        assert_eq!(board.top_piece(Pos(4)), Some((Player::One, Size::Large)));

        // Undo info recorded the revealed piece
        assert_eq!(undo.revealed, Some((Player::Two, Size::Small)));

        // Undo restores
        board.undo(&undo);
        assert_eq!(board.0, original);
    }

    #[test]
    fn test_apply_slide_gobble_and_reveal() {
        let mut board = Board::new();
        // Source: P2 Small under P1 Medium
        board.push_piece(Pos(0), Player::Two, Size::Small);
        board.push_piece(Pos(0), Player::One, Size::Medium);
        // Destination: P2 Small
        board.push_piece(Pos(4), Player::Two, Size::Small);
        // P1's turn

        let original = board.0;
        let mov = Move::Slide { from: Pos(0), to: Pos(4) };
        let undo = board.apply(mov);

        // Source reveals P2 Small
        assert_eq!(board.top_piece(Pos(0)), Some((Player::Two, Size::Small)));
        // Destination: P1 Medium on top of P2 Small
        assert_eq!(board.top_piece(Pos(4)), Some((Player::One, Size::Medium)));
        assert_eq!(board.piece_owner(Pos(4), Size::Small), Some(Player::Two));

        // Undo info
        assert_eq!(undo.revealed, Some((Player::Two, Size::Small)));
        assert_eq!(undo.captured, Some((Player::Two, Size::Small)));

        // Undo restores
        board.undo(&undo);
        assert_eq!(board.0, original);
    }

    #[test]
    fn test_apply_undo_sequence() {
        let mut board = Board::new();
        let original = board.0;

        // Play a few moves
        let moves = [
            Move::Place { size: Size::Small, to: Pos(0) },
            Move::Place { size: Size::Small, to: Pos(4) },
            Move::Place { size: Size::Medium, to: Pos(8) },
            Move::Slide { from: Pos(0), to: Pos(1) },
        ];

        let mut undos = Vec::new();
        for &mov in &moves {
            undos.push(board.apply(mov));
        }

        // Undo in reverse order
        for undo in undos.iter().rev() {
            board.undo(undo);
        }

        assert_eq!(board.0, original);
    }

    #[test]
    fn test_apply_undo_all_initial_moves() {
        let board = Board::new();
        let moves = board.legal_moves();

        for mov in moves {
            let mut test_board = board;
            let undo = test_board.apply(mov);
            test_board.undo(&undo);
            assert_eq!(test_board.0, board.0, "Failed for move {:?}", mov);
        }
    }

    #[test]
    fn test_apply_undo_fuzz() {
        use rand::prelude::*;

        let mut rng = rand::rng();

        for _ in 0..100 {
            let mut board = Board::new();

            // Play random moves
            for _ in 0..10 {
                let moves = board.legal_moves();
                if moves.is_empty() || board.check_winner().is_some() {
                    break;
                }
                let mov = moves[rng.random_range(0..moves.len())];
                board.apply(mov);
            }

            // Now verify apply/undo works
            let moves = board.legal_moves();
            if !moves.is_empty() && board.check_winner().is_none() {
                let original = board.0;
                let mov = moves[rng.random_range(0..moves.len())];
                let undo = board.apply(mov);
                board.undo(&undo);
                assert_eq!(board.0, original, "Fuzz test failed for random position");
            }
        }
    }

    // ========== Milestone 1.8: Symmetry Tests ==========

    #[test]
    fn test_identity_transform() {
        let mut board = Board::new();
        board.push_piece(Pos(0), Player::One, Size::Small);
        board.push_piece(Pos(4), Player::Two, Size::Medium);

        // Identity should return same encoding
        assert_eq!(board.transform(0), board.0);
    }

    #[test]
    fn test_rotate_90() {
        let mut board = Board::new();
        board.push_piece(Pos(0), Player::One, Size::Small);

        // After 90° rotation, (0,0) -> (0,2) which is Pos(2)
        let rotated = Board::from_u64(board.transform(1));
        assert_eq!(rotated.top_piece(Pos(2)), Some((Player::One, Size::Small)));
        assert_eq!(rotated.top_piece(Pos(0)), None);
    }

    #[test]
    fn test_rotate_180() {
        let mut board = Board::new();
        board.push_piece(Pos(0), Player::One, Size::Small);

        // After 180° rotation, (0,0) -> (2,2) which is Pos(8)
        let rotated = Board::from_u64(board.transform(2));
        assert_eq!(rotated.top_piece(Pos(8)), Some((Player::One, Size::Small)));
        assert_eq!(rotated.top_piece(Pos(0)), None);
    }

    #[test]
    fn test_rotate_270() {
        let mut board = Board::new();
        board.push_piece(Pos(0), Player::One, Size::Small);

        // After 270° rotation, (0,0) -> (2,0) which is Pos(6)
        let rotated = Board::from_u64(board.transform(3));
        assert_eq!(rotated.top_piece(Pos(6)), Some((Player::One, Size::Small)));
        assert_eq!(rotated.top_piece(Pos(0)), None);
    }

    #[test]
    fn test_rotate_360_identity() {
        let mut board = Board::new();
        board.push_piece(Pos(0), Player::One, Size::Small);
        board.push_piece(Pos(4), Player::Two, Size::Medium);
        board.push_piece(Pos(8), Player::One, Size::Large);

        // 4 x 90° rotations should return to identity
        let mut current = board.0;
        for _ in 0..4 {
            current = Board::from_u64(current).transform(1);
        }
        assert_eq!(current, board.0);
    }

    #[test]
    fn test_reflect_horizontal() {
        let mut board = Board::new();
        board.push_piece(Pos(0), Player::One, Size::Small);

        // Horizontal reflection: (0,0) -> (0,2) which is Pos(2)
        let reflected = Board::from_u64(board.transform(4));
        assert_eq!(reflected.top_piece(Pos(2)), Some((Player::One, Size::Small)));
        assert_eq!(reflected.top_piece(Pos(0)), None);
    }

    #[test]
    fn test_reflect_vertical() {
        let mut board = Board::new();
        board.push_piece(Pos(0), Player::One, Size::Small);

        // Vertical reflection: (0,0) -> (2,0) which is Pos(6)
        let reflected = Board::from_u64(board.transform(5));
        assert_eq!(reflected.top_piece(Pos(6)), Some((Player::One, Size::Small)));
        assert_eq!(reflected.top_piece(Pos(0)), None);
    }

    #[test]
    fn test_reflect_twice_identity() {
        let mut board = Board::new();
        board.push_piece(Pos(0), Player::One, Size::Small);
        board.push_piece(Pos(1), Player::Two, Size::Medium);

        // Any reflection applied twice should be identity
        for t in 4..8 {
            let once = Board::from_u64(board.transform(t));
            let twice = once.transform(t);
            assert_eq!(twice, board.0, "Transform {} applied twice should be identity", t);
        }
    }

    #[test]
    fn test_center_invariant() {
        let mut board = Board::new();
        board.push_piece(Pos(4), Player::One, Size::Small);

        // Center should be invariant under all transforms
        for t in 0..8 {
            let transformed = Board::from_u64(board.transform(t));
            assert_eq!(
                transformed.top_piece(Pos(4)),
                Some((Player::One, Size::Small)),
                "Transform {} should preserve center piece",
                t
            );
        }
    }

    #[test]
    fn test_all_corners_same_canonical() {
        // Boards with a single piece at each corner should have same canonical form
        let corners = [Pos(0), Pos(2), Pos(6), Pos(8)];
        let mut canonicals = Vec::new();

        for corner in corners {
            let mut board = Board::new();
            board.push_piece(corner, Player::One, Size::Small);
            canonicals.push(board.canonical());
        }

        // All should be the same
        for c in &canonicals[1..] {
            assert_eq!(*c, canonicals[0], "All corner positions should have same canonical");
        }
    }

    #[test]
    fn test_all_edges_same_canonical() {
        // Boards with a single piece at each edge midpoint should have same canonical form
        let edges = [Pos(1), Pos(3), Pos(5), Pos(7)];
        let mut canonicals = Vec::new();

        for edge in edges {
            let mut board = Board::new();
            board.push_piece(edge, Player::One, Size::Small);
            canonicals.push(board.canonical());
        }

        // All should be the same
        for c in &canonicals[1..] {
            assert_eq!(*c, canonicals[0], "All edge positions should have same canonical");
        }
    }

    #[test]
    fn test_canonical_is_minimum() {
        let mut board = Board::new();
        board.push_piece(Pos(0), Player::One, Size::Small);
        board.push_piece(Pos(1), Player::Two, Size::Medium);

        let symmetries = board.all_symmetries();
        let canonical = board.canonical();
        let min = *symmetries.iter().min().unwrap();

        assert_eq!(canonical, min);
    }

    #[test]
    fn test_canonical_idempotent() {
        let mut board = Board::new();
        board.push_piece(Pos(0), Player::One, Size::Small);
        board.push_piece(Pos(4), Player::Two, Size::Medium);

        let canonical = board.canonical();
        let canonical_board = Board::from_u64(canonical);
        let double_canonical = canonical_board.canonical();

        assert_eq!(canonical, double_canonical);
    }

    #[test]
    fn test_all_symmetries_count() {
        let mut board = Board::new();
        board.push_piece(Pos(0), Player::One, Size::Small);
        board.push_piece(Pos(1), Player::Two, Size::Medium);

        let symmetries = board.all_symmetries();

        // Should have 8 transformations
        assert_eq!(symmetries.len(), 8);
    }

    #[test]
    fn test_transform_preserves_player() {
        let mut board = Board::new();
        board.push_piece(Pos(0), Player::One, Size::Small);
        board.switch_player(); // P2's turn

        for t in 0..8 {
            let transformed = Board::from_u64(board.transform(t));
            assert_eq!(
                transformed.current_player(),
                Player::Two,
                "Transform {} should preserve player",
                t
            );
        }
    }

    #[test]
    fn test_canonical_fuzz() {
        use rand::prelude::*;

        let mut rng = rand::rng();

        for _ in 0..50 {
            let mut board = Board::new();

            // Play some random moves
            for _ in 0..5 {
                let moves = board.legal_moves();
                if moves.is_empty() || board.check_winner().is_some() {
                    break;
                }
                let mov = moves[rng.random_range(0..moves.len())];
                board.apply(mov);
            }

            // Canonical should be minimum of all symmetries
            let canonical = board.canonical();
            for t in 0..8 {
                let sym = board.transform(t);
                assert!(canonical <= sym, "Canonical {} should be <= transform {} ({})", canonical, t, sym);
            }

            // Canonical should be idempotent
            let double = Board::from_u64(canonical).canonical();
            assert_eq!(canonical, double, "Canonical should be idempotent");
        }
    }

    // ========== Packed Move/Undo Tests ==========

    #[test]
    fn test_packed_move_place() {
        let mov = PackedMove::place(Size::Small, 4);
        assert!(mov.is_place());
        assert_eq!(mov.to(), 4);
        assert_eq!(mov.reserve_size(), Some(Size::Small));
        assert_eq!(mov.from_pos(), None);
    }

    #[test]
    fn test_packed_move_slide() {
        let mov = PackedMove::slide(0, 8);
        assert!(!mov.is_place());
        assert_eq!(mov.to(), 8);
        assert_eq!(mov.from_pos(), Some(0));
        assert_eq!(mov.source(), 0);
        assert_eq!(mov.reserve_size(), None);
    }

    #[test]
    fn test_packed_move_conversion() {
        // Place move
        let place = Move::Place { size: Size::Large, to: Pos(5) };
        let packed = PackedMove::from_move(place);
        let back = packed.to_move();
        assert_eq!(back, place);

        // Slide move
        let slide = Move::Slide { from: Pos(0), to: Pos(8) };
        let packed = PackedMove::from_move(slide);
        let back = packed.to_move();
        assert_eq!(back, slide);
    }

    #[test]
    fn test_packed_undo_encoding() {
        let mov = PackedMove::slide(2, 5);
        let captured = Some((Player::One, Size::Medium));
        let revealed = Some((Player::Two, Size::Small));

        let undo = PackedUndo::new(mov, captured, revealed);

        assert_eq!(undo.mov(), mov);
        assert_eq!(undo.captured(), captured);
        assert_eq!(undo.revealed(), revealed);
    }

    #[test]
    fn test_packed_undo_none_values() {
        let mov = PackedMove::place(Size::Small, 0);
        let undo = PackedUndo::new(mov, None, None);

        assert_eq!(undo.captured(), None);
        assert_eq!(undo.revealed(), None);
    }

    #[test]
    fn test_move_list_basic() {
        let mut list = MoveList::new();
        assert!(list.is_empty());
        assert_eq!(list.len(), 0);

        list.push(PackedMove::place(Size::Small, 0));
        list.push(PackedMove::slide(4, 5));

        assert_eq!(list.len(), 2);
        assert!(!list.is_empty());
        assert_eq!(list.get(0), PackedMove::place(Size::Small, 0));
        assert_eq!(list.get(1), PackedMove::slide(4, 5));
    }

    #[test]
    fn test_legal_moves_packed_matches_legal_moves() {
        let board = Board::new();

        let vec_moves = board.legal_moves();
        let packed_moves = board.legal_moves_packed();

        assert_eq!(vec_moves.len(), packed_moves.len());

        // Convert all packed moves to regular moves and compare sets
        let vec_set: std::collections::HashSet<_> = vec_moves.iter().map(|m| {
            match m {
                Move::Place { size, to } => (true, *size as u8, 0, to.0),
                Move::Slide { from, to } => (false, 0, from.0, to.0),
            }
        }).collect();

        let packed_set: std::collections::HashSet<_> = packed_moves.iter().map(|m| {
            if m.is_place() {
                (true, m.reserve_size().unwrap() as u8, 0, m.to())
            } else {
                (false, 0, m.source(), m.to())
            }
        }).collect();

        assert_eq!(vec_set, packed_set);
    }

    #[test]
    fn test_apply_packed_place() {
        let mut board = Board::new();
        let mov = PackedMove::place(Size::Medium, 4);

        let undo = board.apply_packed(mov);

        assert_eq!(board.top_piece(Pos(4)), Some((Player::One, Size::Medium)));
        assert_eq!(board.current_player(), Player::Two);
        assert_eq!(undo.captured(), None);
    }

    #[test]
    fn test_apply_undo_packed_roundtrip() {
        let mut board = Board::new();
        let original = board.0;

        let mov = PackedMove::place(Size::Large, 0);
        let undo = board.apply_packed(mov);

        assert_ne!(board.0, original);

        board.undo_packed(undo);
        assert_eq!(board.0, original);
    }

    #[test]
    fn test_apply_packed_slide() {
        let mut board = Board::new();
        board.push_piece(Pos(0), Player::One, Size::Large);

        let original = board.0;
        let mov = PackedMove::slide(0, 4);
        let undo = board.apply_packed(mov);

        assert_eq!(board.top_piece(Pos(0)), None);
        assert_eq!(board.top_piece(Pos(4)), Some((Player::One, Size::Large)));

        board.undo_packed(undo);
        assert_eq!(board.0, original);
    }

    #[test]
    fn test_apply_packed_gobble() {
        let mut board = Board::new();
        board.push_piece(Pos(4), Player::Two, Size::Small);

        let mov = PackedMove::place(Size::Large, 4);
        let undo = board.apply_packed(mov);

        assert_eq!(board.top_piece(Pos(4)), Some((Player::One, Size::Large)));
        assert_eq!(undo.captured(), Some((Player::Two, Size::Small)));
    }

    #[test]
    fn test_packed_fuzz() {
        use rand::prelude::*;

        let mut rng = rand::rng();

        for _ in 0..100 {
            let mut board = Board::new();

            // Play random moves using packed API
            for _ in 0..10 {
                let moves = board.legal_moves_packed();
                if moves.is_empty() || board.check_winner().is_some() {
                    break;
                }
                let idx = rng.random_range(0..moves.len());
                let mov = moves.get(idx);
                board.apply_packed(mov);
            }

            // Verify apply/undo packed works
            let moves = board.legal_moves_packed();
            if !moves.is_empty() && board.check_winner().is_none() {
                let original = board.0;
                let idx = rng.random_range(0..moves.len());
                let mov = moves.get(idx);
                let undo = board.apply_packed(mov);
                board.undo_packed(undo);
                assert_eq!(board.0, original, "Packed fuzz test failed");
            }
        }
    }

    #[test]
    fn test_packed_matches_regular_apply() {
        use rand::prelude::*;

        let mut rng = rand::rng();

        // Play the same random game with both APIs and verify they match
        for _ in 0..50 {
            let mut board1 = Board::new();
            let mut board2 = Board::new();

            for _ in 0..8 {
                let moves1 = board1.legal_moves();
                let _moves2 = board2.legal_moves_packed();

                if moves1.is_empty() || board1.check_winner().is_some() {
                    break;
                }

                // Pick same random move
                let idx = rng.random_range(0..moves1.len());
                let mov = moves1[idx];
                let packed_mov = PackedMove::from_move(mov);

                board1.apply(mov);
                board2.apply_packed(packed_mov);

                // States should match
                assert_eq!(board1.0, board2.0, "Board states diverged");
            }
        }
    }

    // ========== Bitboard Win Detection Tests ==========

    #[test]
    fn test_visibility_masks_empty() {
        let board = Board::new();
        let (p1, p2) = board.visibility_masks();
        assert_eq!(p1, 0);
        assert_eq!(p2, 0);
    }

    #[test]
    fn test_visibility_masks_single_piece() {
        let mut board = Board::new();
        board.push_piece(Pos(4), Player::One, Size::Small);

        let (p1, p2) = board.visibility_masks();
        assert_eq!(p1, 1 << 4); // Center bit set
        assert_eq!(p2, 0);
    }

    #[test]
    fn test_visibility_masks_gobbled() {
        let mut board = Board::new();
        // P1 Small at center, P2 Large covers it
        board.push_piece(Pos(4), Player::One, Size::Small);
        board.push_piece(Pos(4), Player::Two, Size::Large);

        let (p1, p2) = board.visibility_masks();
        assert_eq!(p1, 0); // P1 is hidden
        assert_eq!(p2, 1 << 4); // P2 visible at center
    }

    #[test]
    fn test_check_winner_fast_row() {
        let mut board = Board::new();
        board.push_piece(Pos(0), Player::One, Size::Small);
        board.push_piece(Pos(1), Player::One, Size::Medium);
        board.push_piece(Pos(2), Player::One, Size::Large);

        assert_eq!(board.check_winner_fast(), Some(Player::One));
        assert_eq!(board.check_winner_fast(), board.check_winner());
    }

    #[test]
    fn test_check_winner_fast_column() {
        let mut board = Board::new();
        board.push_piece(Pos(0), Player::Two, Size::Large);
        board.push_piece(Pos(3), Player::Two, Size::Medium);
        board.push_piece(Pos(6), Player::Two, Size::Small);

        assert_eq!(board.check_winner_fast(), Some(Player::Two));
        assert_eq!(board.check_winner_fast(), board.check_winner());
    }

    #[test]
    fn test_check_winner_fast_diagonal() {
        let mut board = Board::new();
        board.push_piece(Pos(0), Player::One, Size::Large);
        board.push_piece(Pos(4), Player::One, Size::Medium);
        board.push_piece(Pos(8), Player::One, Size::Small);

        assert_eq!(board.check_winner_fast(), Some(Player::One));
    }

    #[test]
    fn test_check_winner_fast_no_winner() {
        let mut board = Board::new();
        board.push_piece(Pos(0), Player::One, Size::Small);
        board.push_piece(Pos(1), Player::Two, Size::Medium);
        board.push_piece(Pos(2), Player::One, Size::Large);

        assert_eq!(board.check_winner_fast(), None);
    }

    #[test]
    fn test_has_won_fast_matches_has_won() {
        let mut board = Board::new();
        board.push_piece(Pos(2), Player::Two, Size::Small);
        board.push_piece(Pos(4), Player::Two, Size::Medium);
        board.push_piece(Pos(6), Player::Two, Size::Large);

        assert_eq!(board.has_won_fast(Player::One), board.has_won(Player::One));
        assert_eq!(board.has_won_fast(Player::Two), board.has_won(Player::Two));
        assert!(board.has_won_fast(Player::Two));
    }

    #[test]
    fn test_bitboard_win_all_lines() {
        // Test each of the 8 winning lines with fast detection
        let lines = [
            [0, 1, 2], // Row 0
            [3, 4, 5], // Row 1
            [6, 7, 8], // Row 2
            [0, 3, 6], // Col 0
            [1, 4, 7], // Col 1
            [2, 5, 8], // Col 2
            [0, 4, 8], // Main diagonal
            [2, 4, 6], // Anti-diagonal
        ];

        for (i, line) in lines.iter().enumerate() {
            let mut board = Board::new();
            for &pos in line {
                board.push_piece(Pos(pos), Player::One, Size::Small);
            }
            assert!(
                board.has_won_fast(Player::One),
                "Failed for line {}: {:?}",
                i,
                line
            );
            assert_eq!(
                board.check_winner_fast(),
                Some(Player::One),
                "check_winner_fast failed for line {}: {:?}",
                i,
                line
            );
        }
    }

    #[test]
    fn test_bitboard_fuzz_matches_original() {
        use rand::prelude::*;

        let mut rng = rand::rng();

        for _ in 0..100 {
            let mut board = Board::new();

            // Play random moves
            for _ in 0..10 {
                let moves = board.legal_moves();
                if moves.is_empty() {
                    break;
                }

                // Check that fast and original match BEFORE checking winner
                assert_eq!(
                    board.check_winner_fast(),
                    board.check_winner(),
                    "check_winner mismatch"
                );
                assert_eq!(
                    board.has_won_fast(Player::One),
                    board.has_won(Player::One),
                    "has_won P1 mismatch"
                );
                assert_eq!(
                    board.has_won_fast(Player::Two),
                    board.has_won(Player::Two),
                    "has_won P2 mismatch"
                );

                if board.check_winner().is_some() {
                    break;
                }

                let mov = moves[rng.random_range(0..moves.len())];
                board.apply(mov);
            }
        }
    }

    // ========== top_piece_fast Tests ==========

    #[test]
    fn test_top_piece_fast_empty() {
        let board = Board::new();
        for pos in 0..9 {
            assert_eq!(board.top_piece_fast(Pos(pos)), None);
            assert_eq!(board.top_piece_fast(Pos(pos)), board.top_piece(Pos(pos)));
        }
    }

    #[test]
    fn test_top_piece_fast_single() {
        let mut board = Board::new();
        board.push_piece(Pos(4), Player::One, Size::Medium);

        assert_eq!(board.top_piece_fast(Pos(4)), Some((Player::One, Size::Medium)));
        assert_eq!(board.top_piece_fast(Pos(4)), board.top_piece(Pos(4)));
    }

    #[test]
    fn test_top_piece_fast_stacked() {
        let mut board = Board::new();
        board.push_piece(Pos(0), Player::One, Size::Small);
        board.push_piece(Pos(0), Player::Two, Size::Medium);
        board.push_piece(Pos(0), Player::One, Size::Large);

        // Top should be Large
        assert_eq!(board.top_piece_fast(Pos(0)), Some((Player::One, Size::Large)));
        assert_eq!(board.top_piece_fast(Pos(0)), board.top_piece(Pos(0)));
    }

    #[test]
    fn test_top_piece_fast_fuzz() {
        use rand::prelude::*;

        let mut rng = rand::rng();

        for _ in 0..100 {
            let mut board = Board::new();

            for _ in 0..10 {
                let moves = board.legal_moves();
                if moves.is_empty() || board.check_winner().is_some() {
                    break;
                }

                // Verify top_piece_fast matches top_piece for all positions
                for pos in 0..9 {
                    assert_eq!(
                        board.top_piece_fast(Pos(pos)),
                        board.top_piece(Pos(pos)),
                        "top_piece mismatch at pos {}", pos
                    );
                }

                let mov = moves[rng.random_range(0..moves.len())];
                board.apply(mov);
            }
        }
    }

    // ========== Additional Coverage Tests (from game_logic_testing.md) ==========

    #[test]
    fn test_self_gobble() {
        // 2.3: Player can gobble their own piece
        let mut board = Board::new();
        board.push_piece(Pos(4), Player::One, Size::Small);

        // P1 should be able to place Large on top of their own Small
        let moves = board.legal_moves();
        let gobble_self = moves.iter().any(|m| {
            matches!(m, Move::Place { size: Size::Large, to } if to.0 == 4)
        });
        assert!(gobble_self, "Should be able to gobble own piece");

        // Apply the gobble
        board.apply(Move::Place { size: Size::Large, to: Pos(4) });

        // Stack should have both pieces
        assert_eq!(board.top_piece(Pos(4)), Some((Player::One, Size::Large)));
        // Small is underneath (verify by popping)
        let mut board2 = board;
        board2.pop_top(Pos(4));
        assert_eq!(board2.top_piece(Pos(4)), Some((Player::One, Size::Small)));
    }

    #[test]
    fn test_win_on_move() {
        // 3.6: Win is detected immediately after completing a line
        let mut board = Board::new();

        // P1 has two in a row
        board.push_piece(Pos(0), Player::One, Size::Small);
        board.push_piece(Pos(1), Player::One, Size::Medium);

        assert_eq!(board.check_winner(), None, "No winner yet");

        // P1 completes the row
        board.push_piece(Pos(2), Player::One, Size::Large);

        assert_eq!(board.check_winner(), Some(Player::One), "P1 should win");
    }

    #[test]
    fn test_reveal_own_win_blocked_by_opponent() {
        // 4.5: Can't move to create own win if it reveals opponent's win first
        let mut board = Board::new();

        // P2's hidden row: P2 at (0,0), (0,1), and P2 under P1 at (0,2)
        board.push_piece(Pos(0), Player::Two, Size::Small);
        board.push_piece(Pos(1), Player::Two, Size::Small);
        board.push_piece(Pos(2), Player::Two, Size::Small);
        board.push_piece(Pos(2), Player::One, Size::Large); // P1 covers (0,2)

        // P1's potential diagonal: (1,1) and (2,0), needs (2,2) to win
        board.push_piece(Pos(4), Player::One, Size::Medium);
        board.push_piece(Pos(6), Player::One, Size::Medium);

        // P1 to move - can they move Large from (0,2) to (2,2)?
        let moves = board.legal_moves();
        let can_move_to_22 = moves.iter().any(|m| {
            matches!(m, Move::Slide { from, to } if from.0 == 2 && to.0 == 8)
        });

        // Should NOT be allowed - lifting reveals P2's row 0 win
        // (2,2) is not in row 0, so it's not a valid hail mary destination
        assert!(!can_move_to_22, "Should not be able to move to (2,2) when it reveals opponent win");
    }

    #[test]
    fn test_zugzwang_from_reveal() {
        // 4.7: Player has no legal moves due to reveal rule
        let mut board = Board::new();

        // P2 has row 0 with Large pieces, P1 Small on top of (0,2)
        board.push_piece(Pos(0), Player::Two, Size::Large);
        board.push_piece(Pos(1), Player::Two, Size::Large);
        board.push_piece(Pos(2), Player::Two, Size::Large);
        board.push_piece(Pos(2), Player::One, Size::Small);

        // Place all of P1's other pieces so they have no reserves
        board.push_piece(Pos(3), Player::One, Size::Small);
        board.push_piece(Pos(4), Player::One, Size::Medium);
        board.push_piece(Pos(5), Player::One, Size::Medium);
        board.push_piece(Pos(6), Player::One, Size::Large);
        board.push_piece(Pos(7), Player::One, Size::Large);

        // P1 to move - their only piece that can move is the Small at (0,2)
        // But lifting it reveals P2's row 0, and Small can't gobble any Large
        let moves = board.legal_moves();

        // P1 should have very limited or no moves from (0,2)
        let moves_from_02: Vec<_> = moves.iter().filter(|m| {
            matches!(m, Move::Slide { from, .. } if from.0 == 2)
        }).collect();

        assert!(moves_from_02.is_empty(), "Small cannot gobble Large in winning line");
    }

    #[test]
    fn test_full_stacks_encoding() {
        // 5.5: All 9 cells have 3 pieces stacked
        let mut board = Board::new();

        // This is a contrived state - fill every cell with a full stack
        // We'll alternate players and go S, M, L at each cell
        for pos in 0..9 {
            let p1 = if pos % 2 == 0 { Player::One } else { Player::Two };
            let p2 = p1.opponent();
            board.push_piece(Pos(pos), p1, Size::Small);
            board.push_piece(Pos(pos), p2, Size::Medium);
            board.push_piece(Pos(pos), p1, Size::Large);
        }

        // Encode and decode
        let encoded = board.0;
        let decoded = Board::from_u64(encoded);

        // Verify all cells match
        for pos in 0..9 {
            assert_eq!(
                board.top_piece(Pos(pos)),
                decoded.top_piece(Pos(pos)),
                "Mismatch at pos {}", pos
            );
        }
    }

    #[test]
    fn test_full_board() {
        // Edge case: All 12 pieces placed on board
        let mut board = Board::new();

        // Place all P1 pieces
        board.push_piece(Pos(0), Player::One, Size::Small);
        board.push_piece(Pos(1), Player::One, Size::Small);
        board.push_piece(Pos(2), Player::One, Size::Medium);
        board.push_piece(Pos(3), Player::One, Size::Medium);
        board.push_piece(Pos(4), Player::One, Size::Large);
        board.push_piece(Pos(5), Player::One, Size::Large);

        // Place all P2 pieces (gobbling some P1 pieces)
        board.push_piece(Pos(0), Player::Two, Size::Medium); // Gobbles P1 S
        board.push_piece(Pos(1), Player::Two, Size::Large);  // Gobbles P1 S
        board.push_piece(Pos(6), Player::Two, Size::Small);
        board.push_piece(Pos(7), Player::Two, Size::Small);
        board.push_piece(Pos(8), Player::Two, Size::Medium);
        board.push_piece(Pos(2), Player::Two, Size::Large);  // Gobbles P1 M

        // Both players should have empty reserves
        assert_eq!(board.reserves(Player::One), [0, 0, 0]);
        assert_eq!(board.reserves(Player::Two), [0, 0, 0]);

        // Legal moves should only be board moves (no placements)
        let moves = board.legal_moves();
        let has_placement = moves.iter().any(|m| matches!(m, Move::Place { .. }));
        assert!(!has_placement, "Should have no placement moves with full board");
    }

    #[test]
    fn test_all_pieces_one_cell() {
        // Edge case: All pieces stacked on one cell
        let mut board = Board::new();

        // Stack all pieces at center (only 3 can physically fit due to sizes)
        board.push_piece(Pos(4), Player::One, Size::Small);
        board.push_piece(Pos(4), Player::Two, Size::Medium);
        board.push_piece(Pos(4), Player::One, Size::Large);

        // Verify stack
        assert_eq!(board.top_piece(Pos(4)), Some((Player::One, Size::Large)));

        // Pop and verify
        board.pop_top(Pos(4));
        assert_eq!(board.top_piece(Pos(4)), Some((Player::Two, Size::Medium)));

        board.pop_top(Pos(4));
        assert_eq!(board.top_piece(Pos(4)), Some((Player::One, Size::Small)));

        board.pop_top(Pos(4));
        assert_eq!(board.top_piece(Pos(4)), None);
    }

    #[test]
    fn test_win_by_gobble() {
        // Edge case: Win created by gobbling (piece on top completes line)
        let mut board = Board::new();

        // P1 has two in row 0
        board.push_piece(Pos(0), Player::One, Size::Small);
        board.push_piece(Pos(1), Player::One, Size::Medium);

        // P2 has a piece at (0,2)
        board.push_piece(Pos(2), Player::Two, Size::Small);

        assert_eq!(board.check_winner(), None, "No winner yet");

        // P1 gobbles P2's piece to complete the row
        board.push_piece(Pos(2), Player::One, Size::Large);

        assert_eq!(board.check_winner(), Some(Player::One), "P1 wins by gobbling");
    }
}
