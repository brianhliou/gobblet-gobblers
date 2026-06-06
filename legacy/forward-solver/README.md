# Forward minimax solver (retired)

This is the **original** Gobblet Gobblers solver: a forward depth-first minimax
with alpha-beta pruning and a transposition table keyed on the canonical
position. It is preserved here for reference. **It is wrong, and it is not built.**

## Why it's wrong

Gobblet Gobblers has cycles (pieces move, positions repeat), and the threefold
repetition rule makes a position's value depend on the *history* that reached it,
not the board alone. A transposition table keyed on the board therefore caches a
value that is only valid for one history and reuses it for another — the
**Graph History Interaction (GHI)** bug. The forward solver's tablebase was
internally inconsistent as a result.

The correct solve is the two-phase **retrograde** analysis in
[`../../solver/src/bin/retro.rs`](../../solver/src/bin/retro.rs): enumerate every
non-terminal canonical position, then fill distance-to-mate by backward induction
to a fixpoint. Draws fall out as the cycle-bound residue the fixpoint never
proves won or lost. See [`docs/retrograde_correction.md`](../../docs/retrograde_correction.md)
and [`docs/game_tree_analysis.md`](../../docs/game_tree_analysis.md).

## Files

| File | Role |
|------|------|
| `main.rs` | Forward solver entry (was the `solver` binary) |
| `solver.rs` | Iterative minimax + alpha-beta + transposition table |
| `checkpoint.rs` | `GBL2` binary checkpoint format for solver state |
| `stats.rs` | Memory / progress reporting |
| `export_sqlite.rs` | Dumped the `GBL2` checkpoint to SQLite for inspection |

These depend on the `movegen` module as it existed in the `gobblet-solver` crate.
For a snapshot that still **compiles**, check out the tag:

```bash
git checkout forward-solver-final
```
