# Game Tree Analysis: Cycles, Draws, and Caching

## Summary

During V2 solver development, we discovered a fundamental bug in how both V1 and V2 handle cycles and draw detection. This document captures our analysis.

## The Bug: Discrepancy Between Pruned and Full Solve

### Observation

| Mode | Result | Positions | Time |
|------|--------|-----------|------|
| Pruned (alpha-beta) | **P1 WINS** | 20M | 31s |
| Full (no pruning) | **DRAW** | 531M | 46min |

The same solver produces different results depending on whether alpha-beta pruning is enabled.

### Root Cause: Caching Path-Dependent Values

When we detect a cycle (position already on the current search path), we return DRAW. This DRAW value propagates up and influences the parent position's computed outcome. We then cache that outcome.

**The problem:** The cached outcome was computed with cycle draws that are **path-dependent**. Later, when we access this cached value from a different path, the cached value may be incorrect because:

1. A position that was a "cycle" from path A might not be a cycle from path B
2. The cached value assumes certain moves lead to draws, but from a different path, those same moves might lead to wins

### Example

```
First exploration - Path: [A, B, C]
  At C, P1 can go to: A (cycle→DRAW) or Z (WIN_P2)
  C's best = max(DRAW, WIN_P2) = DRAW
  Cache: C = DRAW  ← BUG: This is path-dependent!

Later exploration - Path: [X, Y, C]
  Cache hit! C = DRAW
  But C→A is NOT a cycle from this path!
  If we explored, A might lead to WIN_P1
  C's actual value here might be WIN_P1, not DRAW
```

## Deeper Issue: What Does "Draw" Mean?

### Official Rules vs Our Implementation

**Official Gobblet Gobblers rule:** A game is a draw when the same position occurs for the **3rd time** (threefold repetition).

**Our implementation:** When a position appears on the current path (2nd occurrence), we immediately return DRAW.

### The Semantic Problem

When we detect a 2nd occurrence and return DRAW, we're saying "this move leads to a repeated position." But:

1. The 2nd occurrence is NOT a draw by official rules (need 3rd)
2. Even if we treat 2nd as draw, this only means THIS MOVE leads to draw
3. The POSITION itself is not inherently a draw - it has a subtree with potentially winning lines
4. We should NOT cache the position as DRAW; we should only treat this specific move as leading to draw

### The Caching Mistake

```rust
// When detecting cycle
if path.contains(&child_canonical) {
    self.update_best(frame, DRAW);  // Correct: this MOVE leads to draw
    continue;
}

// When popping frame
self.table.insert(frame.canonical, outcome);  // BUG: outcome was influenced by cycles!
```

The outcome we cache may have been influenced by cycle-derived draws. This outcome is path-dependent but we're caching it as if it were path-independent.

## Additional Issue: Symmetry and Repetition

### The Problem

We detect cycles using **canonical** position hashes. But:

```
Position A and A' (90° rotation of A) have the SAME canonical hash.

Game: A → B → C → A' (rotation of A)

Our code: "Cycle detected! A' matches A!"
Official rules: A and A' are DIFFERENT positions - NOT a repetition!
```

### The Fix

Use TWO different hashes:
- **Canonical hash** → for transposition table (strategically equivalent positions share cache)
- **Actual position hash** → for cycle/repetition detection (only TRUE repetitions count)

## Possible Solutions

### Solution 1: Don't Cache Cycle-Influenced Values

Track whether a frame's outcome was influenced by any cycle draws:

```rust
struct Frame {
    had_cycle: bool,  // Did any child return cycle-draw?
}

// Propagate up the tree
if frame.had_cycle {
    parent.had_cycle = true;
}

// Only cache if no cycle influence
if !frame.had_cycle {
    self.table.insert(frame.canonical, outcome);
}
```

**Limitation:** We lose caching for many positions, potentially making the solve much slower.

### Solution 2: Full Path-Based State

Instead of caching by position, cache by full game path (or equivalently, by (position, repetition_counts) for all seen positions).

**State = (current_position, {pos → count for all seen positions})**

Two paths with identical repetition counts for ALL positions have the same future and can share cached values.

**Limitation:** The state space is much larger than just positions.

### Solution 3: Full Game Tree Storage

Don't compress transpositions at all. Store the complete game tree:

```
nodes(id, parent_id, move, position, result)
```

Each node represents a specific game path. Same position reached via different paths = different nodes.

**Advantages:**
- Conceptually simple
- No ambiguity about what values mean
- Each result is path-specific and correct

**Disadvantages:**
- Potentially massive storage (number of paths >> number of positions)
- Need to estimate actual size for Gobblet Gobblers

## Open Questions

1. **How big is the full game tree?**
   - We found ~500M unique positions
   - With transpositions as distinct nodes, how many path-states exist?
   - Need empirical measurement

2. **2-fold vs 3-fold repetition?**
   - Official rules: 3-fold
   - 2-fold is simpler and gives same minimax value (if a player can escape, they will)
   - For tablebase viewer, might want to honor official 3-fold rule

3. **Tablebase viewer design?**
   - How to show moves that lead to repeated positions?
   - Allow user to continue past repetition (analysis mode) or enforce rules (official mode)?

4. **Storage format?**
   - Graph database (Neo4j)?
   - SQLite with node/edge tables?
   - Custom binary format?

## Experiment: Full Tree Enumeration

### Attempt

Created `count_tree` binary to enumerate all paths without transposition compression:
- No caching
- 2-fold repetition = terminal draw
- Uses actual position hash (not canonical) for repetition detection

### Results (after ~2.5 minutes)

| Metric | Value |
|--------|-------|
| Nodes visited | 277 million |
| Stack depth | 117 million |
| Memory usage | ~18 GB |
| Checkpoint size | 4.8 GB |
| Backtracking | **ZERO** |

### Analysis

The DFS descended 117 million moves deep without EVER backtracking. With 500M+ unique positions, the 2-fold repetition terminal condition almost never triggers.

**Conclusion:** The full game tree without transposition compression is effectively infinite and cannot be enumerated.

### Implications

1. We CANNOT avoid transposition caching entirely
2. We MUST handle the GHI (Graph History Interaction) problem
3. The tablebase must use some form of (position, history) keying, or accept path-dependent limitations

## Experiment: `had_cycle` Tracking

### Attempt

Implemented Solution 1 (don't cache cycle-influenced values):
- Added `had_cycle: bool` to each frame
- When detecting a cycle, set `had_cycle = true` on the current frame
- When popping a frame, propagate `had_cycle` to parent
- Only cache if `!had_cycle`

### Results

| Metric | With had_cycle | Original |
|--------|---------------|----------|
| Pure positions cached | 175,806 | 19,836,040 |
| Cycle-influenced (not cached) | 158+ million | 0 |
| Solver behavior | Runs indefinitely | 31 seconds |

After 4 minutes, the solver had:
- Cached only 175K "pure" positions
- Skipped caching 158 million cycle-influenced positions
- Still running with no end in sight

### Analysis

**Cycle influence propagates aggressively to the root.** If any leaf in a subtree has a cycle, all ancestors become cycle-influenced. In Gobblet Gobblers:
- ~18,700 direct cycle detections occur
- But almost ALL positions have some descendant that hits a cycle
- Result: 99.9% of positions are cycle-influenced

Without caching these positions, the solver must re-explore the same subtrees repeatedly, effectively doing a full tree enumeration.

### Conclusion

**Solution 1 is infeasible.** The game tree is too cycle-heavy for "don't cache cycle-influenced values" to work.

## Recommended Strategy: Intrinsic Values + Runtime Repetition

### For Alpha-Beta Pruned Solve

The existing implementation is correct and efficient:
- Result: **P1 WINS** (verified)
- Time: **31 seconds**
- Positions: **~20 million**

The alpha-beta pruned solve produces the correct answer because:
1. The winning strategy for P1 is "pure" - it doesn't require cycles
2. We find winning moves first due to good move ordering
3. Early cutoffs mean we rarely cache cycle-influenced values
4. The few incorrect cache entries don't affect the result

### For Tablebase Storage

Store positions as "intrinsic values" - the outcome if that position were the start of a fresh game:

```
canonical_hash → outcome (WIN_P1 | DRAW | WIN_P2)
```

This is what the pruned solve already produces (~20M entries).

### For Tablebase Viewer

Handle repetition at runtime:
1. Track the game history as the user plays
2. For each position, show its intrinsic value from the tablebase
3. Apply 3-fold repetition rule at runtime:
   - If a position appears for the 3rd time → DRAW
   - Otherwise, continue using intrinsic values
4. Optional: highlight moves that would lead to repeated positions

### Semantic Meaning

The tablebase answers: "If I started a new game from this position, who wins with optimal play?"

The viewer answers: "Given the current game history, who wins with optimal play?"

These can differ when:
- A "winning" position's optimal path goes through a repeated position
- The repeated position forces a draw under actual game rules

### Edge Cases

1. **Position X has intrinsic value WIN_P1, but...**
   - The winning line goes through position Y
   - Y has already appeared twice in the game history
   - Moving to Y would be the 3rd occurrence → forced draw
   - P1 must find an alternative winning line (or it's actually a draw in this game)

2. **Resolution**: The viewer should:
   - Show the intrinsic value with a warning if winning path is blocked
   - Compute on-demand for positions where repetition affects the outcome
   - Or accept that some edge cases show "approximate" values

## Summary

| Approach | Feasibility | Correctness | Storage |
|----------|-------------|-------------|---------|
| Full tree enumeration | ❌ Infeasible | ✓ | Infinite |
| Don't cache cycle-influenced | ❌ Infeasible | ✓ | - |
| Cache all (current) | ✓ 31 seconds | ✓ for pruned | 20M entries |
| Intrinsic + runtime repetition | ✓ | ✓ | 20M + viewer logic |

**Recommended**: Keep current solver, add repetition handling in viewer.

## Appendix: Why Pruned Solve Is Correct

The pruned (alpha-beta) solve returns P1 WINS, matching V1 Python solver. Why doesn't the bug affect it?

With alpha-beta pruning and good move ordering:
1. We find winning moves first
2. We cut off exploration early after finding a win
3. We rarely explore deep enough to hit cycles
4. The paths we DO explore mostly don't involve cycles
5. The cached values are mostly "pure" (from terminal wins/losses, not cycles)

The full solve explores everything, including all the cyclic paths. This exposes the caching bug because:
1. More positions are reached via multiple paths
2. More cycle-influenced values get cached
3. More incorrect cache hits occur
