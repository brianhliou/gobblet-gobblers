#!/usr/bin/env python3
"""
Robust solver with time-based checkpointing, logging, and crash diagnostics.

Features:
- Time-based incremental checkpointing (default: every 60 seconds)
- Memory usage logging
- Progress logging with timestamps
- Signal handling for graceful shutdown
- Crash diagnostics
"""

import signal
import sys
import time
import traceback
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path

from gobblet.state import GameState
from solver.checkpoint import IncrementalCheckpointer, save_checkpoint
from solver.minimax import Solver


@dataclass
class SolveConfig:
    """Configuration for robust solve."""
    checkpoint_interval_sec: float = 60.0  # Checkpoint every 60 seconds
    log_interval_sec: float = 30.0  # Log progress every 30 seconds
    prune: bool = True  # Alpha-beta pruning
    force: bool = False  # Force re-exploration of solved positions
    start_state: GameState | None = None  # Custom starting position


def get_memory_mb() -> float:
    """Get current process memory usage in MB."""
    try:
        import resource
        rusage = resource.getrusage(resource.RUSAGE_SELF)
        # maxrss is in bytes on Linux, kilobytes on macOS
        if sys.platform == "darwin":
            return rusage.ru_maxrss / 1024 / 1024
        else:
            return rusage.ru_maxrss / 1024
    except Exception:
        return 0.0


def log(msg: str) -> None:
    """Print timestamped log message."""
    timestamp = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    print(f"[{timestamp}] {msg}", flush=True)


def robust_solve(config: SolveConfig | None = None) -> None:
    """
    Run solver with robust checkpointing and logging.

    Handles:
    - Graceful shutdown on SIGINT/SIGTERM
    - Time-based incremental checkpointing
    - Progress and memory logging
    - Exception handling with checkpoint save
    """
    if config is None:
        config = SolveConfig()

    log("=" * 60)
    log("ROBUST SOLVER")
    log("=" * 60)
    log(f"Checkpoint interval: {config.checkpoint_interval_sec}s")
    log(f"Log interval: {config.log_interval_sec}s")
    log(f"Pruning: {config.prune}")
    log(f"Force re-explore: {config.force}")

    # Initialize solver and checkpointer
    solver = Solver()
    checkpointer = IncrementalCheckpointer(
        checkpoint_interval_sec=config.checkpoint_interval_sec
    )

    loaded = checkpointer.initialize(solver)
    log(f"Loaded {loaded:,} positions from checkpoint")
    log(f"Initial memory: {get_memory_mb():.1f} MB")

    # Track state for graceful shutdown
    shutdown_requested = False
    start_time = time.time()
    last_log_time = start_time
    start_positions = loaded

    def handle_signal(signum, frame):
        nonlocal shutdown_requested
        sig_name = signal.Signals(signum).name
        if shutdown_requested:
            log(f"Force quit (second {sig_name})")
            sys.exit(1)
        shutdown_requested = True
        log(f"Shutdown requested ({sig_name}), will save checkpoint...")

    signal.signal(signal.SIGINT, handle_signal)
    signal.signal(signal.SIGTERM, handle_signal)

    # Custom progress reporter that integrates with checkpointing and logging
    def report_progress():
        nonlocal last_log_time

        now = time.time()
        elapsed = now - start_time

        # Time-based logging
        if now - last_log_time >= config.log_interval_sec:
            last_log_time = now
            new_positions = len(solver.table) - start_positions
            pos_per_sec = solver.stats.positions_evaluated / elapsed if elapsed > 0 else 0
            memory_mb = get_memory_mb()

            log(
                f"Progress: {solver.stats.positions_evaluated:,} evaluated, "
                f"{len(solver.table):,} unique (+{new_positions:,} new), "
                f"depth {solver.stats.max_depth}, "
                f"{pos_per_sec:.0f} pos/s, "
                f"{memory_mb:.0f} MB"
            )

        # Time-based checkpointing
        saved = checkpointer.maybe_checkpoint(solver)
        if saved > 0:
            log(f"Checkpoint: saved {saved:,} new positions ({len(solver.table):,} total)")

        # Check for shutdown
        if shutdown_requested:
            raise KeyboardInterrupt("Shutdown requested")

    # Configure solver - use small interval so time-based checks happen frequently
    solver._report_progress = report_progress
    solver._report_interval = 1000  # Check every 1000 positions for time-based triggers

    # Run solve
    try:
        log("")
        log("Starting solve...")
        log("")

        state = config.start_state or GameState()
        outcome = solver.solve(state, fast=True, prune=config.prune, force=config.force)

        elapsed = time.time() - start_time
        new_positions = len(solver.table) - start_positions

        log("")
        log("=" * 60)
        log("SOLVE COMPLETE!")
        log("=" * 60)
        log(f"Outcome: {outcome.name}")
        log(f"Time: {elapsed/60:.1f} minutes")
        log(f"Positions evaluated: {solver.stats.positions_evaluated:,}")
        log(f"Unique positions: {len(solver.table):,}")
        log(f"New positions: {new_positions:,}")
        log(f"Cache hits: {solver.stats.cache_hits:,}")
        log(f"Terminal positions: {solver.stats.terminal_positions:,}")
        log(f"Cycle draws: {solver.stats.cycle_draws:,}")
        log(f"Max depth: {solver.stats.max_depth}")
        log(f"Final memory: {get_memory_mb():.1f} MB")

    except KeyboardInterrupt:
        elapsed = time.time() - start_time
        log("")
        log(f"Interrupted after {elapsed/60:.1f} minutes")

    except Exception as e:
        log("")
        log(f"ERROR: {e}")
        log(traceback.format_exc())

    finally:
        # Always save checkpoint on exit
        log("")
        log("Saving final checkpoint...")
        saved = checkpointer.force_checkpoint(solver)
        log(f"Saved {saved:,} new positions")
        log(f"Total in database: {len(solver.table):,}")


def main():
    """Entry point with command line argument support."""
    import argparse

    parser = argparse.ArgumentParser(description="Robust Gobblet Gobblers solver")
    parser.add_argument(
        "--checkpoint-interval", type=float, default=60.0,
        help="Checkpoint interval in seconds (default: 60)"
    )
    parser.add_argument(
        "--log-interval", type=float, default=30.0,
        help="Log interval in seconds (default: 30)"
    )
    parser.add_argument(
        "--no-prune", action="store_true",
        help="Disable alpha-beta pruning (explore all positions)"
    )
    parser.add_argument(
        "--force", action="store_true",
        help="Force re-exploration of solved positions"
    )

    args = parser.parse_args()

    config = SolveConfig(
        checkpoint_interval_sec=args.checkpoint_interval,
        log_interval_sec=args.log_interval,
        prune=not args.no_prune,
        force=args.force,
    )

    robust_solve(config)


if __name__ == "__main__":
    main()
