# Solver Development Journal

## Overview

This document tracks the development of the Gobblet Gobblers game solver.

## Phase 1: State Encoding (Complete)

**Files:** `solver/encoding.py`, `tests/test_encoding.py`

Implemented binary encoding for game states:
- 72-bit encoding: 9 cells × 8 bits per cell (3 layers × 2 bits + 2 bits padding)
- Each layer encodes: empty (0), P1 piece (1), P2 piece (2)
- Current player stored in low bit of final byte
- D₄ symmetry group canonicalization (8 transformations: 4 rotations × 2 reflections)
- `canonicalize()` returns minimum encoding across all 8 symmetric variants

**Tests:** 21 unit tests covering encoding, decoding, symmetry transformations.

## Phase 2: Minimax Solver (Complete)

**Files:** `solver/minimax.py`, `tests/test_minimax.py`

### Initial Implementation

- Recursive minimax with transposition table
- Hit Python's recursion limit (~10k depth)
- Converted to iterative implementation with explicit `StackFrame` stack

### Key Components

```python
class Outcome(IntEnum):
    WIN_P2 = -1  # Player 2 wins with optimal play
    DRAW = 0     # Draw with optimal play
    WIN_P1 = 1   # Player 1 wins with optimal play

class Solver:
    table: dict[int, Outcome]  # canonical state -> outcome
    stats: SolverStats         # positions_evaluated, cache_hits, etc.
```

### Optimizations Added

1. **Alpha-beta pruning**: Stop exploring when current player finds their best possible outcome
   - P1 (maximizer) finds WIN_P1 → skip remaining moves
   - P2 (minimizer) finds WIN_P2 → skip remaining moves

2. **Move ordering**: Sort moves to try best outcomes first
   - Winning moves explored first (enables early pruning)
   - Draws second, losing moves last

### Performance Observations

| Duration | Positions Evaluated | Unique in Table | Max Depth | Notes |
|----------|---------------------|-----------------|-----------|-------|
| 60s (no ordering) | 9,423 | 47,088 | 12,724 | Very deep before backtrack |
| 60s (with ordering) | 45,892 | 128,011 | 473 | 5x improvement |
| 5 min | 232,630 | 629,765 | 480 | Steady progress |
| 15 min | 675,304 | 1,869,240 | 480 | ~750 pos/sec |
| 45 min | 1,960,931 | 5,522,224 | 480 | Still running |
| ~50 min (killed) | ~2.2M est | ~8-10M est | 480 | 786 MB memory |

**Key insight:** Move ordering reduced max depth from 12,724 to ~480, dramatically improving pruning effectiveness.

### Checkpointing (Implemented but not tested in production)

**Files:** `solver/checkpoint.py`, `solver/run_solver.py`, `tests/test_checkpoint.py`

- SQLite-based persistence for transposition table
- Save/load `canonical -> outcome` mappings
- 9 unit tests passing

**Limitation:** Only saves completed outcomes. Does not save partial exploration state (stack frames, paths).

## Analysis (2024-12-17)

### 1. State Space Size

**Clarification of terms:**
- **Unique positions**: Number of distinct board states. With transposition table, each is solved once.
- **Game tree nodes**: Much larger - same position reached via different paths. Irrelevant with transposition table.

**Observations:**
- After 45 min: 5.5M unique positions (not complete)
- After ~50 min: ~8-10M estimated (786MB memory)
- Total unknown - could be 10M-50M

**Key insight:** The number of unique positions bounds the work. If there are N unique positions, we solve each once. The question is: what is N?

### 2. Performance Analysis

Benchmark results (per position with ~27 moves):

| Component | Time |
|-----------|------|
| Move generation | 18-61 µs |
| Play move (×27) | ~800 µs total |
| Encode state (×27) | ~50 µs total |
| Canonicalize (×27) | ~270 µs total |
| **Full child gen** | **~1 ms** |

**Finding:** ~1ms per position = ~1000 pos/sec theoretical max. Observed 750 pos/sec.

**Bottleneck:** No single component dominates. Cost is distributed across move generation, state copying, encoding, and canonicalization.

**Optimization options:**
- Rewrite in Rust/C++ (10-100x faster)
- Smarter pruning to reduce positions visited
- Incremental encoding (avoid re-encoding entire state)

### 3. Symmetry Implementation

**How it works:**
1. Encode state → 55-bit integer
2. Apply all 8 D₄ transformations (4 rotations × 2 reflections)
3. Return minimum → canonical form
4. Transposition table uses canonical form as key

**Effect:** Symmetric positions have same canonical form, so table lookup finds them.

**Reduction:** Close to 8x for most positions (some positions are self-symmetric).

### 4. Checkpointing

**What gets saved:**
- Transposition table (canonical → outcome)
- Statistics metadata

**What does NOT get saved:**
- Current exploration stack
- Partial subtree progress

**Resume behavior:**
- Loads table with solved positions
- Starts fresh from root
- Hits cache for completed subtrees
- Re-explores incomplete subtrees

**Tested and verified working:**
- Phase 1: 10s solve → 21,654 positions
- Phase 2: Load checkpoint into fresh solver
- Phase 3: Continue 5s → 3,711 cache hits, only 3,805 new positions

## State Space Analysis (Updated)

### Combinatorial Upper Bound

Each size (S, M, L) has 4 pieces (2 per player). Same-size pieces cannot share cells.

For each size, ways to place 2+2 pieces on 9 cells (same-size exclusion):
- Sum over (a, b) where a = P1 pieces on board, b = P2 pieces on board
- P2 must use different cells than P1
- Total: **1,423 ways per size**

Since different sizes CAN share cells (stacking), placements are independent:
```
1,423³ × 2 (current player) = 5.76 × 10⁹
With D₄ symmetry (÷8): ~7.2 × 10⁸
```

**Upper bound: ~700 million positions**

Reachable positions are fewer (game ends on wins, reveal rule restricts moves).

**Working estimate: ≤10⁹ unique positions, likely 10⁷-10⁸ reachable.**

### Alpha-Beta Pruning Effect

With alpha-beta + move ordering, we skip positions once best outcome is found:
- P1 finds WIN_P1 → skip remaining moves
- P2 finds WIN_P2 → skip remaining moves

Positions reachable only via skipped moves are never explored.

**Evidence:**
- Without move ordering: stuck at depth 12,724
- With move ordering: max depth ~480, steady progress

**Conclusion:** Actual positions to explore likely far fewer than upper bound.

## Performance Analysis (Updated)

### Bottleneck Identified

Benchmark breakdown per position (~27 moves):

| Operation | Time | Per position |
|-----------|------|--------------|
| generate_moves | 18 µs | 18 µs (once) |
| play_move | 21 µs each | 567 µs |
| └─ deepcopy | 15.5 µs | 420 µs (75% of play_move!) |
| encode_state | 1.5 µs each | 40 µs |
| canonicalize | 8.2 µs each | 222 µs |
| **Total** | | **~850 µs** |

**Root cause:** `deepcopy(self._board)` in `GameState.copy()` is 75% of `play_move` time.

### Proposed Optimization: Undo-Based Move Application

**Current approach (copy-based):**
```python
child_state = state.copy()      # Expensive deepcopy
apply_move(child_state, move)
outcome = solve(child_state)
```

**Proposed (undo-based):**
```python
apply_move_in_place(state, move)  # Mutate directly
outcome = solve(state)
undo_move_in_place(state, move)   # Restore original
```

**Expected speedup:** ~2-3x (eliminates 420 µs of deepcopy per position)

**Risk assessment:** Medium
- Risk: Incomplete undo corrupts state
- Mitigation: Comprehensive tests, try/finally pattern, audit all mutable fields

**Alternative (lower risk):** Optimize `copy()` with shallow copies where safe.

## SQLite Checkpoint Contents

Sample from 30-second solve (64,107 positions):

| Outcome | Count | Percentage |
|---------|-------|------------|
| WIN_P1 | 34,255 | 53.4% |
| WIN_P2 | 29,148 | 45.5% |
| DRAW | 704 | 1.1% |

**Observations:**
- Draws are rare (~1%)
- Early-game positions tend to be WIN_P1
- Late-game (all pieces on board) split between WIN_P2 and DRAW

## Phase 3: Undo-Based Optimization (Complete)

**Files:** `solver/fast_move.py`, `tests/test_fast_move.py`

### Problem Identified

Benchmarking revealed `deepcopy` in `GameState.copy()` consumed 75% of `play_move` time (~15.5µs per copy). With ~27 moves per position, this added ~420µs overhead per position.

### Solution: In-Place Move Application

Implemented `apply_move_in_place()` and `undo_move_in_place()` to mutate state directly and restore it when backtracking.

```python
@dataclass
class UndoInfo:
    move: Move
    piece: Piece
    move_completed: bool  # False if reveal loss
    player_switched: bool  # Track player switch for proper undo

def apply_move_in_place(state: GameState, move: Move) -> tuple[GameResult, UndoInfo]:
    """Apply move by mutating state directly."""

def undo_move_in_place(state: GameState, undo: UndoInfo) -> None:
    """Reverse move to restore previous state."""
```

### Key Implementation Details

1. **Reserve placement:** Decrement reserve, append to board cell
2. **Board move:** Pop from source, append to destination
3. **Reveal rule:** If lifting reveals opponent win, check if can gobble into winning line
4. **Player switching:** Only switch for ongoing games (matches `play_move` behavior)
5. **Undo correctness:** Track all changes in `UndoInfo` for precise reversal

### Solver Integration

Added `_solve_iterative_fast()` method that:
- Uses `apply_move_in_place()` to explore children
- Stores `UndoInfo` in `StackFrame.undo_on_pop`
- Undoes move when backtracking (popping frame from stack)

### Benchmark Results

| Version | Time | Positions/sec | Unique Positions | Result |
|---------|------|---------------|------------------|--------|
| SLOW (copy-based) | >120s | ~750 | incomplete | - |
| FAST (undo-based) | 1.59s | ~1,750 | 9,806 | WIN_P1 |

**Initial (buggy) result:** Fast appeared to complete in 1.59s - this was INCORRECT due to bug below.

### Bug Found and Fixed

**Bug:** In `_solve_iterative_fast`, the outcome was computed AFTER undoing the move:
```python
undo_move_in_place(frame.state, frame.undo_on_pop)  # Restores PARENT's player
outcome = max/min(child_outcomes) based on frame.state.current_player  # WRONG!
```

After undoing, `frame.state.current_player` is the parent's player, not the frame's player.
This caused incorrect min/max decisions, leading to wrong pruning and artificially fast completion.

**Fix:** Compute outcome BEFORE undoing:
```python
outcome = max/min(child_outcomes) based on frame.state.current_player  # CORRECT
undo_move_in_place(frame.state, frame.undo_on_pop)
```

### Corrected Benchmark Results

| Solver | Speed | Notes |
|--------|-------|-------|
| SLOW (copy-based) | ~750 pos/sec | Baseline, known correct |
| FAST (undo-based) | ~2,000 pos/sec | 2.8x speedup |

### Validation (After Bug Fix)

1. **Unit tests:** All 24 tests pass (10 for fast_move, 14 for minimax)
2. **Small position tests:** Fast and slow solvers agree on test positions
3. **Speed:** Fast solver runs at ~2,000 pos/sec vs slow solver's ~750 pos/sec

## Game Solution (COMPLETE)

**Result: Player 1 (first mover) wins with optimal play.**

| Metric | Value |
|--------|-------|
| **Outcome** | **WIN_P1** |
| Time to solve | 1.50 hours |
| Positions evaluated | 7,130,462 |
| Unique positions | 19,931,991 |
| Cache hits | 7,857,505 |
| Terminal positions | 20,020,364 |
| Cycle draws detected | 18,867 |
| Max search depth | 480 |

### Outcome Distribution

| Outcome | Count | Percentage |
|---------|-------|------------|
| WIN_P1 | 10,256,905 | 51.5% |
| WIN_P2 | 9,636,034 | 48.3% |
| DRAW | 39,052 | 0.2% |

### Key Observations

- **First-player advantage confirmed**: P1 wins with optimal play
- **Draws are rare**: Only 0.2% of positions are draws
- **~20 million unique positions**: Within our estimated range (10M-100M)
- **Effective pruning**: Only 7.1M positions evaluated to solve 19.9M unique positions
- **Max depth 480**: Games can last up to 480 plies (very long with piece movement)

### Understanding the Statistics

#### Positions Evaluated vs Unique Positions

- **Positions evaluated (7.1M)**: Non-terminal positions where we computed the outcome by examining all children and applying minimax (max for P1, min for P2). These are positions that went through the full evaluation loop.

- **Unique positions (19.9M)**: Total distinct positions stored in the transposition table.

The difference (~12.8M) consists mostly of **terminal positions** - positions where someone has already won. These are added to the table when discovered during move generation, but don't require full "evaluation" since there are no children to examine.

#### Cache Hits (7.8M)

When exploring a child position, we first check: "Have we already solved this position?" If yes, that's a **cache hit** - we reuse the stored outcome instead of re-solving.

Cache hits occur when the same position is reachable via different move sequences (transpositions):
```
Path A: S(0,0) → M(1,1) → ... → Position X
Path B: M(1,1) → S(0,0) → ... → Position X (same position!)
```
The second time we encounter Position X, we get a cache hit.

#### Terminal Positions (20M)

Count of times we discovered a **game-ending position** (someone won). This count includes duplicates - the same winning position can be discovered via different move paths.

Note: `terminal_positions` (20M) > `unique_positions` (19.9M) because many winning positions are reached multiple times from different game paths.

#### Cycle Draws (18,867)

When we detect we're about to revisit a position **already on the current search path**, that would create an infinite loop. We treat this as a **draw** (corresponding to the threefold repetition rule):
```
Position A → ... → Position B → ... → Position A (cycle detected!)
```
Instead of infinite recursion, we return DRAW for this path.

#### How Symmetry Factors In

**All counts are AFTER symmetry reduction.**

Before storing or looking up any position, we canonicalize it by finding the "smallest" encoding among all 8 D₄ symmetric variants (4 rotations × 2 reflections). For example:
```
S(0,0) ≡ S(0,2) ≡ S(2,0) ≡ S(2,2)  # All corners map to same canonical form
```

This means:
- The 19.9M unique positions represent ~159M "raw" positions (×8 for symmetry)
- Cache hits benefit from symmetry - a rotated/reflected version of a solved position hits the cache
- The effective state space reduction is close to 8× for most positions

#### Summary Relationships

```
unique_positions ≈ positions_evaluated + unique_terminal_positions
cache_hits = repeated encounters of already-solved positions
terminal_positions = total terminal discoveries (includes duplicates from different paths)
```

## Unpruned Exploration Attempt

### Motivation

With alpha-beta pruning, once a player finds their optimal outcome (WIN for current player), we skip exploring other moves from that position. This means some positions reachable only via "suboptimal" moves were never explored.

We attempted to run the solver WITHOUT pruning to discover all reachable positions.

### Implementation

Added parameters to `Solver.solve()`:
- `prune=False`: Disable alpha-beta pruning, explore all children
- `force=True`: Re-explore even if position already solved (to traverse pruned branches)

```python
solver.solve(fast=True, prune=False, force=True)
```

### Test Run Results (2 minutes)

| Metric | Value |
|--------|-------|
| Starting positions | 19,931,991 |
| After 2 min | 20,001,341 |
| New positions found | 69,346 |
| Cache hits | 148,467 |

**Finding:** Only ~69k new positions in 2 minutes, with rate slowing rapidly. This suggests pruning skipped very few unique positions - most were reachable via alternative paths and already solved.

**Estimated coverage:** The pruned solution covers ~99.7% of all reachable positions.

### Full Run Attempt (Failed)

Attempted full unpruned exploration but process crashed after loading checkpoint.

**Symptoms:**
- Process started, loaded 19.9M positions (~6.3GB memory)
- Log shows "Starting unpruned exploration..." then nothing
- Process exited without saving progress
- caffeinate confirmed process terminated

**Likely cause: Out of Memory**

Without pruning:
1. Must explore ALL children before backtracking (can't stop early)
2. Stack grows much deeper and wider simultaneously
3. Each `StackFrame` holds `path: frozenset[int]` that grows with depth
4. With max depth 480 and many more active branches, memory usage explodes

The pruned solver can backtrack quickly (found a win → done), keeping memory bounded. Without pruning, it must hold the entire exploration frontier in memory.

### Conclusion

The pruned solution with **19,931,991 positions** is essentially complete:
- Covers ~99.7% of reachable positions
- Remaining ~0.3% are positions only reachable via suboptimal play
- These positions don't affect the game's solution (P1 wins with optimal play)
- Full unpruned exploration is memory-prohibitive with current implementation

A memory-efficient approach (e.g., iterative deepening, disk-based exploration) would be needed to enumerate all positions.

## Implications for Game Tree Viewer

### What Alpha-Beta Pruning Means for the Database

The pruning behavior differs by whose turn it is:

**At P1-to-move positions (outcome WIN_P1):**
- P1 (maximizer) found a winning move → stopped exploring other moves
- P1's suboptimal moves were **pruned and may NOT be in database**

**At P2-to-move positions (outcome WIN_P1):**
- P2 (minimizer) searched ALL moves looking for a better outcome (WIN_P2 or DRAW)
- Found none → confirmed all P2 moves lead to WIN_P1
- **All P2 moves ARE in database** (P2 had to exhaustively check them)

### Coverage by Player

| Player to Move | Can query any move? | Reason |
|----------------|---------------------|--------|
| **P1** | ❌ Only optimal moves guaranteed | Suboptimal moves were pruned after finding a win |
| **P2** | ✅ All moves available | Exhaustively searched while looking for defense |

### Implications for UI/Viewer

**"Play vs Solver" mode works best as:**
- Human plays as **P2** (the losing side with optimal play)
- Human can try ANY move - all are in the database
- Solver plays P1 and always has optimal response available
- Demonstrates "here's why you can't escape P1's winning strategy"

**Showing P1's winning path:**
- Must follow optimal P1 moves (others may be missing)
- Can branch on any P2 response to show P1's refutation

**Constraint:**
- If P1 plays a suboptimal move that was pruned, we fall off the explored tree
- That subtree has no data (would need unpruned solve to support)

### Example Walkthrough

```
Initial position (WIN_P1, P1 to move)
├── P1 optimal move → Position A (WIN_P1, P2 to move) ✅ IN DATABASE
│   ├── P2 move 1 → Position B (WIN_P1) ✅ IN DATABASE
│   ├── P2 move 2 → Position C (WIN_P1) ✅ IN DATABASE
│   └── P2 move 3 → Position D (WIN_P1) ✅ IN DATABASE
│       └── P1 optimal → ... ✅ IN DATABASE
├── P1 suboptimal → Position X (DRAW?) ❓ MAY BE MISSING
└── P1 suboptimal → Position Y (WIN_P2?) ❓ MAY BE MISSING
```

P2's moves are all present because P2 exhaustively searched for an escape.
P1's non-winning moves may be missing because P1 stopped after finding a win.

## Full Solve Strategy (2024-12-18)

### Goal

Complete exploration of ALL reachable positions without alpha-beta pruning, to support:
- Full game tree viewer (including suboptimal P1 moves)
- Complete analysis of all lines of play

### Memory Analysis

Analyzed actual memory usage of the transposition table:

| Metric | Value |
|--------|-------|
| Positions loaded | 19,931,991 |
| Dict object size | 640 MB |
| Key size (int) | 28 bytes |
| Value size (Outcome enum) | 60 bytes |
| Total memory | ~1.2 GB |
| **Bytes per position** | **~66 bytes** |

**Finding:** The `Outcome` IntEnum costs 60 bytes vs 28 for a plain int. Potential optimization: store raw ints instead of enums (~32 bytes saved per position).

### Subtree Solving Approach

**Problem:** Solving from initial position without pruning is memory-intensive. The exploration stack can grow very deep (~500+ levels), and without early pruning cutoffs, many more branches must be held simultaneously.

**Proposed solution:** Divide and conquer by solving subtrees first.

1. **Collect intermediate positions:** Do DFS to depth N (e.g., 400), recording positions WITHOUT solving
2. **Solve each subtree:** For each recorded position, run solver starting from that position
3. **Solve from root:** With subtrees cached, full solve hits cache at depth N

**Why this helps:** Each subtree solve starts fresh with low stack usage. Max depth of each solve is reduced (game depth ~500, so depth-400 subtrees have ~100 remaining depth).

### Correctness Analysis: Why Path History Doesn't Matter

**Key question:** When solving a subtree from position S, do we need the history of how we reached S?

**Answer:** No. A position's minimax outcome is independent of the path taken to reach it.

**Reasoning:**

1. **What minimax computes:** For position X, we compute "optimal outcome from X with optimal play from both sides."

2. **Cycle handling:** If X can reach itself (X → A → B → X), then either player can force a draw by repeating the cycle until threefold repetition. The minimax value of X accounts for this.

3. **Path variable:** The solver's `path` tracks positions on the *current exploration path*, not game history. It detects cycles within a single DFS traversal:
   ```python
   if child_canonical in frame.path:
       frame.child_outcomes.append(Outcome.DRAW)  # Cycle found
   ```

4. **Subtree solving:** When we solve from S with empty path:
   - S is added to children's paths: `new_path = frame.path | {frame.canonical}`
   - If any descendant cycles back to S, S is in the path → cycle detected
   - Cycles through "earlier" game positions are also detected (they're on the current exploration path)

5. **Transposition table:** Cached outcomes are valid regardless of how we reached the position. If position Z is solved, its outcome already accounts for all cycles reachable from Z.

**Conclusion:** Starting a subtree solve with empty path is correct. The algorithm properly detects cycles and computes correct minimax values.

### Concern: Reaching "Earlier" Positions

From depth-400 position S, we might reach positions Z that are also reachable from depth 100 (board moves can "undo" progress).

**Analysis:**
- If Z is in transposition table: cache hit, use stored outcome
- If Z is not in table (was pruned): we solve Z, adding to cache
- Z's subtree might be large, but solving it benefits all future exploration
- Original solve covered ~99.7% of positions, so most are already cached

**Mitigation:** Incremental checkpointing ensures we never lose progress.

### Depth Considerations

**With pruning:** Max depth observed was ~500.

**Without pruning:** Could be deeper. Players could make moves that deliberately avoid winning, potentially reaching depths bounded only by the threefold repetition rule.

**Threefold repetition bound:** Each position can appear at most 3 times before draw. With ~20M positions, theoretical max is enormous but practically:
- Most paths end in wins much earlier
- Draws kick in after 3 repetitions
- Sub-solving at depth 400 should still help by caching partial results

### Repetition Rule Clarification

**Our solver:** Marks single repetition (cycle in current path) as DRAW.

**Game rules:** Threefold repetition (position appears 3 times) = DRAW.

**Why this is correct for minimax:** If a cycle exists, either player can force it to repeat 3 times. The ability to force a draw means the minimax value is at least DRAW. Computing the exact number of moves to achieve the draw isn't necessary for determining the optimal outcome.

**UX implication:** When displaying "DRAW" to users, clarify this means "theoretically drawn with optimal play via eventual threefold repetition" - the game continues until repetition actually occurs.

### Planned Infrastructure Improvements

Before attempting full solve:

1. **Time-based checkpointing:** Save progress every ~60 seconds (not per-position, not every 100k)
2. **Memory logging:** Track and log memory usage during solve
3. **Progress logging:** Timestamps, positions/sec, estimated completion
4. **Crash diagnostics:** Signal handlers to log state before exit

### Implementation Plan

1. Add time-based checkpointing to solver
2. Add memory/progress logging
3. Create script to collect positions at depth N (no solving, just recording)
4. Test subtree approach with a few depth-400 positions
5. If successful, batch solve all collected subtrees
6. Run full solve from initial position (should hit cache quickly)

## Infrastructure Improvements (2024-12-18)

### New Files Created

**`solver/checkpoint.py` - IncrementalCheckpointer class:**
```python
class IncrementalCheckpointer:
    """
    Manages incremental checkpointing to SQLite.
    - Tracks which positions have been saved
    - Only writes NEW positions on checkpoint (incremental saves)
    - Time-based triggering (default: 60 seconds)
    """
```

Key methods:
- `initialize(solver)` - Load checkpoint, track saved positions
- `maybe_checkpoint(solver)` - Save if enough time has passed
- `force_checkpoint(solver)` - Save all new positions immediately

**`solver/robust_solve.py` - Enhanced solver script:**
- Time-based checkpointing (configurable interval)
- Memory usage logging via `resource.getrusage()`
- Progress logging with timestamps
- Signal handling for graceful shutdown (Ctrl+C saves checkpoint)
- Command line args: `--checkpoint-interval`, `--log-interval`, `--no-prune`, `--force`

**`solver/collect_positions.py` - Position collector:**

Collection methods:
- `unsolved` - DFS to find frontier of unsolved positions (fast, limited count)
- `enumerate` - BFS to find ALL unsolved positions with minimum depth (comprehensive)
- `random` - Random walks (not effective for deep positions - games end in ~15 moves)
- `dfs` - Collect at fixed depth

Key insight: Random walks don't reach deep positions because random play leads to quick wins/losses (avg game length ~14 moves). The deep positions exist but require systematic exploration through the solved region.

**`solver/solve_subtrees.py` - Subtree solver (created, not yet tested):**
- Takes list of canonical positions from JSON
- Solves each position one-by-one
- Incremental checkpointing between solves

### Checkpointing Implementation Details

The `_report_interval` (1000 positions) controls how often we CHECK the clock, not how often we save:

```python
def report_progress():
    # Called every 1000 positions
    now = time.time()
    if now - last_log_time >= config.log_interval_sec:
        # Log progress (every ~30 seconds)

    saved = checkpointer.maybe_checkpoint(solver)
    # maybe_checkpoint checks if 60 seconds have passed
    # Only writes to DB if enough time elapsed
```

This avoids database write overhead while still being responsive to time.

### Position Enumeration Strategy

**Problem with fixed-depth collection:**
- Random walks end too quickly (avg 14 moves, max ~50)
- Can't reach depth-400 positions via random play
- Deep positions exist but require exploring through solved region

**Solution - enumerate method:**
1. BFS from initial position through SOLVED positions only
2. When we hit an unsolved position, record it with its depth
3. BFS guarantees minimum depth for each position
4. Output sorted by depth descending (deepest first)

**Solving order (deepest first):**
- Solve deepest subtrees first
- Their results get cached
- Shallower solves hit cache when reaching those positions
- Bottom-up approach maximizes cache utilization

### Test Results

Quick test of `unsolved` method:
- Loaded 19,972,877 solved positions
- Explored only 153 nodes to find 100 unsolved positions
- Unsolved positions are right at the frontier (very close to solved region)

Quick test of robust solver (unpruned, 30 seconds):
- Found and saved 40,886 new positions
- Graceful shutdown worked correctly
- Checkpoint saved on interrupt

### Current Database State

```
solver/gobblet_solver.db: 309 MB, 19,972,877 positions
```

This includes:
- Original pruned solve: 19,931,991 positions
- Additional positions from unpruned testing: ~41,000 positions

## BFS Collector Optimization (2024-12-19)

### Problem

The BFS collector for enumerating unsolved positions was using `deepcopy` to store GameState objects in the queue. With queue sizes reaching 100k+, this caused:
- High memory usage (~200 bytes per queue entry)
- Slow deepcopy operations

### Solution: Encoding-Based Queue

Instead of storing GameState objects, store 64-bit canonical encodings:

```python
# Before (state queue):
child_state = copy.deepcopy(state)
queue.append((child_state, depth + 1))  # ~200 bytes

# After (encoding queue):
child_encoded = canonicalize(encode_state(state))
queue.append((child_encoded, depth + 1))  # 16 bytes

# When processing, decode:
encoded, depth = queue.popleft()
state = decode_state(encoded)
```

### Benchmark Results (60-second runs)

| Metric | Encoding Queue | State Queue | Improvement |
|--------|---------------|-------------|-------------|
| **Speed** | 1,472 nodes/s | 1,255 nodes/s | **17% faster** |
| **Nodes processed** | 89,445 | 76,330 | **17% more** |
| **Unsolved found** | 1,087,426 | 933,193 | **17% more** |
| **Memory @ 70k nodes** | ~24 MB | ~48 MB | **50% less** |

**Why it's faster:** `decode_state()` creates objects deterministically, while `deepcopy()` has overhead tracking object references.

### Key Finding: Massive Unsolved Frontier

In just 60 seconds of BFS (reaching depth 8-9), we found **1+ million unsolved positions**:

| Depth | Unsolved Count |
|-------|---------------|
| 1 | 8 |
| 3 | 233 |
| 4 | 1,127 |
| 5 | 12,957 |
| 6 | 63,007 |
| 7 | 241,669 |
| 8 | 603,236 |

The frontier grows exponentially. Alpha-beta pruning skipped many branches, leaving millions of unexplored positions.

### Potential Further Optimizations

**1. Pure bit operations (biggest potential win)**
- Eliminate all object creation (GameState, Piece, Move)
- Move generation, application, win detection all on 64-bit integers
- Estimated speedup: 3-10x
- Complexity: High (reveal rule logic is tricky)

**2. Incremental encoding**
- Instead of `encode_state()` from scratch each time, update encoding based on move
- A move changes at most 2 cells - just flip those bits
- Estimated speedup: 10-20%

**3. Faster canonicalization**
- Currently generates all 8 D₄ transforms, takes minimum
- Could precompute lookup tables for bit transforms
- Estimated speedup: 5-15%

**4. Cython/C extension**
- Rewrite hot path in C
- Estimated speedup: 10-100x
- Complexity: High (different language, build system)

**5. Symmetry-aware BFS**
- Skip generating children if a symmetric version was already visited
- Could reduce nodes explored by up to 8x (theoretical max)

**6. Apply move directly to encoding**
- Middle-ground approach: `apply_move_to_encoding(encoded, move) -> new_encoded`
- Skip decode→encode round-trip for children
- Keep move generation logic unchanged

### Current Cost Breakdown (per BFS node, ~0.7ms)

| Operation | Cost | Notes |
|-----------|------|-------|
| `decode_state()` | ~15 μs | Create GameState, Piece objects |
| `generate_moves()` | ~30 μs | Find pieces, destinations, reveal rule |
| Per move (×25): | ~750 μs | apply, encode, canonicalize, undo |
| Lookups | ~10 μs | visited set, solver.table |
| **Total** | ~800 μs | ~1,250 nodes/sec |

## Full Enumeration Run (2024-12-19)

### Run Configuration

```bash
caffeinate -i -w $$ python -m solver.collect_positions \
    --method enumerate \
    --output solver/unsolved_frontier.json \
    2>&1 | tee solver/enumerate.log
```

- Started: 05:27 AM
- Checkpoint interval: 5 minutes
- Initial solved positions: 19,972,877

### Progress Timeline

| Time | Elapsed | Nodes | Unsolved Found | Queue Size | Depth | Rate |
|------|---------|-------|----------------|------------|-------|------|
| 05:36 | 9 min | 100k | 1.1M | 111k | 8 | 185/s |
| 06:17 | 50 min | 1M | 8.4M | 508k | 10 | 330/s |
| 07:07 | 100 min | 2M | 16.8M | 921k | 11 | 340/s |
| 08:07 | 160 min | 3.86M | 26.7M | 1.3M | 13 | 402/s |

### Final Results

| Metric | Value |
|--------|-------|
| **Total unsolved positions** | **26,728,674** |
| Depth range | 1 - 14 |
| BFS nodes explored | ~3.86M |
| Output file size | 1.8 GB |
| Peak memory | 12.7 GB |

### Depth Distribution

| Depth | Positions | % of Total |
|-------|-----------|------------|
| 1 | 8 | <0.01% |
| 3 | 233 | <0.01% |
| 4 | 1,127 | <0.01% |
| 5 | 12,957 | 0.05% |
| 6 | 63,007 | 0.24% |
| 7 | 241,669 | 0.90% |
| 8 | 734,439 | 2.75% |
| 9 | 1,753,599 | 6.56% |
| 10 | 3,198,474 | 11.97% |
| 11 | 5,118,472 | 19.15% |
| 12 | 6,499,836 | 24.32% |
| 13 | 6,846,465 | 25.62% |
| 14 | 2,258,388 | 8.45% |

### Issue: Garbage Collection Stall

After ~2.7 hours (08:07), progress stalled. Symptoms:
- BFS progress logs stopped (every 10k nodes)
- Only checkpoint logs appeared (every 5 min)
- Checkpoint showed barely any new positions (~10 per 5 min)
- Process at 100% CPU

**Root cause:** Python's garbage collector.

Using `sample` to profile the running process:
```
815/817 samples in _PyGC_Collect -> mark_all_reachable
  - set_traverse: 405 samples
  - dict_traverse: 102 samples
```

With ~70M+ objects in memory (26M unsolved dict, 24M+ visited set, 20M solver.table, 1.3M queue), Python's cyclic garbage collector spent nearly all CPU time traversing objects to check for cycles.

**Why this happens:**
- Python's GC periodically scans ALL objects to find reference cycles
- With tens of millions of objects, each GC scan takes minutes
- GC is triggered automatically, and frequent scans starve the actual work
- Our data structures (sets/dicts of integers) have NO cycles - GC work is wasted

**Solution:** Disable cyclic GC during BFS. Reference counting (non-cyclic cleanup) still works.

```python
import gc
gc.disable()  # Disable cyclic GC
# ... BFS loop ...
gc.enable()   # Re-enable after
```

### Outcome Assessment

**Is the frontier complete?**

The BFS reached depth 14 with 1.3M items remaining in queue at depth 13. The pattern shows:
- Peak at depth 13 (6.8M positions)
- Drop-off at depth 14 (2.3M positions)
- Very few new positions being discovered (only ~300 in 4 hours before GC stall)

**Assessment:** We have captured most of the frontier. The BFS was approaching completion - the queue was draining and finding mostly already-visited positions. Some depth 14-15 positions may be missing.

**Mitigation:** When solving subtrees, the solver will naturally discover any missing positions through exploration. The transposition table ensures we never solve the same position twice.

### File Format Note

The 1.8 GB JSON file with 26.7M positions is approaching practical limits:
- Loading requires ~4-8 GB RAM (Python object overhead)
- Parsing takes several minutes

For larger datasets, consider:
- **JSONL** (line-delimited) - streamable, incremental processing
- **SQLite** - queryable, doesn't require full load
- **Binary format** - most compact (9 bytes/position vs ~50 for JSON)

Current format works but is at the edge of practicality.

## Memory Efficiency Analysis (2024-12-19)

### Problem: Subtree Solver OOM at 18GB

Attempted to solve first position from the unsolved frontier. Process ran for 4 hours then was killed (OOM) at 18.3GB memory.

**Final stats before kill:**
- max_depth: 26,291,530 (26 million!)
- New positions: +20,907,618 (~21M solved)
- Cache hits: 83,167,571
- Memory: 18,286 MB

**Key insight:** The game tree without pruning goes EXTREMELY deep. Players can shuffle pieces around indefinitely, creating paths millions of moves long before cycles repeat.

### Issue #1: Path Tracking - O(depth²) Memory

**Problem:** Each `StackFrame` stored `path: frozenset[int]` containing all ancestor positions for cycle detection. At depth D, each frame held a frozenset of size D. Total memory: O(D²).

At depth 84k (first crash), this meant:
- Frame 1: frozenset of 1 element
- Frame 2: frozenset of 2 elements
- ...
- Frame 84,000: frozenset of 84,000 elements
- Total: 1 + 2 + ... + 84,000 = ~3.5 billion integers stored

**Solution:** Use a single shared `path_set: set[int]` for the entire DFS. Add when pushing a frame, remove when popping. Memory: O(D).

```python
# Before (per-frame frozenset):
new_path = frame.path | {frame.canonical}  # O(depth) copy each time
child_frame = StackFrame(..., path=new_path)

# After (shared mutable set):
path_set.add(child_canonical)   # O(1) when pushing
path_set.discard(frame.canonical)  # O(1) when popping
```

**Result:** With this fix, reached depth 26M with stable ~18GB memory (vs crashing at 84k before). The 18GB was from other sources (see below).

### Issue #2: Transposition Table - 7.5x Overhead

**Current storage:** `dict[int, Outcome]` with Python objects

| Component | Size | Notes |
|-----------|------|-------|
| Python int (key) | 32 bytes | 64-bit value + object header |
| Dict overhead | 52 bytes/entry | Hash, pointers, sparse slots |
| Outcome enum | ~0 | Singletons (shared) |
| **Total** | **84 bytes/entry** | |

**Optimal:** 8 bytes (int64 key) + 1 byte (int8 value) = **9 bytes/entry**

| Positions | Current | Optimal | Overhead |
|-----------|---------|---------|----------|
| 41M | 2.8 GB | 370 MB | 7.5x |
| 100M | 6.8 GB | 900 MB | 7.5x |
| 1000M | 68 GB | 9 GB | 7.5x |

### Issue #3: DFS Stack with Pre-computed Moves

Each `StackFrame` pre-computes all legal moves (10-20 per position) as tuples:
```python
moves: list[tuple[Move, int, GameResult]]  # Move object, canonical, result
```

At depth 26M with ~15 moves per frame:
- 26M frames × 15 tuples × ~100 bytes = **~39 GB** (theoretical)

This is likely the dominant memory consumer at extreme depths.

### Issue #4: SQLite Storage - 73% Overhead

**Current:** 15.6 bytes/row (608 MB for 41M rows)
**Optimal:** 9 bytes/row (8-byte key + 1-byte value = 369 MB)

SQLite overhead comes from B-tree structure, row format, and page slack.

### Optimization Ideas

**1. Transposition table - use compact arrays:**
```python
# Instead of dict[int, Outcome]:
keys = np.array([...], dtype=np.int64)    # 8 bytes each
values = np.array([...], dtype=np.int8)   # 1 byte each
# Total: 9 bytes/entry vs 84 bytes

# For O(1) lookup: numba.typed.Dict[int64, int8] or custom hash table
```

**2. Store raw ints throughout hot loop:**
```python
# Avoid object conversions in tight loops:
outcome = 1  # Not Outcome.WIN_P1
# Only convert at API boundaries
```

**3. Lazy move generation:**
```python
# Instead of pre-computing all moves per frame:
moves: Iterator[Move]  # Generate on demand
# Reduces per-frame memory from ~1.5KB to ~100 bytes
```

**4. Flat binary checkpoint format:**
```python
# Instead of SQLite with 73% overhead:
np.savez_compressed('checkpoint.npz', keys=keys, values=values)
# 41M entries → ~350 MB compressed (vs 608 MB SQLite)
```

**5. Disk-backed lookup for extreme scale:**
```python
# If transposition table exceeds RAM:
# - Memory-map sorted arrays with binary search
# - Or use LMDB/RocksDB for disk-backed hash table
# Trade-off: slower lookup but bounded memory
```

### Priority Order

1. **Compact transposition table** (biggest win for memory)
   - numpy arrays or numba.typed.Dict
   - 7.5x memory reduction

2. **Lazy move generation** (helps extreme depth)
   - Generate moves on-demand instead of pre-computing
   - Reduces stack memory

3. **Binary checkpoint format** (faster save/load)
   - numpy save/load vs SQLite
   - Also smaller on disk

4. **Raw ints in hot loop** (speed optimization)
   - Avoid Outcome enum in solver core
   - Convert only at boundaries

## Subtree Solve Progress (2024-12-19)

### Test Run Results

Ran solve on first position from unsolved frontier (before implementing optimizations above):

| Metric | Value |
|--------|-------|
| Runtime | ~4 hours |
| max_depth | 26,291,530 |
| Positions solved | +20,907,618 |
| Memory at OOM | 18,286 MB |
| Exit | Killed (OOM) |

**Checkpoint saved:** 40,924,386 total positions in SQLite (21M new + 20M existing).

### Observation: Extreme Depth Without Pruning

Without alpha-beta pruning, the DFS explores ALL branches. Players can make arbitrary moves including "wasting" moves that shuffle pieces without making progress toward a win. This creates game trees millions of moves deep.

The path optimization (O(depth) instead of O(depth²)) made this feasible, but we still hit memory limits from the transposition table and stack.

### Current Database State

```
solver/gobblet_solver.db: 609 MB, 40,924,386 positions
```

Future runs will cache-hit on these positions, avoiding re-exploration.

## Next Steps

1. ~~Run full enumeration~~ ✅ Complete (26.7M positions)

2. ~~Fix path tracking memory~~ ✅ Complete (shared path_set)

3. ~~Fix GC issue~~ ✅ Complete (gc.disable() in solver)

4. **Implement compact transposition table:**
   - Switch from dict[int, Outcome] to numpy arrays or numba.typed.Dict
   - Target: 7.5x memory reduction

5. **Implement lazy move generation:**
   - Generate moves on-demand instead of pre-computing all
   - Reduces per-frame memory at extreme depths

6. **Continue subtree solving:**
   - With memory optimizations, should handle deeper exploration
   - 26.7M positions to solve, ~41M already cached

7. **Build optimal play UI:**
   - Show best move from any position
   - Visualize winning path
   - Add "play vs solver" mode
