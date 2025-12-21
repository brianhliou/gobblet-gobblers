#!/usr/bin/env python3
"""
Overnight solver run with checkpointing.

Usage:
    python -m solver.overnight_solve [--fast]

Default uses slow (copy-based) solver for reliability.
Add --fast flag to use the undo-based solver.
"""

import sys
import time
import signal
from solver.minimax import Solver
from solver.checkpoint import save_checkpoint, load_checkpoint, get_checkpoint_stats
from gobblet.state import GameState


def main():
    use_fast = "--fast" in sys.argv
    solver_name = "FAST (undo-based)" if use_fast else "SLOW (copy-based)"

    print(f"Starting {solver_name} solver with checkpointing")
    print(f"Progress saved every 100k positions to solver/gobblet_solver.db")
    print(f"Press Ctrl+C to stop gracefully")
    print()

    solver = Solver()

    # Try to load existing checkpoint
    loaded = load_checkpoint(solver)
    if loaded > 0:
        print(f"Loaded {loaded:,} positions from checkpoint")
        stats = get_checkpoint_stats()
        print(f"Previous stats: {stats}")
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

    # Custom progress reporting with checkpointing
    last_checkpoint = 0
    checkpoint_interval = 100_000

    original_report = solver._report_progress
    def report_with_checkpoint():
        nonlocal last_checkpoint
        original_report()

        if solver.stats.positions_evaluated - last_checkpoint >= checkpoint_interval:
            save_checkpoint(solver)
            last_checkpoint = solver.stats.positions_evaluated
            print(f"  [Checkpoint saved: {len(solver.table):,} unique positions]")

    solver._report_progress = report_with_checkpoint
    solver._report_interval = 50_000  # Report every 50k

    # Solve
    start_time = time.perf_counter()
    try:
        outcome = solver.solve(fast=use_fast)
        elapsed = time.perf_counter() - start_time

        print()
        print("=" * 60)
        print("SOLVING COMPLETE!")
        print("=" * 60)
        print(f"Outcome: {outcome.name}")
        print(f"Time: {elapsed/3600:.2f} hours")
        print(f"Positions evaluated: {solver.stats.positions_evaluated:,}")
        print(f"Unique positions: {len(solver.table):,}")
        print(f"Cache hits: {solver.stats.cache_hits:,}")
        print(f"Terminal positions: {solver.stats.terminal_positions:,}")
        print(f"Cycle draws: {solver.stats.cycle_draws:,}")
        print(f"Max depth: {solver.stats.max_depth}")

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
        print("Checkpoint saved. Run again to continue from this point.")


if __name__ == "__main__":
    main()
