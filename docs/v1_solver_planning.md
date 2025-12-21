# Solver Planning & Design

This document captures the design decisions for the Gobblet Gobblers solver (M4/M5).

## Goals

1. Fully solve the game (compute optimal play from any position)
2. Store the solution compactly for instant runtime lookup
3. Display move evaluations in the UI

---

## 1. State Representation

### Board Structure

Each cell can have at most **one piece of each size** due to gobbling rules (larger covers smaller). A cell is effectively 3 slots:

| Slot | Contents |
|------|----------|
| Small | Empty, P1, or P2 |
| Medium | Empty, P1, or P2 |
| Large | Empty, P1, or P2 |

**27 valid states per cell** (3³ = 27)

Examples:
- `(0, 0, 0)` = empty cell
- `(1, 0, 0)` = P1 small only
- `(2, 1, 0)` = P2 small under P1 medium
- `(1, 2, 1)` = P1 small, P2 medium, P1 large (full stack)

### Compact Encoding (Binary)

For the transposition table key:

```
Board:    9 cells × 3 slots × 2 bits = 54 bits
Current player: 1 bit
─────────────────────────────────────────────────
Total: 55 bits → fits in a single 64-bit integer
```

**Note:** Reserves are NOT stored - they're derivable from the board (each player starts with 2 of each size; count what's on board, subtract from 2).

Alternative: encode each cell as 0-26 (5 bits), 9 cells = 45 bits + 1 bit player = 46 bits.

### Base64 Encoding (UI Export/Import)

For UI export/import, the 64-bit binary encoding is converted to base64:
- ~11 characters for any position (vs ~20-30 for human-readable)
- Example: `AAAAAAAAAAA` (empty board, P1 to move)
- Consistent with internal representation

This provides ~2x storage savings vs human-readable notation while remaining
copy/paste friendly.

### Symmetry Reduction (D₄)

The board has dihedral symmetry of order 8:
- 4 rotations: 0°, 90°, 180°, 270°
- 4 reflections: horizontal, vertical, two diagonals

**Canonicalization algorithm:**
1. Compute all 8 symmetric transforms of position
2. Encode each as a comparable value (e.g., tuple or int)
3. Return lexicographically smallest as canonical form
4. Use canonical form as hash key

This reduces state space by **up to 8×**.

### State Space Estimate

| Metric | Estimate |
|--------|----------|
| Theoretical upper bound | 27⁹ ≈ 7 × 10¹² |
| With piece constraints | ~10⁸ - 10⁹ |
| With symmetry reduction | **~10⁶ - 10⁸** |

The game is known to be solvable; state space is manageable.

---

## 2. Transposition Table

### Structure

The transposition table is a **dictionary** (not a graph):

```python
TranspositionTable = dict[CanonicalState, SolverResult]

@dataclass
class SolverResult:
    outcome: Outcome          # WIN_P1, WIN_P2, DRAW
    depth_to_outcome: int     # Moves until forced result (0 if terminal)
    best_move: Move | None    # Optimal move (None if terminal)
```

### Outcome Enum

```python
class Outcome(Enum):
    WIN_P1 = 1      # Player 1 wins with optimal play
    WIN_P2 = -1     # Player 2 wins with optimal play
    DRAW = 0        # Draw with optimal play
```

### Usage Pattern

```python
def solve(state: GameState) -> SolverResult:
    canonical = canonicalize(state)

    # Check cache first
    if canonical in transposition_table:
        return transposition_table[canonical]

    # Compute via minimax...
    result = minimax_search(state)

    # Cache and return
    transposition_table[canonical] = result
    return result
```

### Why Not a Graph?

- Edges (legal moves) can be regenerated cheaply from any position
- We only need position **values**, not explicit graph topology
- Dictionary provides O(1) lookup, which is all we need

---

## 3. Cycle Detection (Three-fold Repetition)

### The Problem

Standard minimax assumes a DAG, but Gobblet Gobblers allows position repetition:

```
Position A → moves → Position B → moves → Position A (cycle)
```

Three-fold repetition = draw. How do we handle this in search?

### Solution: Path-Based Detection

Track positions on the current search path. If we'd revisit a position, treat as potential draw:

```python
def solve(state: GameState, path: frozenset[CanonicalState]) -> SolverResult:
    canonical = canonicalize(state)

    # Cycle detection: position already on current path
    if canonical in path:
        # Revisiting = draw (simplifying 3-fold to 2-fold for search)
        return SolverResult(Outcome.DRAW, 0, None)

    # Check transposition table
    if canonical in transposition_table:
        return transposition_table[canonical]

    # Terminal check
    if is_terminal(state):
        return evaluate_terminal(state)

    # Minimax recursion
    new_path = path | {canonical}
    best_result = None

    for move in generate_moves(state):
        child_state = apply_move(state, move)
        child_result = solve(child_state, new_path)
        # ... minimax logic to find best ...

    transposition_table[canonical] = best_result
    return best_result
```

### Transposition Table vs Path History

There's a subtle distinction:

| Concern | Handling |
|---------|----------|
| **Transposition table** | Stores "inherent" value assuming optimal play from position forward |
| **Path history** | Detects cycles during search; path-dependent |

The table stores: "If you reach this position fresh (no prior visits), what's the outcome with optimal play?"

During search, if a player can force a cycle (draw), minimax naturally accounts for it:
- Losing player will prefer draw via repetition
- Winning player will avoid repetition

### Alternative: Retrograde Analysis

Work backwards from terminal positions. More elegant for cycles but more complex to implement. **Recommendation: start with path-based minimax.**

---

## 4. Compute Strategy: Solve Once, Save, Load

### Storage Estimate

Assuming 10⁷ unique positions:

| Component | Bytes/Entry | Total |
|-----------|-------------|-------|
| State key (canonical) | 8 | 80 MB |
| Outcome | 1 | 10 MB |
| Depth to outcome | 2 | 20 MB |
| Best move | 4 | 40 MB |
| **Total** | **~15** | **~150 MB** |

With compression: **50-100 MB**

### Approach

**Phase 1: Solve (one-time)**
- Run full minimax with transposition table
- May take minutes to hours depending on implementation
- Save results to disk

**Phase 2: Runtime (instant)**
- Load precomputed table into memory
- O(1) lookup for any position

### Storage Format Options

| Format | Size (10⁷ entries) | Load Time | Checkpointing | Debugging |
|--------|-------------------|-----------|---------------|-----------|
| JSON | 500MB-1GB | 10-30s | Risky (full rewrite) | Easy |
| CSV | 200-400MB | 5-15s | Append-friendly | Easy |
| SQLite | 200-400MB | 5-10s | Atomic writes | Queryable |
| Binary | 100-200MB | 1-5s | Manual | Hard |

**Decision:**
- **During solving:** SQLite for checkpointing (atomic writes prevent corruption on interrupt)
- **For inspection:** Export to CSV for browsing/spot-checking
- **At runtime:** Load into Python dict for O(1) lookups

### Fallback (if state space larger than expected)

If state space exceeds 10⁹:
- On-demand solving with LRU cache
- Store only "important" positions (openings, near-terminal)
- But unlikely needed for 3×3 board

---

## 5. UI Design

### North Star

For each legal move, display:
- Move notation
- Outcome (win/draw/loss)
- Depth to outcome
- Highlight optimal move(s)

### Mock Display

```
┌─────────────────────────────────────────────┐
│ Current Position: P1 to move                │
│ Evaluation: P1 wins with optimal play       │
├─────────────────────────────────────────────┤
│ Legal Moves:                                │
│                                             │
│  ● L(1,1)     Win in 3    ← optimal         │
│  ● M(0,0)     Win in 5                      │
│  ● L(2,2)     Win in 7                      │
│  ○ S(0,1)     Draw                          │
│  ✗ M(1,0)     Loss in 4                     │
│  ✗ S(2,1)     Loss in 2                     │
└─────────────────────────────────────────────┘
```

### Color Coding

| Color | Meaning |
|-------|---------|
| Green | Winning move (maintains win) |
| Yellow | Drawing move |
| Red | Losing move (blunder) |

### Sorting Priority

1. Winning moves (shortest win first)
2. Drawing moves
3. Losing moves (longest resistance first)

### Additional Features

- **Board overlay**: Color squares by outcome if moved there
- **Optimal line**: Show full sequence of optimal play
- **What-if analysis**: Click any move to see new evaluation
- **Auto-play**: Button to play optimal move

---

## 6. Implementation Plan

Phased approach with human verification checkpoints at each stage.

### Phase 0: UI Foundation ✅ COMPLETE
**Goal:** Make game state visible for manual verification before building solver.

**0.1 Legal Moves Panel** ✅
- Display all legal moves for current player in UI
- Shows move notation + type (reserve vs board move)
- Clicking a move executes it
- Moves grouped by type (reserve placements, board moves)

**0.2 State Export/Import** ✅
- Endpoints: `GET /state/export`, `POST /state/import`
- Uses base64-encoded binary format
- Useful for: creating test positions, debugging, sharing states

### Phase 1: State Encoding ✅ COMPLETE
**Goal:** Build and verify compact representation before using it in solver.

**1.1 Binary Encoding** ✅
- Implemented 64-bit state encoding in `solver/encoding.py`
- Functions: `encode_state(GameState) -> int`, `decode_state(int) -> GameState`
- 21 unit tests: encode → decode roundtrip for various positions

**1.2 Canonicalization** ✅
- Implemented D₄ transforms (8 symmetries)
- `canonicalize(encoded) -> canonical_int`
- Unit tests: symmetric positions produce same canonical form

**1.3 Base64 API Integration** ✅
- `GET /state/export` returns base64-encoded state
- `POST /state/import` accepts base64-encoded state
- Frontend UI uses base64 for state export/import

### Phase 2: Solver Core
**Goal:** Minimax that can pause/resume and report progress.

**2.1 Basic Minimax**
- Implement in `solver/minimax.py`
- Transposition table (Python dict)
- Path-based cycle detection
- Returns `SolverResult(outcome, depth, best_move)`

**2.2 Checkpointing**
- Save transposition table to SQLite periodically
- Triggers: every N positions (e.g., 100k), every M minutes (e.g., 5), on SIGINT
- Resume: load existing checkpoint, skip already-computed positions
- Schema: `CREATE TABLE solutions (canonical INT PRIMARY KEY, outcome INT, depth INT, best_move TEXT)`

**2.3 Progress Reporting**
- Log: positions evaluated, cache hits, estimated completion %
- *Verification: Run on small subtrees, manually verify results*

### Phase 3: Solution Storage & Inspection
**Goal:** Save results in a format that can be browsed and validated.

**3.1 Final Solution Storage**
- Save to `solver/solution.db` (SQLite)
- Export to CSV for inspection: `solver/solution.csv`

**3.2 CLI Inspection Tools**
- `python -m solver.inspect --state "./sM/./..."` → show evaluation
- `python -m solver.inspect --random N` → show N random positions with evals
- `python -m solver.inspect --stats` → total positions, win/draw/loss breakdown
- *Verification: Spot-check positions, play through optimal lines manually*

### Phase 4: UI Integration
**Goal:** Full solver UI as originally envisioned.

**4.1 Backend Integration**
- Load solution database at API startup
- New endpoint: `GET /evaluate` → evaluation for current position
- New endpoint: `GET /moves/evaluated` → all moves with evaluations

**4.2 Frontend Display**
- Evaluation panel showing all moves with outcomes
- Color coding (green/yellow/red)
- Sorting (best moves first)
- *Verification: Play games, confirm evaluations match analysis*

**4.3 Polish (optional)**
- Optimal line display
- "Best move" auto-play button
- Board overlay showing destination outcomes

---

## 7. Checkpointing Design

Robust checkpointing is critical since solver runtime is uncertain.

### Checkpoint Data

```python
@dataclass
class SolverCheckpoint:
    positions_evaluated: int
    cache_hits: int
    transposition_table: dict[int, SolverResult]
```

### SQLite Schema

```sql
CREATE TABLE metadata (
    key TEXT PRIMARY KEY,
    value TEXT
);

CREATE TABLE solutions (
    canonical INTEGER PRIMARY KEY,  -- 64-bit encoded state
    outcome INTEGER,                -- -1=P2 wins, 0=draw, 1=P1 wins
    depth INTEGER,                  -- moves to outcome
    best_move TEXT                  -- notation, e.g., "L(1,1)" or "(0,0)→(1,1)"
);

-- Metadata entries:
-- positions_evaluated, cache_hits, last_checkpoint_time, solver_version
```

### Checkpoint Triggers

1. **Periodic:** Every 100,000 positions evaluated
2. **Time-based:** Every 5 minutes
3. **On interrupt:** SIGINT (Ctrl+C) triggers graceful save before exit

### Resume Logic

```python
def solve_with_checkpointing():
    # 1. Check for existing checkpoint
    if checkpoint_exists():
        load_checkpoint()  # Populates transposition_table
        log(f"Resumed from checkpoint: {positions_evaluated} positions")

    # 2. Run solver (skips positions already in table)
    solve_from_initial_state()

    # 3. Save final solution
    save_final_solution()
```

### Graceful Shutdown

```python
import signal

def handle_sigint(signum, frame):
    log("Interrupt received, saving checkpoint...")
    save_checkpoint()
    sys.exit(0)

signal.signal(signal.SIGINT, handle_sigint)
```

---

## 8. Open Questions

1. **Alpha-beta vs pure minimax**: Alpha-beta is faster but requires move ordering. Worth the complexity? *Decision: Start with pure minimax for simplicity. Optimize later if needed.*

2. **Iterative deepening**: Useful for progress feedback during solve phase? *Decision: Not needed initially - progress reporting via position count is sufficient.*

3. **Parallel solving**: Can we parallelize the search? (Tricky with shared transposition table) *Decision: Defer. Single-threaded should be fast enough for this state space.*

4. **Move ordering heuristics**: Captures first? Center first? Could speed up alpha-beta significantly. *Decision: Defer until/unless we need alpha-beta.*

---

## 9. References

- Gobblet Gobblers is a known solved game
- D₄ symmetry group: https://en.wikipedia.org/wiki/Dihedral_group
- Retrograde analysis: https://en.wikipedia.org/wiki/Retrograde_analysis
- Transposition tables: https://en.wikipedia.org/wiki/Transposition_table
