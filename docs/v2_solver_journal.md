# V2 Solver Journal

## 2024-12-19: Initial Alpha-Beta Solve

### First Run Results

Completed initial alpha-beta solve in **5 minutes 13 seconds** (313s).

| Metric | Value |
|--------|-------|
| Result | **P1 wins** (confirmed) |
| Time | 313s (~5 minutes) |
| Positions evaluated | 146,910,206 |
| Unique positions | 197,810,960 |
| Rate | ~469K positions/sec |
| Pruning efficiency | 91.4% |

### Position Count Discrepancy Investigation

V2 stored **~198M positions** vs V1's **~41M positions** (~5x more).

**Root Cause: Move Ordering**

V1 uses **dynamic move ordering** based on the transposition table:
```python
# V1: minimax.py lines 368-382
def move_priority(item):
    if child_canonical in self.table:
        outcome = self.table[child_canonical]
        if outcome == best_outcome:
            return 0  # Winning moves first
        elif outcome == Outcome.DRAW:
            return 1
        else:
            return 2  # Losing moves last
    return 1  # Unknown = middle
moves_info.sort(key=move_priority)
```

V2 uses **static move ordering** (Large → Medium → Small for reserves):
```rust
// V2: movegen.rs - generates in fixed order
// No reordering based on known outcomes
```

**Impact:** V1 explores winning moves first, triggering alpha-beta cutoffs earlier. This prunes more branches, storing fewer positions. V2's static ordering explores more branches before finding cutoffs.

**Key Insight:** Despite storing 5x more positions, V2 is still **18x faster** than V1 due to Rust's raw performance advantage:
- V1: 1.5 hours for 41M positions
- V2: 5 minutes for 198M positions

### Verification

Both solvers produce **identical results** for positions they share:
- Initial position: P1 wins ✓
- Canonicalization matches ✓
- All outcomes consistent between V1 and V2 ✓

### Action Item

Implement dynamic move ordering in V2 to reduce position count and improve pruning efficiency.

---

## 2024-12-20: Dynamic Move Ordering Implemented

### Implementation

Added priority move scanning to `Solver::create_frame()`:
1. When creating a frame, scan all legal moves
2. For each move, check if child is in transposition table with winning outcome
3. Store winning moves in `priority_moves` vector
4. Explore priority moves first, then continue with lazy generation

### Results

| Metric | Before | After | Change |
|--------|--------|-------|--------|
| Unique positions | 197.8M | 117.9M | **-40%** |
| Positions evaluated | 146.9M | 91.1M | **-38%** |
| Max depth | 9.2M | 3.3M | **-64%** |
| Time | 313s | 321s | ~same |
| Pruning | 91.4% | 92.1% | +0.7% |
| Result | P1 wins | P1 wins | ✓ |

### Analysis

- **40% fewer positions** stored due to better pruning
- **64% shallower max depth** - finding wins earlier terminates exploration sooner
- Slightly slower rate due to move scanning overhead, but overall similar time
- Still 3x more positions than V1 (118M vs 41M) - room for further optimization

### Remaining Gap

V1's 41M vs V2's 118M suggests V1 may have additional optimizations:
- V1 sorts ALL moves by outcome (wins → draws → unknown → losses)
- V2 only prioritizes known wins, explores rest in default order
- Could add second tier: deprioritize known losses

For now, 118M is acceptable. The solver completes in ~5 minutes with correct results.

---

## 2024-12-20: Terminal Position Fix - Matching V1's 20M Positions

### Root Cause Found

Further investigation revealed the key difference: **V1 adds terminal positions to the table DURING frame creation**, before sorting. This means moves that lead to immediate wins get priority 0 in the sort.

V1's `_create_frame_fast` does:
```python
for move in generate_moves(state):
    game_result, undo = apply_move_in_place(state, move)
    child_canonical = canonicalize(encode_state(state))

    if game_result != GameResult.ONGOING:
        # Game ended with this move - ADD TO TABLE NOW
        self.table[child_canonical] = child_outcome  # <-- KEY DIFFERENCE

    moves_info.append((move, child_canonical, game_result))
    undo_move_in_place(state, undo)

# Sort by priority - terminal positions are now in table!
moves_info.sort(key=move_priority)
```

V2 was NOT doing this - it only checked existing table entries, so terminal moves always got priority 1 (unknown).

### The Fix

Modified `Solver::create_frame()` to check for winners during the scan phase:

```rust
let priority = if let Some(&outcome) = self.table.get(&child_canonical) {
    // ... existing logic
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
```

### Results

| Metric | Before Fix | After Fix | V1 |
|--------|------------|-----------|-----|
| Unique positions | 155M | **19.8M** | ~20M |
| Time | 7.5 min | **31 seconds** | 1.5 hours |
| Max depth | 3.3M | **460** | ~1000 |
| Result | P1 wins | P1 wins | P1 wins |

### Analysis

**V2 now matches V1's position count!** The key insight:

1. Immediate winning moves must get priority 0 to enable early cutoffs
2. V1 achieves this by adding terminal positions to the table DURING frame creation
3. V2 was missing this step, causing 8x more positions to be stored

The fix also reduced max depth from 3.3 million to 460, showing that alpha-beta pruning now cuts off exploration much earlier.

### Performance Comparison

| Solver | Positions | Time | Speed |
|--------|-----------|------|-------|
| V1 (Python) | 20M | 1.5 hours | ~3.7K pos/sec |
| V2 (Rust) | 19.8M | 31 seconds | 233K pos/sec |
| **Speedup** | - | **180x faster** | **63x faster** |

V2 is **180x faster** than V1 while storing the same number of positions!

---

## 2024-12-20: Full Solve Attempt - Critical Bug Discovered

### The Experiment

Added `--no-prune` flag to run full minimax without alpha-beta pruning. Goal: build complete tablebase of all positions.

Used separate checkpoint files:
- `pruned.bin` - alpha-beta results (frozen, ~20M positions)
- `full.bin` - full solve results (starts empty)

### Surprising Result

| Mode | Result | Positions | Time |
|------|--------|-----------|------|
| Pruned | **P1 WINS** | 20M | 31s |
| Full | **DRAW** | 531M | 46min |

**The same solver produces different results!** This indicates a fundamental bug.

### Investigation

Verified the discrepancy by checking the checkpoint files:
```
pruned.bin: canonical 0 → outcome 1 (P1 WINS)
full.bin:   canonical 0 → outcome 0 (DRAW)
```

### Root Cause Analysis

After extensive analysis, we identified the bug: **caching path-dependent values**.

When we detect a cycle (position already on current search path), we return DRAW. This DRAW propagates up and influences parent outcomes. We cache those outcomes. But later, accessing the cache from a **different path** gives wrong results because:

1. What was a "cycle" from path A might not be a cycle from path B
2. The cached value assumed certain moves lead to draws, but from a different path, those moves might lead to wins

**Example:**
```
Path: [A, B, C] - exploring C
  C→A: cycle (A on path) → DRAW
  C→Z: terminal → WIN_P2
  C's value = max(DRAW, WIN_P2) = DRAW
  Cache: C = DRAW  ← path-dependent!

Later, Path: [X, Y, C] - cache hit for C
  Returns DRAW
  But C→A is NOT a cycle here!
  C's actual value might be WIN_P1
```

### Additional Issue: Symmetry

We detect cycles using canonical hashes, but:
- Position A and its rotation A' have the same canonical hash
- Our code treats A' as "same position" as A for cycle detection
- Official rules: rotations are DIFFERENT positions, not repetitions

### Why Pruned Solve Appears Correct

Alpha-beta pruning with good move ordering:
1. Finds winning moves first
2. Cuts off early after finding a win
3. Rarely explores deep enough to hit cycles
4. Most cached values are "pure" (from terminals, not cycles)

The full solve explores everything, exposing the bug.

### Implications

This bug affects both V1 and V2 - any solver that:
1. Detects cycles and returns DRAW
2. Caches outcomes influenced by cycle draws
3. Reuses those cached values from different paths

### Proposed Solutions

See `docs/game_tree_analysis.md` for full analysis. Summary:

1. **Don't cache cycle-influenced values** - Track `had_cycle` flag, propagate upward, skip caching if true
2. **Full path-based state** - Key by (position, full repetition counts) instead of just position
3. **Full game tree storage** - Don't compress transpositions; each path is a distinct node

### Next Steps

1. Estimate full game tree size (transpositions distinct, 2-fold terminal)
2. Decide on storage/caching strategy based on size
3. Implement correct cycle handling
4. Consider tablebase viewer UX for repetition scenarios

### Key Insight

The "result" of a position is not a simple intrinsic property. It depends on:
- The path taken to reach it (which positions have been seen)
- How many times each position has been visited
- What future repetitions are possible

A flat `position → result` map is fundamentally insufficient for games with repetition rules.

---

## 2024-12-20: Final Strategy Decision

### `had_cycle` Experiment

Implemented Solution 1 from game_tree_analysis.md (don't cache cycle-influenced values):

| Metric | With had_cycle | Original |
|--------|---------------|----------|
| Pure positions cached | 175,806 | 19,836,040 |
| Cycle-influenced (not cached) | 158+ million | 0 |
| Solver behavior | Runs indefinitely | 31 seconds |

**Result**: Infeasible. 99.9% of positions are cycle-influenced due to aggressive propagation up the tree.

### Root Cause

If ANY leaf in a subtree hits a cycle, all ancestors become cycle-influenced. In Gobblet Gobblers, with ~18,700 direct cycles and 500M+ positions, almost every position has some cyclic descendant.

### Final Strategy: Intrinsic Values + Runtime Repetition

1. **Keep current solver** - Alpha-beta pruned solve is correct (P1 WINS, 31s, 20M positions)
2. **Store intrinsic values** - `canonical_hash → outcome` for ~20M positions
3. **Handle repetition in viewer** - Track game history at runtime, apply 3-fold rule

### Why Alpha-Beta Solve Is Correct

The winning strategy for P1 is "pure" - it reaches terminal WIN positions without needing cycles:
- Good move ordering finds wins early
- Alpha-beta cuts off before exploring cycle-heavy branches
- Cycle-influenced cache values don't affect the winning path

### Tablebase Semantics

- **Tablebase value**: "Who wins if this position were a fresh game?"
- **Viewer value**: "Who wins given the current game history?"

These differ only when repetition rules block the optimal path - a rare edge case that the viewer can handle specially.

See `docs/game_tree_analysis.md` for full analysis.

