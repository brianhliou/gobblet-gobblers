#!/usr/bin/env python3
"""
Complete the full game exploration WITHOUT alpha-beta pruning.

This explores all positions that were skipped by pruning in the original solve.
Loads the existing checkpoint and only computes new positions.
"""

import sys
import time
import signal
from solver.minimax import Solver
from solver.checkpoint import save_checkpoint, load_checkpoint, get_checkpoint_stats
from gobblet.state import GameState


def main():
    print("=" * 60)
    print("UNPRUNED EXPLORATION")
    print("=" * 60)
    print()
    print("This will explore ALL positions, including those that were")
    print("skipped by alpha-beta pruning in the original solve.")
    print()

    solver = Solver()

    # Load existing checkpoint
    loaded = load_checkpoint(solver)
    print(f"Loaded {loaded:,} positions from checkpoint")
    print()

    # Set up graceful shutdown
    shutdown_requested = False
    def handle_signal(signum, frame):
        nonlocal shutdown_requested
        if shutdown_requested:
            print("\nForce quit")
            sys.exit(1)
        shutdown_requested = True
        print("\nShutdown requested, saving checkpoint...")

    signal.signal(signal.SIGINT, handle_signal)
    signal.signal(signal.SIGTERM, handle_signal)

    # Progress reporting with checkpointing
    last_checkpoint = 0
    checkpoint_interval = 50_000  # More frequent since we expect fewer new positions
    start_positions = loaded

    original_report = solver._report_progress
    def report_with_checkpoint():
        nonlocal last_checkpoint
        original_report()

        new_found = len(solver.table) - start_positions
        print(f"  [New positions found: {new_found:,}]")

        if solver.stats.positions_evaluated - last_checkpoint >= checkpoint_interval:
            save_checkpoint(solver)
            last_checkpoint = solver.stats.positions_evaluated
            print(f"  [Checkpoint saved: {len(solver.table):,} total positions]")

    solver._report_progress = report_with_checkpoint
    solver._report_interval = 25_000

    # Solve WITHOUT pruning, WITH force re-exploration
    start_time = time.perf_counter()
    try:
        print("Starting unpruned exploration...")
        print("(Most positions will be cache hits from the original solve)")
        print()

        outcome = solver.solve(fast=True, prune=False, force=True)
        elapsed = time.perf_counter() - start_time

        new_positions = len(solver.table) - start_positions

        print()
        print("=" * 60)
        print("UNPRUNED EXPLORATION COMPLETE!")
        print("=" * 60)
        print(f"Outcome: {outcome.name}")
        print(f"Time: {elapsed/60:.1f} minutes")
        print(f"Original positions: {start_positions:,}")
        print(f"Final positions: {len(solver.table):,}")
        print(f"NEW positions found: {new_positions:,}")
        print(f"Positions evaluated: {solver.stats.positions_evaluated:,}")
        print(f"Cache hits: {solver.stats.cache_hits:,}")
        print(f"Terminal positions: {solver.stats.terminal_positions:,}")

        # Final checkpoint
        save_checkpoint(solver)
        print("\nFinal checkpoint saved.")

    except Exception as e:
        print(f"\nError: {e}")
        print("Saving checkpoint before exit...")
        save_checkpoint(solver)
        raise

    if shutdown_requested:
        save_checkpoint(solver)
        print("Checkpoint saved. Run again to continue.")


if __name__ == "__main__":
    main()
