#!/usr/bin/env python3
"""
Script to run the Gobblet Gobblers solver with checkpointing.

Usage:
    python -m solver.run_solver              # Run solver, resume from checkpoint if exists
    python -m solver.run_solver --fresh      # Start fresh (ignore existing checkpoint)
    python -m solver.run_solver --status     # Show checkpoint status
    python -m solver.run_solver --clear      # Clear checkpoint and exit
"""

from __future__ import annotations

import argparse
import signal
import sys
import time
from pathlib import Path

from solver.checkpoint import (
    clear_checkpoint,
    get_checkpoint_stats,
    load_checkpoint,
    save_checkpoint,
)
from solver.minimax import Solver


def main() -> None:
    parser = argparse.ArgumentParser(description="Gobblet Gobblers solver")
    parser.add_argument("--fresh", action="store_true", help="Start fresh, ignore checkpoint")
    parser.add_argument("--status", action="store_true", help="Show checkpoint status")
    parser.add_argument("--clear", action="store_true", help="Clear checkpoint and exit")
    parser.add_argument("--checkpoint-interval", type=int, default=100000,
                        help="Save checkpoint every N positions (default: 100000)")
    args = parser.parse_args()

    if args.clear:
        clear_checkpoint()
        print("Checkpoint cleared.")
        return

    if args.status:
        stats = get_checkpoint_stats()
        if stats:
            print("Checkpoint found:")
            for key, value in stats.items():
                print(f"  {key}: {value}")
        else:
            print("No checkpoint found.")
        return

    # Create solver
    solver = Solver()
    solver._report_interval = args.checkpoint_interval

    # Load checkpoint if exists and not fresh
    if not args.fresh:
        loaded = load_checkpoint(solver)
        if loaded > 0:
            print(f"Loaded checkpoint: {loaded:,} positions")
            print(f"  Positions evaluated: {solver.stats.positions_evaluated:,}")
            print(f"  Cache hits: {solver.stats.cache_hits:,}")

    # Setup signal handler for graceful shutdown
    shutdown_requested = False

    def handle_signal(signum, frame):
        nonlocal shutdown_requested
        if shutdown_requested:
            print("\nForce quit (no checkpoint save)")
            sys.exit(1)
        print("\nShutdown requested, saving checkpoint...")
        shutdown_requested = True

    signal.signal(signal.SIGINT, handle_signal)
    signal.signal(signal.SIGTERM, handle_signal)

    # Run solver with periodic checkpoints
    print(f"Starting solver (checkpoint every {args.checkpoint_interval:,} positions)...")
    start_time = time.time()
    last_checkpoint = solver.stats.positions_evaluated

    # Monkey-patch progress reporting to include checkpointing
    original_report = solver._report_progress

    def report_with_checkpoint():
        original_report()
        nonlocal last_checkpoint
        if solver.stats.positions_evaluated - last_checkpoint >= args.checkpoint_interval:
            saved = save_checkpoint(solver)
            print(f"  Checkpoint saved: {saved:,} positions", flush=True)
            last_checkpoint = solver.stats.positions_evaluated

        if shutdown_requested:
            saved = save_checkpoint(solver)
            print(f"Final checkpoint saved: {saved:,} positions")
            elapsed = time.time() - start_time
            print(f"Elapsed: {elapsed:.1f}s ({elapsed/60:.1f} min)")
            sys.exit(0)

    solver._report_progress = report_with_checkpoint

    try:
        outcome = solver.solve()
        elapsed = time.time() - start_time

        print(f"\n{'='*50}")
        print(f"SOLVED!")
        print(f"{'='*50}")
        print(f"Result: {outcome.name}")
        print(f"Time: {elapsed:.1f}s ({elapsed/60:.1f} min)")
        print(f"Positions evaluated: {solver.stats.positions_evaluated:,}")
        print(f"Unique positions: {len(solver.table):,}")
        print(f"Cache hits: {solver.stats.cache_hits:,}")
        print(f"Terminal positions: {solver.stats.terminal_positions:,}")
        print(f"Max depth: {solver.stats.max_depth}")
        print(f"Cycle draws: {solver.stats.cycle_draws:,}")

        # Final save
        saved = save_checkpoint(solver)
        print(f"\nFinal checkpoint saved: {saved:,} positions")

    except Exception as e:
        print(f"\nError: {e}")
        saved = save_checkpoint(solver)
        print(f"Emergency checkpoint saved: {saved:,} positions")
        raise


if __name__ == "__main__":
    main()
