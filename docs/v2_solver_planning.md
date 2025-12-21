# V2 Solver Planning

## Context

Phase 1 (game logic + frontend integration) is complete. The V2 Rust implementation has:
- Full game logic with 64-bit board encoding
- 109 tests passing, 50,009 positions verified for V1/V2 parity
- Axum API server with all V1-compatible endpoints
- Working frontend integration

Now we tackle the solver.

## V1 Solver Recap

### What V1 Achieved
- **40.9 million positions** solved
- **P1 wins** with optimal play (confirmed)
- Outcome distribution: WIN_P1 (51%), WIN_P2 (49%), DRAW (0.5%)
- Full solve time: ~1.5 hours with alpha-beta pruning
- Database: 638 MB in SQLite (15.6 bytes/row)

### V1 Challenges & Lessons Learned

| Issue | Impact | V1 Solution | V2 Approach |
|-------|--------|-------------|-------------|
| **Python object overhead** | 84 bytes/entry (7.5x optimal) | Accepted | Rust primitives |
| **GC stalls** | 100% CPU in garbage collector | `gc.disable()` | No GC in Rust |
| **O(depth²) path tracking** | Memory explosion at depth 84k | Shared `path_set` | Same approach |
| **Pre-computed moves** | ~1.5 KB/frame, ~39 GB at depth 26M | Not addressed | Lazy generation |
| **Deep recursion** | Hit Python's 10k limit | Iterative stack | Iterative stack |
| **OOM at 18GB** | Killed during unpruned solve | Stopped exploration | Better memory efficiency |
| **Extreme depth** | 26M+ moves without pruning | Gave up | Defer unpruned solve |

### V1 Database Schema
```sql
CREATE TABLE transposition (
    canonical INTEGER PRIMARY KEY,  -- 8 bytes (stored as SQLite integer)
    outcome INTEGER NOT NULL        -- 4 bytes (only needs 1)
);
-- Actual: 15.6 bytes/row with B-tree overhead (73% waste)
```

### Unsolved Frontier
- **26.7 million positions** enumerated but not solved (via BFS)
- These are positions reachable only via suboptimal play (pruned by alpha-beta)
- Full exploration blocked by memory limits in V1

---

## V2 Design Decisions

### Storage Strategy

**Phase 1 (Solving): Binary checkpoint format**

Optimized for solve speed and memory:
```rust
// 9 bytes per entry
struct Entry {
    canonical: u64,  // 8 bytes, little-endian
    outcome: i8,     // 1 byte (-1=P2 wins, 0=draw, 1=P1 wins)
}
// File: sorted by canonical for binary search
```

| Format | Bytes/entry | 100M positions | 1B positions |
|--------|-------------|----------------|--------------|
| V1 SQLite | 15.6 | 1.5 GB | 15.6 GB |
| **V2 Binary** | 9 | 900 MB | 9 GB |

**Phase 2 (Production UI): Migrate to queryable format**

After solve is complete, one-time migration to SQLite/Postgres for UI queries:
- Position lookup with metadata
- Aggregate statistics
- Move evaluation queries

This is a reasonable "solve fast now, migrate later" approach.

### Transposition Table (In-Memory)

```rust
use std::collections::HashMap;

// Rust HashMap: ~24-32 bytes per entry
// Much better than Python's 84 bytes
type TranspositionTable = HashMap<u64, i8>;

// For 100M positions: ~2.5-3.2 GB
// For 1B positions: ~25-32 GB (may need optimization)
```

If memory becomes tight, options:
1. Custom open-addressing hash table (~12-16 bytes/entry)
2. Memory-mapped binary file with binary search
3. Disk-backed hash (LMDB, RocksDB)

### Path Tracking (Cycle Detection)

Shared mutable set, O(depth) memory:
```rust
let mut path: HashSet<u64> = HashSet::new();

// When pushing frame:
path.insert(canonical);

// When popping frame:
path.remove(&canonical);
```

### Move Ordering (Critical for Alpha-Beta)

**Lesson Learned:** Move ordering dramatically affects alpha-beta pruning efficiency.

V1 uses **dynamic ordering** - it sorts moves by outcome from the transposition table:
1. Known winning moves first (trigger immediate cutoff)
2. Known draws second
3. Unknown moves third
4. Known losses last

V2's initial implementation used **static ordering** (Large → Medium → Small), which
explored more branches before finding cutoffs. Result: V2 stored 5x more positions.

**Solution:** Check transposition table before exploring each move. If child is already
solved as a win for current player, explore it immediately for cutoff. This doesn't
require generating all moves upfront - we check each move as we generate it.

```rust
// For each move from generator:
if let Some(&outcome) = table.get(&child_canonical) {
    if outcome == best_for_current_player {
        // Found a winning move! Use it immediately.
        frame.update(outcome);
        // Cutoff remaining moves
        break;
    }
}
```

### Move Generation

**Problem: Pre-computed moves consume memory at depth**

In V1, each stack frame stored all legal moves upfront:

```
Stack (DFS):
┌─────────────────────────────────────────────────────────────┐
│ Frame 0: Initial position                                    │
│   moves = [(L(0,0), canonical_a, ongoing),                  │
│            (L(0,1), canonical_b, ongoing),                  │
│            (L(1,1), canonical_c, ongoing),                  │  ← 27 moves stored
│            ... 24 more moves ...]                           │
│   move_idx = 1  (currently exploring L(0,1))                │
├─────────────────────────────────────────────────────────────┤
│ Frame 1: After L(0,0)                                        │
│   moves = [(L(0,1), ...), (M(0,0), ...), ...]               │  ← 26 moves stored
│   move_idx = 0                                               │
├─────────────────────────────────────────────────────────────┤
│ Frame 2: After L(0,0) → L(0,1)                              │
│   moves = [...]                                              │  ← ~25 moves stored
└─────────────────────────────────────────────────────────────┘

At depth D: D frames × ~15 moves × ~100 bytes = ~1.5 KB per frame
At depth 26M (unpruned): ~39 GB just for move lists
```

**Solution: Lazy generation**

Instead of storing all moves, store an iterator that produces them on demand:

```
┌─────────────────────────────────────────────────────────────┐
│ Frame 0: Initial position                                    │
│   move_gen = MoveGenerator { reserve_idx: 1, board_idx: 0 } │  ← ~50 bytes
│   (produces next move on demand, remembers where it left off)│
├─────────────────────────────────────────────────────────────┤
│ Frame 1: After L(0,0)                                        │
│   move_gen = MoveGenerator { ... }                          │  ← ~50 bytes
└─────────────────────────────────────────────────────────────┘

At depth D: D frames × ~50 bytes = much less memory
```

The key insight: we don't need to remember all moves - just be able to generate the *next* one when we backtrack.

**Implementation:** Lazy generation
```rust
struct Frame {
    canonical: u64,
    undo: Option<Undo>,
    move_gen: MoveGenerator,  // Iterator, generates on demand
    best_outcome: i8,
    is_maximizing: bool,
}

impl MoveGenerator {
    fn next(&mut self, board: &Board) -> Option<Move> {
        // Generate next move without storing all moves
    }
}
```

At depth D, each frame is ~50-100 bytes (vs V1's ~1500 bytes).

---

## Milestones

### Phase 2.1: Solver Infrastructure

**Goal:** Build the foundation before solving.

**Deliverables:**
- [ ] Transposition table (`HashMap<u64, i8>`)
- [ ] Binary checkpoint format (save/load)
- [ ] Progress logging with timestamps
- [ ] Memory usage logging
- [ ] SIGINT handling for graceful shutdown
- [ ] Basic stats tracking

**Stats to Track:**
- Positions evaluated
- Unique positions in table
- Cache hits
- Terminal positions (P1 wins / P2 wins / draws)
- Cycle detections
- Max stack depth
- Positions per second
- Memory usage (RSS)
- Checkpoint save duration

**Verification:**
- Can save/load checkpoint correctly
- Graceful shutdown preserves progress

---

### Phase 2.2: Alpha-Beta Solve

**Goal:** Complete solve with alpha-beta pruning, validate against V1's ~20M results.

This is the primary validation milestone. Alpha-beta pruning explores the same positions
as V1 did (~20M), allowing direct comparison of outcomes.

**Deliverables:**
- [ ] Iterative minimax with explicit stack
- [ ] Alpha-beta pruning
- [ ] Move ordering (winning moves first)
- [ ] Time-based checkpointing (every 60s)
- [ ] Run full solve from initial position

**Validation:**
- Result: P1 wins with optimal play
- Position count: ~20M (matching V1's 19,931,991 with pruning)
- Cross-check against V1 SQLite: `v1/solver/gobblet_solver.db` (40.9M positions)
- For positions in both V1 and V2, outcomes must match exactly
- Outcome distribution similar to V1 (~51% P1, ~49% P2, ~0.5% draw)

**Expected Performance:**
- Target: 100,000+ positions/sec
- Full solve: ~5-10 minutes (vs V1's 1.5 hours)

---

### Phase 2.3: Benchmarking & Optimization

**Goal:** Measure and tune performance.

**Deliverables:**
- [ ] Comprehensive benchmarks
- [ ] Flamegraph profiling
- [ ] Tune hash table capacity
- [ ] Optimize hot paths if needed

**Metrics to Capture:**
- Time per position (µs)
- Move generation time
- Canonicalization time
- Hash table lookup/insert time
- Pruning effectiveness (% branches skipped)
- Average branching factor
- Positions by depth level

**Log Format Example:**
```
[00:01:30] positions=1,500,000 unique=4,200,000 cache_hits=2,100,000
           rate=25,000/s memory=850MB depth=420
           terminals: p1=2,100,000 p2=1,950,000 draw=15,000
           cycles=5,200 pruned=65%
```

---

### Phase 2.4: Production Integration

**Goal:** Make solver data available to UI.

**Deliverables:**
- [ ] Migrate binary checkpoint to SQLite
- [ ] API endpoint: `GET /evaluate` - position evaluation
- [ ] API endpoint: `GET /moves/evaluated` - all moves with outcomes
- [ ] Frontend: display move evaluations (win/draw/loss)
- [ ] Frontend: color coding (green/yellow/red)
- [ ] Frontend: sort moves by quality

**UI Display:**
```
Legal Moves (sorted by quality):
  L(1,1)     Win in 3    [green, optimal]
  M(0,0)     Win in 5    [green]
  S(0,1)     Draw        [yellow]
  M(1,0)     Loss in 4   [red]
```

---

### Phase 2.5: Full Enumeration (Optional)

**Goal:** Solve all reachable positions (including pruned branches).

**Prerequisites:**
- Lazy move generation implemented
- Memory-efficient stack frames
- Possibly memory-mapped checkpoint

**Approach:**
1. Load existing checkpoint (~20M positions)
2. Run solver without pruning (`prune=false`)
3. Incremental checkpointing
4. May take many hours/days

**Expected:**
- Total positions: ~50-100M (estimated)
- Would enable "play as P1 suboptimally" in UI

**Decision:** Defer until after Phase 2.4. Not required for core functionality.

---

## Technical Details

### Checkpoint File Format

```
Header (32 bytes):
  Bytes 0-3:   Magic "GBL2"
  Bytes 4-7:   Version (u32 LE) = 1
  Bytes 8-15:  Entry count (u64 LE)
  Bytes 16-23: Checksum (xxhash64 of data section)
  Bytes 24-31: Reserved (zeros)

Data section (entry_count × 9 bytes):
  Bytes 0-7: Canonical position (u64 LE)
  Byte 8:    Outcome (i8: -1, 0, or 1)

Entries sorted by canonical for O(log n) binary search.
```

### Solver Loop (Pseudocode)

```rust
fn solve(board: &mut Board, table: &mut HashMap<u64, i8>) -> i8 {
    let mut stack: Vec<Frame> = vec![initial_frame(board)];
    let mut path: HashSet<u64> = HashSet::new();

    while let Some(frame) = stack.last_mut() {
        if let Some(mov) = frame.move_gen.next(board) {
            let undo = board.apply(mov);
            let child_canonical = board.canonical();

            // Cycle detection
            if path.contains(&child_canonical) {
                board.undo(&undo);
                frame.update(DRAW);
                continue;
            }

            // Cache hit
            if let Some(&outcome) = table.get(&child_canonical) {
                board.undo(&undo);
                frame.update(outcome);
                if frame.can_prune() { frame.move_gen.exhaust(); }
                continue;
            }

            // Terminal check
            if let Some(outcome) = board.terminal_outcome() {
                table.insert(child_canonical, outcome);
                board.undo(&undo);
                frame.update(outcome);
                continue;
            }

            // Push child frame
            path.insert(child_canonical);
            stack.push(Frame::new(board, Some(undo)));
        } else {
            // Pop frame
            let frame = stack.pop().unwrap();
            path.remove(&frame.canonical);
            table.insert(frame.canonical, frame.best_outcome);

            if let Some(undo) = frame.undo {
                board.undo(&undo);
            }
            if let Some(parent) = stack.last_mut() {
                parent.update(frame.best_outcome);
            }
        }
    }

    table[&board.canonical()]
}
```

### V1 Validation Approach

```rust
// Load V1 SQLite checkpoint
let v1_conn = Connection::open("v1/solver/gobblet_solver.db")?;
let v1_outcomes: Vec<(u64, i8)> = v1_conn
    .prepare("SELECT canonical, outcome FROM transposition")?
    .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
    .collect();

// Compare random sample
for (canonical, v1_outcome) in v1_outcomes.iter().take(10_000) {
    let v2_outcome = v2_table.get(canonical);
    assert_eq!(v2_outcome, Some(v1_outcome), "Mismatch at {}", canonical);
}
```

---

## Open Questions

### Q1: Parallel Solving?
**Decision:** Defer. Single-threaded should be fast enough. Add parallelism later if needed.

Options if we revisit:
- Root parallelism (solve each first move in parallel)
- Lazy SMP (multiple threads share table)
- Work-stealing with thread-local caches

### Q2: What if state space is larger than expected?
**Mitigation:**
- Design for 2x headroom
- Memory-mapped checkpoint as fallback
- Disk-backed table if needed (LMDB)

### Q3: Unpruned solve feasibility?
**Analysis needed:** After pruned solve, measure:
- How many positions in pruned branches?
- Memory requirements for full exploration
- Is it worth the effort?

---

## Success Criteria

Phase 2 is successful when:

1. [ ] Alpha-beta solve completes: P1 wins confirmed
2. [ ] Performance: 100,000+ positions/sec
3. [ ] Memory: under 4 GB for pruned solve
4. [ ] Validation: matches V1 results on sample
5. [ ] Checkpointing: can interrupt and resume
6. [ ] UI integration: move evaluations displayed

---

## File Structure

```
v2/
├── gobblet-core/           # Game logic (existing)
├── gobblet-api/            # Web API (existing)
├── gobblet-solver/         # NEW: Solver binary
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs         # CLI entry point
│       ├── solver.rs       # Minimax implementation
│       ├── table.rs        # Transposition table
│       ├── checkpoint.rs   # Binary save/load
│       └── stats.rs        # Progress tracking
└── data/
    └── checkpoint.bin      # Solver output
```

---

*Document created: 2024-12-19*
*Status: Ready for implementation*
