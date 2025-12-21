# V2 Rewrite Planning

## Motivation

V1 has hit fundamental scalability limits:

| Issue | V1 Reality | V2 Target |
|-------|-----------|-----------|
| Speed | ~2,000 positions/sec | 100,000+ positions/sec |
| Memory per position | 84 bytes | 9-12 bytes |
| Max positions in RAM | ~100M (8GB) | 1000M+ (12GB) |
| Time to solve 1000M | 139 hours | 2-3 hours |
| Language | Python | Rust |

### Why We Can't Just Optimize V1

1. **Python object overhead is fundamental** - Every int is 28 bytes, every dict entry is 50+ bytes overhead
2. **GC stalls** - With 100M+ objects, Python's cyclic GC becomes a bottleneck
3. **Interpreter overhead** - Even with bits-only Python, function calls and loops have overhead
4. **Memory layout** - Python objects are scattered in memory, poor cache utilization

### Scale of the Problem

- Combinatorial upper bound: ~700M-1000M positions
- Currently solved: 41M positions (~4-6% of upper bound)
- Remaining: potentially 95%+ unsolved
- Without pruning, game trees go 26M+ moves deep

## V2 Architecture Overview

```
gobblet-gobblers/
├── v1/                    # Existing code (archived)
│   ├── gobblet/
│   ├── solver/
│   ├── api/
│   ├── frontend/
│   └── tests/
│
├── v2/
│   ├── gobblet-core/      # Rust: game logic on bits
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── board.rs       # 64-bit board representation
│   │   │   ├── moves.rs       # Move generation
│   │   │   ├── rules.rs       # Win detection, reveal rule, hail mary
│   │   │   ├── symmetry.rs    # D4 canonicalization
│   │   │   └── notation.rs    # Human-readable conversion
│   │   └── Cargo.toml
│   │
│   ├── gobblet-solver/    # Rust: solver binary
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   ├── minimax.rs     # Iterative minimax
│   │   │   ├── table.rs       # Compact transposition table
│   │   │   ├── checkpoint.rs  # Binary save/load
│   │   │   └── parallel.rs    # Multi-threaded solving (optional)
│   │   └── Cargo.toml
│   │
│   ├── gobblet-api/       # Rust: web API (Axum)
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   ├── routes.rs
│   │   │   └── state.rs       # Shared solver state
│   │   └── Cargo.toml
│   │
│   └── frontend/          # React (minimal changes)
│       └── ...
│
└── docs/
    ├── solver_journal.md
    └── v2_planning.md
```

### Alternative: Python Bindings

If we want to keep Python for API/scripting:

```
v2/
├── gobblet-core/          # Rust library with PyO3 bindings
│   ├── src/
│   │   ├── lib.rs
│   │   ├── python.rs      # PyO3 module
│   │   └── ...
│   └── Cargo.toml
│
├── api/                   # FastAPI (uses Rust via PyO3)
└── frontend/              # React (unchanged)
```

**Recommendation:** Start with pure Rust. Axum is simple and fast. We can add Python bindings later if needed.

## Core Design: Bit-Based Board Representation

### Current V1 Encoding (for reference)

```
72 bits total:
- 9 cells × 8 bits per cell
- Each cell: 3 layers × 2 bits (empty/P1/P2) + 2 padding bits
- Plus 1 bit for current player
```

### V2 Encoding Options

**Option A: Keep 64-bit canonical (current scheme, but native)**

```rust
// Current scheme fits in 64 bits after canonicalization
// 55 bits for board + 1 bit for player = 56 bits used
type BoardState = u64;
```

**Option B: Separate piece tracking (faster move generation)**

```rust
// Track each player's pieces by size
struct Board {
    // Each u16 tracks 2 pieces of one size for one player
    // Bits: position (4 bits each) + on_board flag
    p1_small: u16,   // 2 small pieces
    p1_medium: u16,  // 2 medium pieces
    p1_large: u16,   // 2 large pieces
    p2_small: u16,
    p2_medium: u16,
    p2_large: u16,
    current_player: u8,
}
// 12 bytes + 1 byte = 13 bytes, but faster move gen
```

**Option C: Bitboards (like chess engines)**

```rust
struct Board {
    // 9-bit masks for each piece type
    p1_small: u16,    // Which cells have P1 small visible on top
    p1_medium: u16,
    p1_large: u16,
    p2_small: u16,
    p2_medium: u16,
    p2_large: u16,
    // Plus stacking info...
}
```

**Recommendation:** Option A for storage/hashing, with helper structures for fast move generation. The 64-bit canonical form is already compact and works well for the transposition table.

### Critical Game Rules to Preserve

1. **Gobbling:** Larger pieces can cover smaller pieces (either player's)
2. **Reveal rule:** Lifting a piece may reveal opponent's winning line
3. **Hail mary:** If reveal creates opponent win, you MUST gobble into that line to survive
4. **Same-square restriction:** Cannot place piece back where you lifted it
5. **Zugzwang:** No legal moves = loss
6. **Threefold repetition:** Same position 3 times = draw (we detect single cycle as draw)
7. **Win detection:** 3 in a row of same player's pieces visible on top

### Bit Operations for Rules

```rust
// Win detection: check 8 lines
const LINES: [u16; 8] = [
    0b111_000_000,  // Row 0
    0b000_111_000,  // Row 1
    0b000_000_111,  // Row 2
    0b100_100_100,  // Col 0
    0b010_010_010,  // Col 1
    0b001_001_001,  // Col 2
    0b100_010_001,  // Diag
    0b001_010_100,  // Anti-diag
];

fn has_won(visible_pieces: u16) -> bool {
    LINES.iter().any(|&line| (visible_pieces & line) == line)
}

// Top piece at each cell (for visibility)
fn top_piece_mask(board: &Board) -> (u16, u16) {
    // Returns (p1_visible, p2_visible)
    // Large > Medium > Small
    let p1_large = board.p1_large_positions;
    let p2_large = board.p2_large_positions;
    let p1_medium = board.p1_medium_positions & !p1_large & !p2_large;
    // ... etc
}
```

## Memory-Efficient Transposition Table

### V1 Problem

```python
# Python dict: 84 bytes per entry
table: dict[int, Outcome]
# 41M entries = 3.4 GB
# 1000M entries = 84 GB (impossible)
```

### V2 Solution

```rust
use std::collections::HashMap;

// Rust HashMap: ~24-32 bytes per entry
// Key: u64 (8 bytes)
// Value: i8 (1 byte, but aligned)
// Overhead: ~16 bytes for hash + metadata
type TranspositionTable = HashMap<u64, i8>;

// 1000M entries ≈ 24-32 GB

// Even better: custom table with open addressing
struct CompactTable {
    keys: Vec<u64>,      // 8 bytes each
    values: Vec<i8>,     // 1 byte each
    // With 2x capacity for ~50% load factor:
    // 1000M entries ≈ 18 GB
}
```

### Robin Hood Hashing (Best Performance)

```rust
// Robin Hood hashing: excellent cache behavior
// Libraries: hashbrown (used by Rust std), indexmap
use hashbrown::HashMap;

// Or custom with compression:
struct PackedTable {
    // Store (key, value) pairs in one vector
    // 9 bytes per entry, packed
    data: Vec<u8>,  // Custom packed format
}
```

**Target:** 12-16 bytes per entry → 1000M entries in 12-16 GB

## Compact Storage Format

### V1 Problem

```
SQLite: 15.6 bytes/row (608 MB for 41M rows)
- B-tree overhead
- Row format overhead
- Page slack
```

### V2 Solution: Binary Format

```rust
// Simple binary format: sorted array of (key, value) pairs
// File format:
// - Header: magic bytes, version, count
// - Data: packed (u64, i8) entries, sorted by key

struct CheckpointHeader {
    magic: [u8; 4],      // "GBL2"
    version: u32,
    count: u64,
    checksum: u64,
}

// Data section: count × 9 bytes
// 1000M entries = 9 GB on disk

// For lookup: binary search O(log n)
// Or load into HashMap for O(1)
```

### Memory-Mapped Access

```rust
use memmap2::Mmap;

// Memory-map the checkpoint file
// OS handles paging - only loads what's accessed
let file = File::open("checkpoint.bin")?;
let mmap = unsafe { Mmap::map(&file)? };

// Binary search directly on mmap'd data
fn lookup(mmap: &[u8], key: u64) -> Option<i8> {
    // Binary search on sorted entries
}
```

## Avoiding V1 Mistakes

### Mistake 1: Object Copying Instead of Mutation + Undo

**V1:**
```python
child_state = state.copy()  # Deep copy
apply_move(child_state, move)
outcome = solve(child_state)
# child_state garbage collected
```

**V2:**
```rust
apply_move(&mut state, &mov);
let outcome = solve(&mut state);
undo_move(&mut state, &mov);
// No allocation, no copying
```

### Mistake 2: O(depth²) Path Tracking

**V1 (original):**
```python
# Each frame stored frozenset of all ancestors
new_path = frame.path | {frame.canonical}  # O(depth) copy
```

**V2:**
```rust
// Shared HashSet, add on push, remove on pop
path_set.insert(canonical);  // O(1)
// ... recurse ...
path_set.remove(&canonical); // O(1)
```

### Mistake 3: Pre-computing All Moves Per Frame

**V1:**
```python
# Generate ALL moves upfront, store in frame
moves = [(move, child_state, child_canonical) for move in generate_moves(state)]
```

**V2:**
```rust
// Generate moves lazily, one at a time
struct Frame {
    state_before_move: Option<UndoInfo>,
    move_index: usize,
    // Generate next move on demand
}
```

### Mistake 4: Python GC Stalls

**V1:**
```python
gc.disable()  # Manual workaround
```

**V2:**
```rust
// No GC in Rust - deterministic memory management
// No stalls, predictable performance
```

### Mistake 5: Enum Objects in Hot Loop

**V1:**
```python
outcome = Outcome.WIN_P1  # Creates/references enum object
```

**V2:**
```rust
const WIN_P1: i8 = 1;
const DRAW: i8 = 0;
const WIN_P2: i8 = -1;
// Raw integers throughout, no object overhead
```

## Performance Targets

### Speed

| Operation | V1 | V2 Target | Improvement |
|-----------|-----|-----------|-------------|
| Position evaluation | 500 µs | 5-10 µs | 50-100x |
| Move generation | 30 µs | 0.5 µs | 60x |
| Canonicalization | 10 µs | 0.1 µs | 100x |
| Table lookup | 0.5 µs | 0.05 µs | 10x |
| **Positions/sec** | **2,000** | **200,000+** | **100x** |

### Memory

| Component | V1 | V2 Target | Improvement |
|-----------|-----|-----------|-------------|
| Per-position (table) | 84 bytes | 12 bytes | 7x |
| Per-frame (stack) | ~500 bytes | ~50 bytes | 10x |
| 1000M positions | 84 GB | 12 GB | 7x |

### Time to Complete

| Positions | V1 | V2 Target |
|-----------|-----|-----------|
| 100M | 14 hours | 8 minutes |
| 1000M | 139 hours | 1.4 hours |

## Implementation Plan

### Phase 1: Core Game Logic (Rust)

1. Board representation (u64)
2. Move generation (bit ops)
3. Win detection (bit ops)
4. Reveal rule + hail mary
5. Same-square restriction
6. Symmetry canonicalization
7. Comprehensive tests (port V1 tests)

**Deliverable:** `gobblet-core` crate with full game logic, passing all V1 test cases

### Phase 2: Solver

1. Transposition table (HashMap<u64, i8>)
2. Iterative minimax with explicit stack
3. Alpha-beta pruning (optional, for comparison)
4. Binary checkpoint format
5. Progress reporting

**Deliverable:** `gobblet-solver` binary that can solve from any position

### Phase 3: Complete Solution

1. Run solver to completion
2. Verify solution (P1 wins)
3. Analyze complete position database
4. Optimize checkpoint size if needed

**Deliverable:** Complete solution file with all reachable positions

### Phase 4: Web API + UI

1. Rust API server (Axum)
2. Endpoints: game state, legal moves, best move, apply move
3. Connect React frontend
4. Optimal play visualization

**Deliverable:** Playable web UI with solver integration

## Open Questions

### Q1: Pure Rust API or Python Bindings?

**Pure Rust (Axum):**
- Simpler deployment (single binary)
- Faster API responses
- Learning curve for Rust web

**Python bindings (PyO3):**
- Keep FastAPI (familiar)
- Easier iteration on API
- Extra complexity (two languages)

**Recommendation:** Pure Rust. The API is simple enough that Axum is straightforward.

### Q2: Parallel Solving?

The minimax algorithm is inherently sequential (need child results before parent). Options:

1. **Root parallelism:** Solve each first move in parallel
2. **Young brothers wait:** Parallelize after first child evaluated
3. **Lazy SMP:** Multiple threads explore same tree, share table

**Recommendation:** Start single-threaded. Add Lazy SMP later if needed. With 200k pos/sec, we may not need parallelism.

### Q3: Keep V1 Running During Rewrite?

**Option A:** Freeze V1, focus on V2
- Faster rewrite
- No context switching

**Option B:** Keep V1 playable
- Can demo while building V2
- More complex

**Recommendation:** Freeze V1 for now. Web UI will be offline during rewrite.

### Q4: What About the 41M Already Solved?

Options:
1. **Re-solve everything:** V2 is fast enough, ensures correctness
2. **Convert checkpoint:** Write tool to convert SQLite → binary format
3. **Ignore:** Small fraction of total, not worth complexity

**Recommendation:** Re-solve. It's a good validation that V2 matches V1 results, and only takes ~3 minutes at V2 speeds.

## Risks and Mitigations

### Risk 1: Bit Logic Bugs

Complex rules (reveal, hail mary) are tricky to implement in bit ops.

**Mitigation:**
- Port ALL V1 tests to V2
- Add fuzzing: generate random games, compare V1 vs V2
- Property-based testing

### Risk 2: Performance Not as Expected

Rust might not hit 100x improvement.

**Mitigation:**
- Benchmark each component
- Profile with `perf` / `flamegraph`
- Even 20x is still worthwhile (7 hours → 20 minutes)

### Risk 3: Memory Estimation Wrong

Actual position count could exceed estimates.

**Mitigation:**
- Design for 2x headroom
- Memory-mapped checkpoint as fallback
- Disk-backed table if needed

## Success Criteria

V2 is successful when:

1. [ ] All V1 game logic tests pass in Rust
2. [ ] Solver runs at 100,000+ positions/sec
3. [ ] Memory usage under 16 GB for complete solution
4. [ ] Complete solution computed (P1 wins verified)
5. [ ] Web UI functional with optimal play display

## Detailed Design: Board Representation

### Encoding Scheme (64-bit)

We keep the V1 scheme exactly (from `solver/encoding.py`):

```
Bits 0-53: Board state (9 cells × 6 bits per cell)
Bit 54:    Current player (0 = P1, 1 = P2)
Bits 55-63: Unused (zero for canonical form)

Each cell (6 bits) - indexed by SIZE, not stack position:
  Bits 0-1: Small piece owner (0=empty, 1=P1, 2=P2)
  Bits 2-3: Medium piece owner
  Bits 4-5: Large piece owner

Cell encoding: cell_bits = small | (medium << 2) | (large << 4)

Cell indices (row-major order):
  (0,0)=0  (0,1)=1  (0,2)=2
  (1,0)=3  (1,1)=4  (1,2)=5
  (2,0)=6  (2,1)=7  (2,2)=8

Cell bit position: cell_index * 6
```

**Key insight:** Stacking order is NOT explicitly stored. We store which PLAYER owns each SIZE at each cell. When decoding, we reconstruct the stack bottom-up (small, medium, large). This works because larger pieces are always on top.

**Example:** Cell (0,0) has P1 Small under P2 Medium under P1 Large
- small_owner = 1 (P1)
- medium_owner = 2 (P2)
- large_owner = 1 (P1)
- cell_bits = 1 | (2 << 2) | (1 << 4) = 0b01_10_01 = 0x19
- Decoded stack: [P1 Small, P2 Medium, P1 Large] ✓

### Rust Types

```rust
/// Compact board state - fits in a single u64
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Board(u64);

/// Player identifier
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Player { One = 1, Two = 2 }

/// Piece size
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Size { Small = 0, Medium = 1, Large = 2 }

/// Position on board (0-8)
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Pos(u8);

/// A move
#[derive(Clone, Copy)]
pub enum Move {
    Place { size: Size, to: Pos },
    Slide { from: Pos, to: Pos },
}

/// Undo information for backtracking
pub struct Undo {
    mov: Move,
    captured: Option<(Player, Size)>,  // What was under destination
    revealed_at_source: Option<(Player, Size)>,  // What was revealed
}
```

### Bit Operations

```rust
impl Board {
    const CELL_BITS: u32 = 6;
    const CELL_MASK: u64 = 0b111111;
    const LAYER_MASK: u64 = 0b11;
    const PLAYER_BIT: u32 = 54;

    /// Get cell value (6 bits) at position
    #[inline]
    pub fn cell(&self, pos: Pos) -> u64 {
        (self.0 >> (pos.0 as u32 * Self::CELL_BITS)) & Self::CELL_MASK
    }

    /// Set cell value at position
    #[inline]
    pub fn set_cell(&mut self, pos: Pos, value: u64) {
        let shift = pos.0 as u32 * Self::CELL_BITS;
        self.0 = (self.0 & !(Self::CELL_MASK << shift)) | (value << shift);
    }

    /// Get top piece at position (returns None if empty)
    pub fn top_piece(&self, pos: Pos) -> Option<(Player, Size)> {
        let cell = self.cell(pos);
        // Check from top layer down
        for layer in (0..3).rev() {
            let bits = (cell >> (layer * 2)) & Self::LAYER_MASK;
            if bits != 0 {
                let player = if bits == 1 { Player::One } else { Player::Two };
                let size = match layer { 0 => Size::Small, 1 => Size::Medium, _ => Size::Large };
                return Some((player, size));
            }
        }
        None
    }

    /// Get current player
    #[inline]
    pub fn current_player(&self) -> Player {
        if (self.0 >> Self::PLAYER_BIT) & 1 == 0 { Player::One } else { Player::Two }
    }

    /// Switch current player
    #[inline]
    pub fn switch_player(&mut self) {
        self.0 ^= 1 << Self::PLAYER_BIT;
    }
}
```

### Reserve Tracking

Reserves are NOT stored in the 64-bit board encoding. We derive them:

```rust
impl Board {
    /// Count pieces of each size on board for a player
    pub fn pieces_on_board(&self, player: Player) -> [u8; 3] {
        let mut counts = [0u8; 3];
        let player_bits = player as u64;

        for pos in 0..9 {
            let cell = self.cell(Pos(pos));
            for layer in 0..3 {
                if (cell >> (layer * 2)) & 0b11 == player_bits {
                    counts[layer as usize] += 1;
                }
            }
        }
        counts
    }

    /// Get reserve count (2 - pieces_on_board) for each size
    pub fn reserves(&self, player: Player) -> [u8; 3] {
        let on_board = self.pieces_on_board(player);
        [2 - on_board[0], 2 - on_board[1], 2 - on_board[2]]
    }
}
```

## Detailed Design: Move Generation

### Legal Move Rules

1. **Reserve placement:** Can place if reserve > 0 AND (cell empty OR top piece is smaller)
2. **Board move:** Can move visible piece you own to cell where it's largest
3. **Reveal restriction:** If lifting reveals opponent win, can ONLY move to gobble into that line
4. **Same-square restriction:** Cannot place back where you lifted

### Implementation

```rust
impl Board {
    /// Generate all legal moves for current player
    pub fn legal_moves(&self) -> Vec<Move> {
        let player = self.current_player();
        let reserves = self.reserves(player);
        let mut moves = Vec::with_capacity(32);

        // Reserve placements
        for size_idx in 0..3 {
            if reserves[size_idx] > 0 {
                let size = Size::from_idx(size_idx);
                for to in 0..9 {
                    if self.can_place(size, Pos(to)) {
                        moves.push(Move::Place { size, to: Pos(to) });
                    }
                }
            }
        }

        // Board moves
        for from in 0..9 {
            if let Some((piece_player, size)) = self.top_piece(Pos(from)) {
                if piece_player == player {
                    // Check reveal rule
                    let restricted_destinations = self.check_reveal(Pos(from));

                    for to in 0..9 {
                        if from != to && self.can_place(size, Pos(to)) {
                            // Same-square restriction
                            if let Some(ref dests) = restricted_destinations {
                                if !dests.contains(&Pos(to)) {
                                    continue;
                                }
                            }
                            moves.push(Move::Slide { from: Pos(from), to: Pos(to) });
                        }
                    }
                }
            }
        }

        moves
    }

    /// Check if lifting from `pos` reveals opponent win
    /// Returns Some(valid_destinations) if restricted, None if unrestricted
    fn check_reveal(&self, from: Pos) -> Option<Vec<Pos>> {
        // Temporarily remove top piece
        let mut test = *self;
        let (_, size) = test.top_piece(from).unwrap();
        test.remove_top(from);

        // Check if opponent now wins
        let opponent = self.current_player().opponent();
        if let Some(winning_line) = test.winning_line(opponent) {
            // Must gobble into the winning line
            let valid: Vec<Pos> = winning_line
                .iter()
                .filter(|&&pos| pos != from && self.can_place(size, pos))
                .copied()
                .collect();
            Some(valid)
        } else {
            None
        }
    }
}
```

### Win Detection (Fast)

```rust
const WIN_LINES: [[Pos; 3]; 8] = [
    [Pos(0), Pos(1), Pos(2)],  // Row 0
    [Pos(3), Pos(4), Pos(5)],  // Row 1
    [Pos(6), Pos(7), Pos(8)],  // Row 2
    [Pos(0), Pos(3), Pos(6)],  // Col 0
    [Pos(1), Pos(4), Pos(7)],  // Col 1
    [Pos(2), Pos(5), Pos(8)],  // Col 2
    [Pos(0), Pos(4), Pos(8)],  // Diagonal
    [Pos(2), Pos(4), Pos(6)],  // Anti-diagonal
];

impl Board {
    /// Check if player has won (3 in a row visible on top)
    pub fn has_won(&self, player: Player) -> bool {
        for line in &WIN_LINES {
            if line.iter().all(|&pos| {
                self.top_piece(pos).map(|(p, _)| p) == Some(player)
            }) {
                return true;
            }
        }
        false
    }

    /// Get winning line if any (for reveal rule)
    pub fn winning_line(&self, player: Player) -> Option<&'static [Pos; 3]> {
        for line in &WIN_LINES {
            if line.iter().all(|&pos| {
                self.top_piece(pos).map(|(p, _)| p) == Some(player)
            }) {
                return Some(line);
            }
        }
        None
    }
}
```

## Detailed Design: Apply/Undo

### Apply Move

```rust
impl Board {
    /// Apply move in place, return undo info
    pub fn apply(&mut self, mov: Move) -> Undo {
        match mov {
            Move::Place { size, to } => {
                let captured = self.top_piece(to);
                self.push_piece(to, self.current_player(), size);
                self.switch_player();
                Undo { mov, captured, revealed_at_source: None }
            }
            Move::Slide { from, to } => {
                let (player, size) = self.top_piece(from).unwrap();
                let revealed = self.pop_top(from);
                let captured = self.top_piece(to);
                self.push_piece(to, player, size);
                self.switch_player();
                Undo { mov, captured, revealed_at_source: revealed }
            }
        }
    }

    /// Undo a move
    pub fn undo(&mut self, undo: &Undo) {
        self.switch_player();  // Switch back first

        match undo.mov {
            Move::Place { size: _, to } => {
                self.pop_top(to);
                // captured piece is still there (we pushed on top)
            }
            Move::Slide { from, to } => {
                let (player, size) = self.pop_top(to).unwrap();
                // Restore revealed piece at source
                if let Some((p, s)) = undo.revealed_at_source {
                    self.push_piece(from, p, s);
                }
                // Put our piece back on top at source
                self.push_piece(from, player, size);
            }
        }
    }
}
```

## Detailed Design: Symmetry Canonicalization

### D4 Transformations

```rust
/// Position mapping for each of 8 D4 transformations
const TRANSFORMS: [[u8; 9]; 8] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8],  // Identity
    [2, 5, 8, 1, 4, 7, 0, 3, 6],  // Rotate 90
    [8, 7, 6, 5, 4, 3, 2, 1, 0],  // Rotate 180
    [6, 3, 0, 7, 4, 1, 8, 5, 2],  // Rotate 270
    [2, 1, 0, 5, 4, 3, 8, 7, 6],  // Reflect horizontal
    [6, 7, 8, 3, 4, 5, 0, 1, 2],  // Reflect vertical
    [0, 3, 6, 1, 4, 7, 2, 5, 8],  // Reflect diagonal
    [8, 5, 2, 7, 4, 1, 6, 3, 0],  // Reflect anti-diagonal
];

impl Board {
    /// Apply transformation, return new board encoding
    fn transform(&self, t: usize) -> u64 {
        let mut result = 0u64;
        for new_pos in 0..9 {
            let old_pos = TRANSFORMS[t][new_pos] as u32;
            let cell = (self.0 >> (old_pos * 6)) & 0b111111;
            result |= cell << (new_pos as u32 * 6);
        }
        // Preserve player bit
        result | (self.0 & (1 << 54))
    }

    /// Get canonical form (minimum across all transformations)
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
}
```

## Detailed Design: Solver

### Stack Frame

```rust
struct Frame {
    canonical: u64,
    undo: Option<Undo>,       // How to undo the move that got us here
    move_iter: MoveIterator,  // Lazy move generation
    best_outcome: i8,         // Best outcome found so far
    is_maximizing: bool,      // P1 maximizes, P2 minimizes
}

struct MoveIterator {
    moves: Vec<Move>,
    index: usize,
}
```

### Main Loop

```rust
pub fn solve(board: &mut Board, table: &mut HashMap<u64, i8>) -> i8 {
    let initial_canonical = board.canonical();

    if let Some(&outcome) = table.get(&initial_canonical) {
        return outcome;
    }

    let mut stack: Vec<Frame> = Vec::with_capacity(1000);
    let mut path: HashSet<u64> = HashSet::new();

    // Push initial frame
    let moves = board.legal_moves();
    stack.push(Frame {
        canonical: initial_canonical,
        undo: None,
        move_iter: MoveIterator { moves, index: 0 },
        best_outcome: if board.current_player() == Player::One { -2 } else { 2 },
        is_maximizing: board.current_player() == Player::One,
    });
    path.insert(initial_canonical);

    while let Some(frame) = stack.last_mut() {
        // Try next move
        if let Some(mov) = frame.move_iter.next() {
            let undo = board.apply(mov);
            let child_canonical = board.canonical();

            // Cycle detection
            if path.contains(&child_canonical) {
                board.undo(&undo);
                frame.update_outcome(DRAW);
                continue;
            }

            // Cache hit
            if let Some(&outcome) = table.get(&child_canonical) {
                board.undo(&undo);
                frame.update_outcome(outcome);

                // Alpha-beta pruning
                if frame.can_prune() {
                    frame.move_iter.exhaust();
                }
                continue;
            }

            // Check for terminal (win/loss)
            if let Some(outcome) = board.terminal_outcome() {
                table.insert(child_canonical, outcome);
                board.undo(&undo);
                frame.update_outcome(outcome);
                continue;
            }

            // Push child frame
            let child_moves = board.legal_moves();
            if child_moves.is_empty() {
                // Zugzwang
                let outcome = if board.current_player() == Player::One { WIN_P2 } else { WIN_P1 };
                table.insert(child_canonical, outcome);
                board.undo(&undo);
                frame.update_outcome(outcome);
                continue;
            }

            path.insert(child_canonical);
            stack.push(Frame {
                canonical: child_canonical,
                undo: Some(undo),
                move_iter: MoveIterator { moves: child_moves, index: 0 },
                best_outcome: if board.current_player() == Player::One { -2 } else { 2 },
                is_maximizing: board.current_player() == Player::One,
            });
        } else {
            // All moves exhausted, pop frame
            let frame = stack.pop().unwrap();
            path.remove(&frame.canonical);
            table.insert(frame.canonical, frame.best_outcome);

            // Undo move and propagate to parent
            if let Some(undo) = frame.undo {
                board.undo(&undo);
            }
            if let Some(parent) = stack.last_mut() {
                parent.update_outcome(frame.best_outcome);
            }
        }
    }

    table[&initial_canonical]
}
```

## Detailed Design: Checkpoint Format

### Binary Format Specification

```
File: checkpoint.bin

Header (32 bytes):
  Bytes 0-3:   Magic "GBL2"
  Bytes 4-7:   Version (u32, little-endian) = 1
  Bytes 8-15:  Entry count (u64, little-endian)
  Bytes 16-23: Checksum (u64, xxhash of data section)
  Bytes 24-31: Reserved (zeros)

Data section:
  Repeated entry_count times:
    Bytes 0-7: Canonical position (u64, little-endian)
    Byte 8:    Outcome (i8: -1, 0, or 1)

  Entries MUST be sorted by canonical position for binary search.

Total size: 32 + (entry_count * 9) bytes
```

### Load/Save

```rust
pub fn save_checkpoint(path: &Path, table: &HashMap<u64, i8>) -> io::Result<()> {
    let mut entries: Vec<(u64, i8)> = table.iter().map(|(&k, &v)| (k, v)).collect();
    entries.sort_by_key(|&(k, _)| k);

    let mut file = BufWriter::new(File::create(path)?);

    // Header
    file.write_all(b"GBL2")?;
    file.write_all(&1u32.to_le_bytes())?;
    file.write_all(&(entries.len() as u64).to_le_bytes())?;
    // ... checksum, reserved ...

    // Data
    for (canonical, outcome) in entries {
        file.write_all(&canonical.to_le_bytes())?;
        file.write_all(&[outcome as u8])?;
    }

    Ok(())
}

pub fn load_checkpoint(path: &Path) -> io::Result<HashMap<u64, i8>> {
    let file = File::open(path)?;
    let mmap = unsafe { Mmap::map(&file)? };

    // Parse header
    let count = u64::from_le_bytes(mmap[8..16].try_into().unwrap()) as usize;

    // Load entries
    let mut table = HashMap::with_capacity(count);
    let data = &mmap[32..];
    for i in 0..count {
        let offset = i * 9;
        let canonical = u64::from_le_bytes(data[offset..offset+8].try_into().unwrap());
        let outcome = data[offset + 8] as i8;
        table.insert(canonical, outcome);
    }

    Ok(table)
}
```

## Testing Strategy

### Unit Tests (Port from V1)

All 81 V1 tests should pass in V2. Key categories:

1. **Encoding/decoding** - Roundtrip board ↔ u64
2. **Move generation** - All legal moves, reveal rule, hail mary
3. **Win detection** - All 8 lines
4. **Apply/undo** - State restoration
5. **Symmetry** - Canonical form consistency
6. **Solver** - Known positions with known outcomes

### Fuzzing

```rust
#[test]
fn fuzz_apply_undo() {
    let mut rng = rand::thread_rng();
    for _ in 0..10000 {
        let mut board = random_board(&mut rng);
        let original = board.clone();

        let moves = board.legal_moves();
        if let Some(mov) = moves.choose(&mut rng) {
            let undo = board.apply(*mov);
            board.undo(&undo);
            assert_eq!(board, original, "apply/undo not symmetric");
        }
    }
}
```

### V1/V2 Comparison

```rust
#[test]
fn compare_with_v1() {
    // Load V1 checkpoint
    let v1_outcomes = load_v1_sqlite("../v1/solver/gobblet_solver.db");

    // For each position, verify V2 computes same outcome
    for (canonical, v1_outcome) in v1_outcomes.iter().take(10000) {
        let board = Board::from_canonical(*canonical);
        let v2_outcome = solve_position(&board);
        assert_eq!(v2_outcome, *v1_outcome, "Mismatch at {}", canonical);
    }
}
```

## API Contract (For Frontend Compatibility)

V2 API must match V1 endpoints:

```
GET  /game          → { board, reserves, current_player, result }
GET  /moves         → [{ from?, to, size? }, ...]
POST /move          → Apply move, return new game state
POST /reset         → Reset to initial position
GET  /history       → Move history with notation
POST /undo          → Undo last move
POST /redo          → Redo
GET  /best-move     → Optimal move from current position
```

The internal representation is u64, but API responses use human-readable JSON. Conversion happens at the API boundary.

## Phase 1 Milestones: Game Logic Rewrite

The goal is complete game logic parity with V1, operating on bit representations.

### Milestone 1.1: Project Setup & Basic Types

**Deliverables:**
- [ ] Move existing code to `v1/` subdirectory
- [ ] Create `v2/gobblet-core/` Rust crate
- [ ] Define basic types: `Board(u64)`, `Player`, `Size`, `Pos`, `Move`
- [ ] Implement `Board::new()` → empty board with P1 to move

**Verification:**
- Cargo builds without errors
- Can create new board, verify encoding is 0

**Estimated effort:** Small (setup)

---

### Milestone 1.2: Board Encoding & Decoding

**Deliverables:**
- [ ] `Board::cell(pos) -> u64` - Get 6 bits for a cell
- [ ] `Board::set_cell(pos, value)` - Set 6 bits for a cell
- [ ] `Board::top_piece(pos) -> Option<(Player, Size)>` - Get visible piece
- [ ] `Board::current_player() -> Player`
- [ ] `Board::switch_player()`
- [ ] `Board::from_u64(u64) -> Board` - Decode
- [ ] Verify roundtrip: `Board::from_u64(board.0) == board`

**Verification:**
- Unit tests for each operation
- Fuzz test: random boards, encode/decode roundtrip

**Estimated effort:** Medium

---

### Milestone 1.3: Piece Operations

**Deliverables:**
- [ ] `Board::push_piece(pos, player, size)` - Add piece to stack
- [ ] `Board::pop_top(pos) -> Option<(Player, Size)>` - Remove top piece
- [ ] `Board::can_place(size, pos) -> bool` - Check if placement valid
- [ ] `Board::reserves(player) -> [u8; 3]` - Derive from board state

**Verification:**
- Push/pop roundtrip
- Reserves decrease as pieces placed
- Cannot place on larger/equal pieces

**Estimated effort:** Medium

---

### Milestone 1.4: Win Detection

**Deliverables:**
- [ ] `Board::has_won(player) -> bool` - Check all 8 lines
- [ ] `Board::winning_line(player) -> Option<[Pos; 3]>` - For reveal rule
- [ ] `Board::check_winner() -> Option<Player>` - Check both players

**Verification:**
- Test all 8 winning lines
- Hidden pieces don't count
- Multiple lines still single winner

**Estimated effort:** Small

---

### Milestone 1.5: Move Generation (Simple)

**Deliverables:**
- [ ] Generate reserve placement moves
- [ ] Generate board moves (without reveal rule)
- [ ] `Board::legal_moves_simple() -> Vec<Move>`

**Verification:**
- Initial position has 27 moves
- Cannot move opponent's pieces
- Cannot gobble larger/equal

**Estimated effort:** Medium

---

### Milestone 1.6: Reveal Rule

**Deliverables:**
- [ ] `Board::check_reveal(from) -> Option<[Pos; 3]>` - Get winning line if revealed
- [ ] Filter board moves through reveal rule
- [ ] Same-square restriction
- [ ] `Board::legal_moves() -> Vec<Move>` - Full implementation

**Verification:**
- All Category 4 tests from game_logic_testing.md
- This is the hardest part - take extra care

**Estimated effort:** Large (complex logic)

---

### Milestone 1.7: Apply & Undo

**Deliverables:**
- [ ] `Board::apply(move) -> Undo` - Mutate board, return undo info
- [ ] `Board::undo(undo)` - Restore previous state
- [ ] Handle player switching
- [ ] Handle piece revealing on undo

**Verification:**
- Fuzz: random games, apply/undo every move, verify restoration
- Test gobbling undo reveals hidden piece

**Estimated effort:** Medium

---

### Milestone 1.8: Symmetry & Canonicalization

**Deliverables:**
- [ ] `Board::transform(t) -> u64` - Apply one of 8 D4 transforms
- [ ] `Board::canonical() -> u64` - Return minimum across transforms
- [ ] Verify transforms are correct (test rotation, reflection)

**Verification:**
- Corners map to same canonical
- Center is fixed point
- Canonical is idempotent

**Estimated effort:** Medium

---

### Milestone 1.9: V1/V2 Parity Testing ✓

**Deliverables:**
- [x] Export ~1000 positions from V1 with their legal moves
- [x] Export ~1000 positions from V1 with their encodings
- [x] Run V2 on same positions, verify exact match
- [x] Small game tree comparison (depth 5, ~10k nodes)

**Completed:** Exported 50,009 positions from V1 (49,997 game tree + 12 edge cases).
Created `tests/parity.rs` with 3 integration tests:
- `test_v1_v2_parity`: Full parity check (50,009 pass)
- `test_game_tree_parity`: Game tree only (49,997 pass)
- `test_edge_cases_parity_report`: Edge case report (12 pass)

Fixed V1 export script to use `place_from_reserve()` helper that properly updates
both board AND reserves. Also fixed edge cases that created impossible game states
(e.g., Small on top of Medium, which violates gobble rules).

**Verification:**
- 100% parity between V1 and V2 on all 50,009 positions
- All edge cases now use valid, reachable game states

**Estimated effort:** Medium

---

### Milestone 1.10: Frontend Integration ✓

**Deliverables:**
- [x] `gobblet-api` Rust crate with Axum
- [x] Implement API endpoints matching V1 contract
- [x] Convert u64 board to JSON for frontend
- [x] Convert JSON moves to internal representation
- [x] Frontend connects to V2 backend

**Completed:** Created gobblet-api with all V1-compatible endpoints. Added `encoding`
field to /game response and switched state export to decimal u64 (not base64).
Frontend works unchanged with the new Rust backend.

**Verification:**
- All API endpoints tested via curl
- Frontend serves and connects to API

**Estimated effort:** Medium

---

## Discussion: Potential Pitfalls

### Pitfall 1: Encoding Mismatch

**Risk:** V2's bit layout differs from V1's, breaking checkpoint compatibility.

**Mitigation:**
- Document V1's exact encoding with bit-level examples
- Export V1 encodings for specific positions
- Verify V2 produces identical bits before proceeding

**Detection:** Milestone 1.9 parity testing

---

### Pitfall 2: Reveal Rule Edge Cases

**Risk:** Subtle reveal rule bugs (multiple lines, same-square, zugzwang).

**Mitigation:**
- Enumerate ALL reveal scenarios in game_logic_testing.md
- Test each explicitly
- Compare move generation with V1 for many positions

**Detection:** Category 4 tests, fuzz testing against V1

---

### Pitfall 3: Undo Logic Errors

**Risk:** Undo doesn't perfectly restore state (corrupted board).

**Mitigation:**
- Fuzz heavily: random games, undo every move
- Compare board before and after apply+undo
- Include revealed piece tracking in Undo struct

**Detection:** Fuzz testing in Milestone 1.7

---

### Pitfall 4: Symmetry Transform Errors

**Risk:** Wrong rotation/reflection mappings lead to wrong canonicalization.

**Mitigation:**
- Test each of 8 transforms individually
- Verify known positions (corners, center)
- Check canonical is truly minimum

**Detection:** Milestone 1.8 tests

---

### Pitfall 5: Off-By-One in Bit Operations

**Risk:** Shifting by wrong amount, masking wrong bits.

**Mitigation:**
- Add extensive inline comments with bit layouts
- Unit test each cell position individually
- Use constants for bit widths, not magic numbers

**Detection:** Encoding/decoding roundtrip tests

---

## Questions Resolved

### Q: Pure Rust vs Python bindings?
**A:** Pure Rust (Axum). Simpler build, fast enough for our needs.

### Q: Rust vs C++?
**A:** Rust. Equal speed, but better tooling (Cargo) and safety guarantees.

### Q: What about /docs and /frontend?
**A:** Keep at root (shared). V2 API matches V1 contract, frontend works with both.

### Q: How to verify parity?
**A:** Export test positions from V1, verify V2 matches exactly. Small game tree comparison.

---

## Phase 1 Status: COMPLETE

All milestones 1.1-1.10 completed:
- gobblet-core: Full game logic with 64-bit encoding
- 109 tests passing, 50,009 positions verified for V1/V2 parity
- gobblet-api: Axum REST API matching V1 contract
- Frontend integration working

**Next:** See `v2_solver_planning.md` for Phase 2 (Solver).

---

*Document created: 2024-12-19*
*Phase 1 completed: 2024-12-19*
