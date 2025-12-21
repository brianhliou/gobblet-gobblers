#!/usr/bin/env python3
"""
Solve collected subtree positions one by one.

Takes a list of canonical position IDs, decodes each to a GameState,
and runs the solver from that position. Results are saved incrementally.

Key features:
- Mid-solve checkpointing: saves progress during long solves, not just between
- Progress logging during solves
- Graceful shutdown handling
"""

import json
import signal
import sys
import time
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path

from gobblet.state import GameState
from solver.checkpoint import IncrementalCheckpointer
from solver.encoding import decode_state
from solver.minimax import Solver
from solver.robust_solve import get_memory_mb


@dataclass
class SubtreeSolveConfig:
    positions_file: Path = Path("solver/depth_14_positions.json")
    checkpoint_interval_sec: float = 60.0
    log_interval_sec: float = 30.0
    max_positions: int | None = None  # Limit number of positions to solve


def log(msg: str) -> None:
    """Print timestamped log message."""
    timestamp = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    print(f"[{timestamp}] {msg}", flush=True)


def load_positions_flexible(filepath: Path) -> list[int]:
    """
    Load positions from JSON file, supporting multiple formats.

    Supported formats:
    - Simple: {"positions": [123, 456, ...]} (list of ints)
    - With depth: {"positions": [{"canonical": 123, "depth": 14}, ...]}
    """
    with open(filepath) as f:
        data = json.load(f)

    positions = data["positions"]

    if not positions:
        return []

    # Check first element to determine format
    if isinstance(positions[0], dict):
        # Format with depth info
        return [p["canonical"] for p in positions]
    else:
        # Simple format (list of ints)
        return positions


def solve_subtrees(config: SubtreeSolveConfig) -> None:
    """
    Solve each collected position as a subtree.

    Loads positions from JSON file, then solves each one.
    Checkpoints are saved incrementally, including DURING long solves.
    """
    log("=" * 60)
    log("SUBTREE SOLVER")
    log("=" * 60)
    log(f"Positions file: {config.positions_file}")
    log(f"Checkpoint interval: {config.checkpoint_interval_sec}s")
    log(f"Log interval: {config.log_interval_sec}s")

    # Load positions to solve
    if not config.positions_file.exists():
        log(f"ERROR: Positions file not found: {config.positions_file}")
        return

    log("Loading positions file...")
    canonical_positions = load_positions_flexible(config.positions_file)
    log(f"Loaded {len(canonical_positions):,} positions to solve")

    if config.max_positions:
        canonical_positions = canonical_positions[:config.max_positions]
        log(f"Limited to first {config.max_positions} positions")

    # Initialize solver with existing checkpoint
    log("Loading solver checkpoint...")
    solver = Solver()
    checkpointer = IncrementalCheckpointer(
        checkpoint_interval_sec=config.checkpoint_interval_sec
    )
    loaded = checkpointer.initialize(solver)
    log(f"Loaded {loaded:,} solved positions from checkpoint")
    log(f"Initial memory: {get_memory_mb():.1f} MB")

    # Track which positions are already solved
    already_solved = 0
    to_solve = []
    for canonical in canonical_positions:
        if canonical in solver.table:
            already_solved += 1
        else:
            to_solve.append(canonical)

    log(f"Already solved: {already_solved:,}, remaining: {len(to_solve):,}")

    if not to_solve:
        log("All positions already solved!")
        return

    # Handle graceful shutdown
    shutdown_requested = [False]  # Use list for mutability in nested function

    def handle_signal(signum, frame):
        if shutdown_requested[0]:
            log("Force quit")
            sys.exit(1)
        shutdown_requested[0] = True
        log("Shutdown requested, will save checkpoint after current solve...")

    signal.signal(signal.SIGINT, handle_signal)
    signal.signal(signal.SIGTERM, handle_signal)

    # Progress tracking for mid-solve reporting
    solve_start_time = [time.time()]
    last_log_time = [time.time()]
    current_position_idx = [0]
    positions_at_solve_start = [len(solver.table)]

    def report_progress():
        """Called by solver every N positions - allows mid-solve checkpointing."""
        now = time.time()
        elapsed = now - solve_start_time[0]
        new_since_start = len(solver.table) - positions_at_solve_start[0]
        rate = solver.stats.positions_evaluated / elapsed if elapsed > 0 else 0
        mem_mb = get_memory_mb()

        # Always log (we're using small interval for debugging)
        log(
            f"  [progress] {solver.stats.positions_evaluated:,} eval, "
            f"+{new_since_start:,} new, "
            f"{rate:.0f}/s, "
            f"max_depth={solver.stats.max_depth}, "
            f"cache_hits={solver.stats.cache_hits:,}, "
            f"mem={mem_mb:.0f}MB"
        )

        # Warn if memory is getting high
        if mem_mb > 10000:  # >10GB
            log(f"  [WARNING] Memory usage high: {mem_mb:.0f} MB")

        # Time-based checkpointing
        saved = checkpointer.maybe_checkpoint(solver)
        if saved > 0:
            log(f"  [checkpoint] Saved {saved:,} positions mid-solve")

        # Check for shutdown (will be handled after current solve completes)
        # We don't abort mid-solve to avoid corrupted state

    # Configure solver to call our progress reporter
    # Use small interval for debugging - log every 100 positions
    solver._report_progress = report_progress
    solver._report_interval = 100  # Very frequent for debugging

    # Solve each position
    log("")
    log("Starting subtree solves...")
    overall_start_time = time.time()
    positions_solved = 0
    total_new_positions = 0

    for i, canonical in enumerate(to_solve):
        if shutdown_requested[0]:
            log("Shutdown requested, stopping...")
            break

        current_position_idx[0] = i

        # Skip if solved (might have been solved as part of another subtree)
        if canonical in solver.table:
            if i < 10 or i % 1000 == 0:  # Don't spam logs
                log(f"[{i+1}/{len(to_solve)}] Already solved (transposition)")
            continue

        # Decode canonical to state
        try:
            state = decode_state(canonical)
        except Exception as e:
            log(f"[{i+1}/{len(to_solve)}] ERROR decoding {canonical}: {e}")
            continue

        # Reset per-solve tracking
        solve_start_time[0] = time.time()
        last_log_time[0] = time.time()
        positions_at_solve_start[0] = len(solver.table)
        before_count = len(solver.table)

        log(f"[{i+1}/{len(to_solve)}] Solving position {canonical}...")
        log(f"  [start] mem={get_memory_mb():.0f}MB, table_size={len(solver.table):,}")

        try:
            outcome = solver.solve(state, fast=True, prune=False, force=False)
            after_count = len(solver.table)
            new_positions = after_count - before_count
            solve_time = time.time() - solve_start_time[0]

            positions_solved += 1
            total_new_positions += new_positions

            log(
                f"  -> {outcome.name}, +{new_positions:,} new positions, "
                f"{solve_time:.1f}s"
            )

        except Exception as e:
            log(f"  -> ERROR: {e}")
            import traceback
            traceback.print_exc()
            continue

        # Checkpoint after each solve if needed
        saved = checkpointer.maybe_checkpoint(solver)
        if saved > 0:
            log(f"  [checkpoint] Saved {saved:,} positions")

    # Final summary
    elapsed = time.time() - overall_start_time
    log("")
    log("=" * 60)
    log("SUBTREE SOLVING " + ("STOPPED" if shutdown_requested[0] else "COMPLETE"))
    log("=" * 60)
    log(f"Subtrees solved: {positions_solved:,}")
    log(f"New positions found: {total_new_positions:,}")
    log(f"Total in table: {len(solver.table):,}")
    log(f"Time: {elapsed:.1f} seconds ({elapsed/60:.1f} min)")
    log(f"Final memory: {get_memory_mb():.1f} MB")

    if positions_solved > 0:
        log(f"Avg new positions per subtree: {total_new_positions / positions_solved:,.0f}")
        log(f"Avg time per subtree: {elapsed / positions_solved:.2f}s")

    # Final checkpoint
    log("")
    log("Saving final checkpoint...")
    saved = checkpointer.force_checkpoint(solver)
    log(f"Saved {saved:,} positions")


def main():
    """Entry point with command line argument support."""
    import argparse

    parser = argparse.ArgumentParser(description="Solve collected subtree positions")
    parser.add_argument(
        "--positions-file", type=str, default="solver/depth_14_positions.json",
        help="File containing positions to solve (default: solver/depth_14_positions.json)"
    )
    parser.add_argument(
        "--checkpoint-interval", type=float, default=60.0,
        help="Checkpoint interval in seconds (default: 60)"
    )
    parser.add_argument(
        "--log-interval", type=float, default=30.0,
        help="Log interval in seconds during solves (default: 30)"
    )
    parser.add_argument(
        "--max", type=int, default=None,
        help="Maximum number of positions to solve"
    )

    args = parser.parse_args()

    config = SubtreeSolveConfig(
        positions_file=Path(args.positions_file),
        checkpoint_interval_sec=args.checkpoint_interval,
        log_interval_sec=args.log_interval,
        max_positions=args.max,
    )

    solve_subtrees(config)


if __name__ == "__main__":
    main()
