# Bit-Level Game Representation

This document explains the bit-level representation used in the V2 Rust solver for Gobblet Gobblers. Every aspect of the game—board state, moves, undo information, and win detection—is encoded in compact bit formats for maximum performance.

**Key file:** `v2/gobblet-core/src/lib.rs`

---

## Table of Contents

1. [Design Philosophy](#1-design-philosophy)
2. [Board Encoding (64 bits)](#2-board-encoding-64-bits)
3. [Move Encoding (8 bits)](#3-move-encoding-8-bits)
4. [Undo Encoding (16 bits)](#4-undo-encoding-16-bits)
5. [Fixed-Size Move List](#5-fixed-size-move-list)
6. [Win Detection Masks](#6-win-detection-masks)
7. [Visibility Masks](#7-visibility-masks)
8. [Symmetry Transformations](#8-symmetry-transformations)
9. [Performance Comparison](#9-performance-comparison)
10. [API Reference](#10-api-reference)

---

## 1. Design Philosophy

### Why Bits Matter

When exploring 1 billion+ game states, every byte and every CPU cycle counts:

| Concern | Object-Based (Python) | Bit-Based (Rust) |
|---------|----------------------|------------------|
| Board state size | ~500+ bytes (objects, refs, GC overhead) | 8 bytes (u64) |
| Move representation | ~100+ bytes per move object | 1 byte (u8) |
| Undo information | ~200+ bytes with captured piece objects | 2 bytes (u16) |
| Move list | Heap allocation per game state | Stack-allocated, no heap |
| Win detection | Loop + method calls + option unwrapping | Bitmask AND + compare |
| Memory for 1B states | ~500+ GB | ~8 GB |

### Core Principles

1. **Everything fits in registers**: Board (u64), Move (u8), Undo (u16) all fit in CPU registers
2. **No heap allocation in hot paths**: Move generation uses fixed-size stack arrays
3. **Bit operations instead of branches**: Win detection uses mask comparisons
4. **Derive rather than store**: Reserves computed from board, not stored separately

---

## 2. Board Encoding (64 bits)

The entire game state fits in a single `u64`:

```
Bits 0-53:  Board state (9 cells × 6 bits per cell)
Bit 54:     Current player (0 = Player 1, 1 = Player 2)
Bits 55-63: Unused (zero for canonical form)
```

### Cell Layout (6 bits each)

Each cell can hold one piece of each size (stacked via gobbling):

```
Bits 0-1: Small piece owner  (0=empty, 1=P1, 2=P2)
Bits 2-3: Medium piece owner (0=empty, 1=P1, 2=P2)
Bits 4-5: Large piece owner  (0=empty, 1=P1, 2=P2)
```

### Cell Indexing (Row-Major Order)

```
Position indices:     Bit ranges:
  0  1  2             Cell 0: bits 0-5
  3  4  5             Cell 1: bits 6-11
  6  7  8             Cell 2: bits 12-17
                      ...
                      Cell 8: bits 48-53
                      Player: bit 54
```

### Example Encoding

Board with P1 Small under P2 Large at position 0:

```
Cell 0 = small:P1, medium:empty, large:P2
       = 0b01 | (0b00 << 2) | (0b10 << 4)
       = 0b100001
       = 33

Full board: 33 | (other_cells << 6) | (player_bit << 54)
```

### Why Not Store Reserves?

Reserves are **derived** from the board state:
- Each player starts with 2 of each size (6 pieces total)
- Count pieces on board per size, subtract from 2

```rust
pub fn reserves(&self, player: Player) -> [u8; 3] {
    let on_board = self.pieces_on_board(player);
    [2 - on_board[0], 2 - on_board[1], 2 - on_board[2]]
}
```

**Benefit**: No redundant data, no consistency bugs, smaller encoding.

---

## 3. Move Encoding (8 bits)

Moves are encoded in a single byte (`PackedMove`):

```
Bits 0-3: Destination position (0-8)
Bits 4-7: Source (0-8 for board move, 9-11 for reserve placement)
```

### Source Encoding

| Source Value | Meaning |
|--------------|---------|
| 0-8 | Board position (slide move) |
| 9 | Small from reserve |
| 10 | Medium from reserve |
| 11 | Large from reserve |

### Examples

```rust
// Place Small at center (position 4)
PackedMove::place(Size::Small, 4)  // = (9 << 4) | 4 = 0x94

// Slide from corner (0) to center (4)
PackedMove::slide(0, 4)            // = (0 << 4) | 4 = 0x04
```

### Why 8 Bits?

| Representation | Size | Notes |
|---------------|------|-------|
| Rust enum `Move` | 4+ bytes | Enum tag + data |
| Python object | 100+ bytes | Object overhead |
| **PackedMove (u8)** | **1 byte** | Fits in register |

With 40+ possible moves per position, saving 3+ bytes per move adds up fast.

---

## 4. Undo Encoding (16 bits)

Undo information for backtracking during search fits in 2 bytes (`PackedUndo`):

```
Bits 0-7:   The move (PackedMove)
Bits 8-10:  Captured piece (3 bits)
Bits 11-13: Revealed piece (3 bits)
Bits 14-15: Unused
```

### Piece Encoding (3 bits)

Each piece is encoded as:

| Value | Meaning |
|-------|---------|
| 0 | No piece |
| 1 | Player 1 Small |
| 2 | Player 1 Medium |
| 3 | Player 1 Large |
| 4 | Player 2 Small |
| 5 | Player 2 Medium |
| 6 | Player 2 Large |

Formula: `(player - 1) * 3 + size + 1` (0 reserved for "none")

### What Gets Stored

- **Captured**: The piece that was covered at the destination (for gobble moves)
- **Revealed**: The piece that became visible at the source (for slide moves)

### Example

Slide from position 2 to 5, capturing P2 Small, revealing P1 Medium:

```rust
let undo = PackedUndo::new(
    PackedMove::slide(2, 5),        // bits 0-7
    Some((Player::Two, Size::Small)),  // bits 8-10 = 4
    Some((Player::One, Size::Medium)), // bits 11-13 = 2
);
// Result: 0x1225 = 0b0001_0010_0010_0101
```

### Why Not Store More?

The moved piece's size can be derived:
- For placements: from the source encoding (9=Small, 10=Medium, 11=Large)
- For slides: from the destination after apply (the piece is now there)

---

## 5. Fixed-Size Move List

Instead of heap-allocating a `Vec<Move>`, we use a stack-allocated array:

```rust
pub const MAX_MOVES: usize = 64;

pub struct MoveList {
    moves: [PackedMove; MAX_MOVES],  // 64 bytes on stack
    len: u8,                          // 1 byte
}
```

### Why 64?

Maximum theoretical moves:
- 3 sizes × 9 positions = 27 reserve placements (but limited by reserves)
- 9 pieces × 8 destinations = 72 board moves (but limited by what's on board)

In practice, max is ~40-50. 64 provides headroom without wasting much stack space.

### Memory Comparison

| Approach | Memory per call | Allocation |
|----------|-----------------|------------|
| `Vec<Move>` | 24 bytes header + heap | Heap alloc/free |
| **MoveList** | **65 bytes on stack** | **No allocation** |

For minimax with millions of recursive calls, avoiding heap allocation is critical.

---

## 6. Win Detection Masks

Win detection uses precomputed bitmasks for the 8 winning lines:

```rust
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
```

### How It Works

1. Compute visibility mask for each player (which cells they control)
2. For each win mask, check: `(player_mask & win_mask) == win_mask`

```rust
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
```

### Performance Comparison

| Approach | Operations |
|----------|-----------|
| Loop + top_piece() calls | 8 lines × 3 cells × top_piece overhead |
| **Bitmask comparison** | **Compute masks once, 8 AND+CMP operations** |

The bitmask approach is branchless within each check, better for CPU pipelining.

---

## 7. Visibility Masks

A visibility mask is a 9-bit value indicating which cells a player controls:

```rust
pub fn visibility_masks(&self) -> (u16, u16) {
    let mut p1_mask = 0u16;
    let mut p2_mask = 0u16;

    for pos in 0..9 {
        let owner = self.top_owner_bits(pos);  // 0, 1, or 2
        if owner == 1 { p1_mask |= 1 << pos; }
        else if owner == 2 { p2_mask |= 1 << pos; }
    }
    (p1_mask, p2_mask)
}
```

### Top Owner Detection (Optimized)

Instead of looping through sizes:

```rust
fn top_owner_bits(&self, pos: u8) -> u8 {
    let cell = self.cell(Pos(pos));
    let large = (cell >> 4) & 3;
    let medium = (cell >> 2) & 3;
    let small = cell & 3;

    // Priority: Large > Medium > Small
    if large != 0 { large as u8 }
    else if medium != 0 { medium as u8 }
    else { small as u8 }
}
```

This avoids the loop and `Size::from_index()` call in the general `top_piece()`.

---

## 8. Symmetry Transformations

The 3×3 board has D₄ symmetry (8 transformations). All operate directly on the u64 encoding.

### Transform Lookup Table

```rust
const TRANSFORMS: [[u8; 9]; 8] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8], // Identity
    [6, 3, 0, 7, 4, 1, 8, 5, 2], // Rotate 90° CW
    [8, 7, 6, 5, 4, 3, 2, 1, 0], // Rotate 180°
    [2, 5, 8, 1, 4, 7, 0, 3, 6], // Rotate 270° CW
    [2, 1, 0, 5, 4, 3, 8, 7, 6], // Reflect horizontal
    [6, 7, 8, 3, 4, 5, 0, 1, 2], // Reflect vertical
    [0, 3, 6, 1, 4, 7, 2, 5, 8], // Reflect main diagonal
    [8, 5, 2, 7, 4, 1, 6, 3, 0], // Reflect anti-diagonal
];
```

Each row maps `new_position → old_position`.

### Canonicalization

The canonical form is the minimum encoding across all 8 transforms:

```rust
pub fn canonical(&self) -> u64 {
    let mut min = self.0;
    for t in 1..8 {
        let transformed = self.transform(t);
        if transformed < min { min = transformed; }
    }
    min
}
```

This reduces the effective state space by up to 8×.

---

## 9. Performance Comparison

### Memory Usage (Estimated for 100M positions)

| Component | Python V1 | Rust V2 |
|-----------|-----------|---------|
| Board state | 500 bytes × 100M = 50 GB | 8 bytes × 100M = 800 MB |
| Transposition key | 30 bytes/key avg | 8 bytes/key |
| Move generation | Heap alloc per call | Stack only |
| **Total RAM** | **50+ GB** | **~2-4 GB** |

### Operation Speed (Conceptual)

| Operation | Python V1 | Rust V2 |
|-----------|-----------|---------|
| Apply move | Object mutation + allocation | Bit manipulation |
| Undo move | Store/restore objects | 16-bit restore |
| Check winner | 8 × 3 × method calls | 8 × (AND + CMP) |
| Generate moves | Vec allocation + push | Array fill |
| Canonicalize | Decode/encode per transform | Bit shuffle |

### Why This Matters

At 1 billion positions:
- **10 bytes saved per position = 10 GB RAM saved**
- **1 microsecond saved per operation = 16 minutes saved**

The bit-level approach makes solving tractable on commodity hardware.

---

## 10. API Reference

### Board (`Board`)

```rust
// Core state
Board::new() -> Board                    // Empty board, P1 to move
Board::from_u64(bits: u64) -> Board      // From encoding
board.to_u64() -> u64                    // To encoding
board.current_player() -> Player
board.switch_player()

// Cell access
board.cell(pos: Pos) -> u64              // Raw 6-bit cell
board.set_cell(pos: Pos, value: u64)
board.top_piece(pos: Pos) -> Option<(Player, Size)>
board.top_piece_fast(pos: Pos) -> Option<(Player, Size)>  // Optimized
board.is_empty(pos: Pos) -> bool

// Piece operations
board.push_piece(pos, player, size)
board.pop_top(pos) -> Option<(Player, Size)>
board.can_place(size, pos) -> bool
board.reserves(player) -> [u8; 3]

// Win detection
board.has_won(player) -> bool
board.has_won_fast(player) -> bool       // Bitboard version
board.check_winner() -> Option<Player>
board.check_winner_fast() -> Option<Player>  // Bitboard version
board.visibility_masks() -> (u16, u16)

// Move generation
board.legal_moves() -> Vec<Move>         // With reveal rule
board.legal_moves_packed() -> MoveList   // Zero-allocation version

// Apply/Undo
board.apply(mov: Move) -> Undo
board.undo(undo: &Undo)
board.apply_packed(mov: PackedMove) -> PackedUndo
board.undo_packed(undo: PackedUndo)

// Symmetry
board.transform(t: usize) -> u64         // Apply transformation t
board.canonical() -> u64                 // Minimum of all 8
board.all_symmetries() -> [u64; 8]
```

### PackedMove (`PackedMove`)

```rust
PackedMove::place(size: Size, to: u8) -> PackedMove
PackedMove::slide(from: u8, to: u8) -> PackedMove
PackedMove::from_move(mov: Move) -> PackedMove
packed.to_move() -> Move

packed.to() -> u8                        // Destination
packed.source() -> u8                    // Source (0-8 or 9-11)
packed.is_place() -> bool
packed.from_pos() -> Option<u8>          // None for placements
packed.reserve_size() -> Option<Size>    // None for slides
```

### PackedUndo (`PackedUndo`)

```rust
PackedUndo::new(mov, captured, revealed) -> PackedUndo
undo.mov() -> PackedMove
undo.captured() -> Option<(Player, Size)>
undo.revealed() -> Option<(Player, Size)>
```

### MoveList (`MoveList`)

```rust
MoveList::new() -> MoveList              // Empty list
list.push(mov: PackedMove)
list.len() -> usize
list.is_empty() -> bool
list.get(idx: usize) -> PackedMove
list.iter() -> impl Iterator<Item = PackedMove>
```

---

## Appendix: V1 Python Encoding (Historical)

V1 used the same 64-bit board encoding but with Python overhead:
- `int` objects for state (still efficient for hashing)
- `@dataclass` for moves and undo info
- `list` for move generation

The V2 Rust encoding is semantically identical but eliminates all object overhead.
