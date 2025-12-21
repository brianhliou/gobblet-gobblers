#!/usr/bin/env python3
"""
Collect game positions at a specific depth for subtree solving.

This script traverses the game tree to a target depth and records
the positions found there (without solving them). These positions
can then be used as starting points for subtree solves.
"""

import gc
import json
import random
import signal
import sys
import time
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path

from gobblet.game import GameResult
from gobblet.moves import generate_moves
from gobblet.state import GameState
from solver.checkpoint import load_checkpoint
from solver.encoding import canonicalize, encode_state
from solver.fast_move import apply_move_in_place, undo_move_in_place
from solver.minimax import Solver


@dataclass
class CollectConfig:
    target_depth: int = 400
    max_positions: int = 1000  # Stop after collecting this many
    method: str = "random"  # "random", "dfs", "unsolved", or "enumerate"
    random_walks: int = 10000  # For random method: number of random walks
    output_file: Path = Path("solver/depth_positions.json")
    use_cache: bool = True  # Skip positions already in transposition table
    timeout_sec: float | None = None  # For enumerate method: stop after this many seconds
    use_encoding_queue: bool = True  # For enumerate: use 64-bit encodings instead of GameState objects


def log(msg: str) -> None:
    """Print timestamped log message."""
    timestamp = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    print(f"[{timestamp}] {msg}", flush=True)


def collect_by_random_walks(config: CollectConfig, solver: Solver | None = None) -> set[int]:
    """
    Collect positions at target depth using random walks.

    Performs random walks from the initial position, recording
    the canonical position when reaching the target depth.

    Note: Random walks typically end within 15-20 moves due to wins,
    so this method is not effective for deep positions (depth > 50).
    """
    collected: set[int] = set()
    walks_done = 0
    walks_terminated_early = 0

    state = GameState()

    for walk_num in range(config.random_walks):
        if len(collected) >= config.max_positions:
            break

        # Reset to initial state
        # (We need a fresh state since we're mutating in place)
        state = GameState()
        depth = 0

        # Random walk to target depth
        while depth < config.target_depth:
            moves = generate_moves(state)
            if not moves:
                walks_terminated_early += 1
                break

            # Pick a random move
            move = random.choice(moves)
            result, undo = apply_move_in_place(state, move)

            if result != GameResult.ONGOING:
                # Game ended before reaching target depth
                walks_terminated_early += 1
                undo_move_in_place(state, undo)
                break

            depth += 1

            if depth == config.target_depth:
                canonical = canonicalize(encode_state(state))

                # Skip if already in cache (already solved)
                if config.use_cache and solver and canonical in solver.table:
                    pass  # Don't collect - already solved
                else:
                    collected.add(canonical)

            # Undo for next iteration (actually we reset state anyway, but good practice)
            # Actually we continue the walk, so don't undo
            pass

        walks_done += 1

        if walks_done % 1000 == 0:
            log(f"Walks: {walks_done:,}, collected: {len(collected):,}, early terminations: {walks_terminated_early:,}")

    return collected


def collect_unsolved_positions(config: CollectConfig, solver: Solver) -> set[int]:
    """
    Collect positions reachable from initial state that are NOT in the cache.

    Uses DFS, exploring only non-terminal moves. When a position is not
    in the cache, we record it and don't explore further (it's a root
    of an unsolved subtree).

    This finds the "frontier" of unsolved positions.
    """
    collected: set[int] = set()
    visited: set[int] = set()
    nodes_explored = 0

    state = GameState()
    initial_canonical = canonicalize(encode_state(state))
    visited.add(initial_canonical)

    # Stack for iterative DFS
    # Each frame: (remaining_moves, undo_info_to_get_here)
    # undo_info is None for the root frame
    stack: list[tuple[list, object]] = []

    # Start with initial position's moves
    initial_moves = list(generate_moves(state))
    if initial_moves:
        stack.append((initial_moves, None))

    while stack and len(collected) < config.max_positions:
        moves, _ = stack[-1]

        if not moves:
            # No more moves at this level, backtrack
            _, undo_info = stack.pop()
            if undo_info is not None:
                undo_move_in_place(state, undo_info)
            continue

        # Take next move from current frame
        move = moves.pop(0)
        result, undo = apply_move_in_place(state, move)
        nodes_explored += 1

        if nodes_explored % 10000 == 0:
            log(f"Explored: {nodes_explored:,}, collected: {len(collected):,}, depth: {len(stack)}")

        should_undo = True

        if result == GameResult.ONGOING:
            child_canonical = canonicalize(encode_state(state))

            if child_canonical not in visited:
                visited.add(child_canonical)

                if child_canonical not in solver.table:
                    # Found an unsolved position! Record it.
                    collected.add(child_canonical)
                    # Don't explore further - this is a subtree root to solve later
                else:
                    # Position is solved, continue exploring its children
                    child_moves = list(generate_moves(state))
                    if child_moves:
                        # Push new frame with undo info
                        stack.append((child_moves, undo))
                        should_undo = False  # Don't undo - we're going deeper

        if should_undo:
            undo_move_in_place(state, undo)

    # Clean up any remaining stack (undo all moves)
    while stack:
        _, undo_info = stack.pop()
        if undo_info is not None:
            undo_move_in_place(state, undo_info)

    log(f"Final: explored {nodes_explored:,} nodes, found {len(collected):,} unsolved positions")
    return collected


def enumerate_all_unsolved_with_depth(
    solver: Solver,
    log_interval: int = 10000,
    timeout_sec: float | None = None,
    stop_flag: list | None = None,
    checkpoint_file: Path | None = None,
    checkpoint_interval_sec: float = 300.0,  # Save every 5 minutes
    use_encoding_queue: bool = True,  # Use optimized encoding-based queue
) -> dict[int, int]:
    """
    Enumerate unsolved positions reachable from initial state, with their minimum depth.

    Uses BFS to explore the solved region. When we encounter an unsolved position,
    we record it with its depth but don't explore further (it's a subtree root).

    Args:
        solver: Solver with loaded transposition table
        log_interval: How often to log progress
        timeout_sec: Optional timeout in seconds. If set, stops after this time and
                     returns what has been collected so far.
        stop_flag: Optional list that acts as a flag. If stop_flag[0] becomes True,
                   enumeration stops early. Used for signal handling.
        checkpoint_file: Optional file to save intermediate results to.
        checkpoint_interval_sec: How often to save checkpoints (default: 5 minutes).
        use_encoding_queue: If True, store 64-bit encodings in queue instead of
                           GameState objects. Much more memory efficient.

    Returns dict mapping canonical position -> minimum depth at which it was found.

    BFS guarantees we find the minimum depth for each position.
    """
    if use_encoding_queue:
        return _enumerate_with_encoding_queue(
            solver, log_interval, timeout_sec, stop_flag,
            checkpoint_file, checkpoint_interval_sec
        )
    else:
        return _enumerate_with_state_queue(
            solver, log_interval, timeout_sec, stop_flag,
            checkpoint_file, checkpoint_interval_sec
        )


def _enumerate_with_encoding_queue(
    solver: Solver,
    log_interval: int,
    timeout_sec: float | None,
    stop_flag: list | None,
    checkpoint_file: Path | None,
    checkpoint_interval_sec: float,
) -> dict[int, int]:
    """
    BFS using encoding-based queue (memory efficient).

    Stores 64-bit encodings in queue instead of GameState objects.
    Decodes to GameState when processing each node.
    """
    from collections import deque
    from solver.encoding import decode_state

    unsolved: dict[int, int] = {}  # canonical -> depth
    visited: set[int] = set()
    nodes_explored = 0
    max_depth_seen = 0

    start_time = time.time()
    last_checkpoint_time = start_time

    # BFS queue stores (canonical_encoding, depth) - just integers!
    initial = GameState()
    initial_canonical = canonicalize(encode_state(initial))
    visited.add(initial_canonical)

    queue: deque[tuple[int, int]] = deque()
    queue.append((initial_canonical, 0))

    def save_checkpoint():
        if checkpoint_file and unsolved:
            save_positions_with_depth(unsolved, checkpoint_file)
            log(f"Checkpoint saved: {len(unsolved):,} positions to {checkpoint_file}")

    # Disable cyclic GC during BFS - our data structures (sets/dicts of ints)
    # have no cycles, but GC wastes enormous time traversing millions of objects.
    # Reference counting still works for non-cyclic cleanup.
    gc.disable()
    log("Disabled cyclic GC for BFS (will re-enable after)")

    try:
        while queue:
            now = time.time()
            elapsed = now - start_time

            if timeout_sec is not None and elapsed >= timeout_sec:
                log(f"Timeout after {elapsed:.1f}s - returning partial results")
                break

            if stop_flag is not None and stop_flag[0]:
                log("Stop requested - returning partial results")
                break

            if checkpoint_file and (now - last_checkpoint_time >= checkpoint_interval_sec):
                save_checkpoint()
                last_checkpoint_time = now

            encoded, depth = queue.popleft()
            nodes_explored += 1
            max_depth_seen = max(max_depth_seen, depth)

            if nodes_explored % log_interval == 0:
                nodes_per_sec = nodes_explored / elapsed if elapsed > 0 else 0
                # Memory estimate: queue holds integers (16 bytes per entry for tuple of 2 ints)
                mem_estimate_mb = (len(visited) * 8 + len(queue) * 16 + len(unsolved) * 16) / 1024 / 1024
                log(
                    f"BFS[enc]: {nodes_explored:,} nodes ({nodes_per_sec:.0f}/s), "
                    f"{len(unsolved):,} unsolved, queue: {len(queue):,}, "
                    f"depth: {depth}, ~{mem_estimate_mb:.0f}MB, elapsed: {elapsed:.0f}s"
                )

            # Decode to GameState for move generation
            state = decode_state(encoded)

            # Generate all moves from this position
            for move in generate_moves(state):
                result, undo = apply_move_in_place(state, move)

                if result == GameResult.ONGOING:
                    child_canonical = canonicalize(encode_state(state))

                    if child_canonical not in visited:
                        visited.add(child_canonical)

                        if child_canonical not in solver.table:
                            unsolved[child_canonical] = depth + 1
                        else:
                            # Just store the encoding - no deepcopy!
                            queue.append((child_canonical, depth + 1))

                undo_move_in_place(state, undo)

    finally:
        gc.enable()
        log("Re-enabled cyclic GC")

    elapsed = time.time() - start_time
    log(
        f"BFS[enc] {'complete' if not queue else 'stopped'}: "
        f"{nodes_explored:,} nodes in {elapsed:.1f}s, "
        f"{len(unsolved):,} unsolved found, "
        f"max depth reached: {max_depth_seen}, "
        f"visited: {len(visited):,}"
    )

    if checkpoint_file:
        save_checkpoint()

    return unsolved


def _enumerate_with_state_queue(
    solver: Solver,
    log_interval: int,
    timeout_sec: float | None,
    stop_flag: list | None,
    checkpoint_file: Path | None,
    checkpoint_interval_sec: float,
) -> dict[int, int]:
    """
    BFS using GameState queue (original approach, for benchmarking).

    Stores full GameState objects via deepcopy. More memory intensive.
    """
    import copy
    from collections import deque

    unsolved: dict[int, int] = {}
    visited: set[int] = set()
    nodes_explored = 0
    max_depth_seen = 0

    start_time = time.time()
    last_checkpoint_time = start_time

    initial = GameState()
    initial_canonical = canonicalize(encode_state(initial))
    visited.add(initial_canonical)

    queue: deque[tuple[GameState, int]] = deque()
    queue.append((initial, 0))

    def save_checkpoint():
        if checkpoint_file and unsolved:
            save_positions_with_depth(unsolved, checkpoint_file)
            log(f"Checkpoint saved: {len(unsolved):,} positions to {checkpoint_file}")

    # Disable cyclic GC during BFS - same rationale as encoding queue version
    gc.disable()
    log("Disabled cyclic GC for BFS (will re-enable after)")

    try:
        while queue:
            now = time.time()
            elapsed = now - start_time

            if timeout_sec is not None and elapsed >= timeout_sec:
                log(f"Timeout after {elapsed:.1f}s - returning partial results")
                break

            if stop_flag is not None and stop_flag[0]:
                log("Stop requested - returning partial results")
                break

            if checkpoint_file and (now - last_checkpoint_time >= checkpoint_interval_sec):
                save_checkpoint()
                last_checkpoint_time = now

            state, depth = queue.popleft()
            nodes_explored += 1
            max_depth_seen = max(max_depth_seen, depth)

            if nodes_explored % log_interval == 0:
                nodes_per_sec = nodes_explored / elapsed if elapsed > 0 else 0
                mem_estimate_mb = (len(visited) * 8 + len(queue) * 200 + len(unsolved) * 16) / 1024 / 1024
                log(
                    f"BFS[state]: {nodes_explored:,} nodes ({nodes_per_sec:.0f}/s), "
                    f"{len(unsolved):,} unsolved, queue: {len(queue):,}, "
                    f"depth: {depth}, ~{mem_estimate_mb:.0f}MB, elapsed: {elapsed:.0f}s"
                )

            for move in generate_moves(state):
                result, undo = apply_move_in_place(state, move)

                if result == GameResult.ONGOING:
                    child_canonical = canonicalize(encode_state(state))

                    if child_canonical not in visited:
                        visited.add(child_canonical)

                        if child_canonical not in solver.table:
                            unsolved[child_canonical] = depth + 1
                        else:
                            child_state = copy.deepcopy(state)
                            queue.append((child_state, depth + 1))

                undo_move_in_place(state, undo)

    finally:
        gc.enable()
        log("Re-enabled cyclic GC")

    elapsed = time.time() - start_time
    log(
        f"BFS[state] {'complete' if not queue else 'stopped'}: "
        f"{nodes_explored:,} nodes in {elapsed:.1f}s, "
        f"{len(unsolved):,} unsolved found, "
        f"max depth reached: {max_depth_seen}, "
        f"visited: {len(visited):,}"
    )

    if checkpoint_file:
        save_checkpoint()

    return unsolved


def save_positions_with_depth(positions: dict[int, int], output_file: Path) -> None:
    """Save positions with depth to JSON file."""
    output_file.parent.mkdir(parents=True, exist_ok=True)

    # Sort by depth descending (deepest first) for solving order
    sorted_positions = sorted(positions.items(), key=lambda x: -x[1])

    data = {
        "count": len(positions),
        "min_depth": min(positions.values()) if positions else 0,
        "max_depth": max(positions.values()) if positions else 0,
        "positions": [{"canonical": c, "depth": d} for c, d in sorted_positions],
        "timestamp": datetime.now().isoformat(),
    }

    with open(output_file, "w") as f:
        json.dump(data, f, indent=2)


def load_positions_with_depth(input_file: Path) -> list[tuple[int, int]]:
    """Load positions with depth from JSON file. Returns list of (canonical, depth) sorted by depth desc."""
    with open(input_file) as f:
        data = json.load(f)
    return [(p["canonical"], p["depth"]) for p in data["positions"]]


def collect_by_dfs(config: CollectConfig, solver: Solver | None = None) -> set[int]:
    """
    Collect positions at target depth using DFS.

    Explores the game tree depth-first, recording positions
    when reaching the target depth.
    """
    collected: set[int] = set()
    visited: set[int] = set()  # Avoid revisiting same positions

    state = GameState()
    initial_canonical = canonicalize(encode_state(state))
    visited.add(initial_canonical)

    # Stack: (depth, list of (move, undo) to backtrack)
    # We track moves made to reach current state
    path: list[tuple] = []  # List of (move, undo_info)

    def explore(depth: int) -> None:
        """Recursive DFS with iterative backtracking."""
        nonlocal state

        if len(collected) >= config.max_positions:
            return

        if depth == config.target_depth:
            canonical = canonicalize(encode_state(state))
            if config.use_cache and solver and canonical in solver.table:
                return  # Already solved
            collected.add(canonical)
            if len(collected) % 100 == 0:
                log(f"Collected: {len(collected):,}")
            return

        moves = generate_moves(state)
        for move in moves:
            if len(collected) >= config.max_positions:
                return

            result, undo = apply_move_in_place(state, move)

            if result == GameResult.ONGOING:
                child_canonical = canonicalize(encode_state(state))
                if child_canonical not in visited:
                    visited.add(child_canonical)
                    explore(depth + 1)

            undo_move_in_place(state, undo)

    explore(0)
    return collected


def save_positions(positions: set[int], output_file: Path) -> None:
    """Save collected positions to JSON file."""
    output_file.parent.mkdir(parents=True, exist_ok=True)

    data = {
        "count": len(positions),
        "positions": list(positions),
        "timestamp": datetime.now().isoformat(),
    }

    with open(output_file, "w") as f:
        json.dump(data, f)


def load_positions(input_file: Path) -> list[int]:
    """Load positions from JSON file."""
    with open(input_file) as f:
        data = json.load(f)
    return data["positions"]


def collect_positions(config: CollectConfig) -> set[int]:
    """
    Collect positions at target depth.

    Returns set of canonical position encodings.
    """
    log("=" * 60)
    log("POSITION COLLECTOR")
    log("=" * 60)
    log(f"Target depth: {config.target_depth}")
    log(f"Max positions: {config.max_positions:,}")
    log(f"Method: {config.method}")
    log(f"Output file: {config.output_file}")

    # Optionally load solver cache to skip already-solved positions
    solver = None
    if config.use_cache:
        log("Loading transposition table to skip solved positions...")
        solver = Solver()
        loaded = load_checkpoint(solver)
        log(f"Loaded {loaded:,} solved positions")

    # Handle graceful shutdown
    shutdown_requested = False

    def handle_signal(signum, frame):
        nonlocal shutdown_requested
        if shutdown_requested:
            log("Force quit")
            sys.exit(1)
        shutdown_requested = True
        log("Shutdown requested, saving collected positions...")

    signal.signal(signal.SIGINT, handle_signal)
    signal.signal(signal.SIGTERM, handle_signal)

    # Collect positions
    log("")
    log("Starting collection...")
    start_time = time.time()

    try:
        if config.method == "random":
            collected = collect_by_random_walks(config, solver)
            elapsed = time.time() - start_time
            log("")
            log("=" * 60)
            log("COLLECTION COMPLETE")
            log("=" * 60)
            log(f"Positions collected: {len(collected):,}")
            log(f"Time: {elapsed:.1f} seconds")
            if collected:
                save_positions(collected, config.output_file)
                log(f"Saved to: {config.output_file}")
            return collected

        elif config.method == "dfs":
            collected = collect_by_dfs(config, solver)
            elapsed = time.time() - start_time
            log("")
            log("=" * 60)
            log("COLLECTION COMPLETE")
            log("=" * 60)
            log(f"Positions collected: {len(collected):,}")
            log(f"Time: {elapsed:.1f} seconds")
            if collected:
                save_positions(collected, config.output_file)
                log(f"Saved to: {config.output_file}")
            return collected

        elif config.method == "unsolved":
            if solver is None:
                raise ValueError("'unsolved' method requires cache")
            collected = collect_unsolved_positions(config, solver)
            elapsed = time.time() - start_time
            log("")
            log("=" * 60)
            log("COLLECTION COMPLETE")
            log("=" * 60)
            log(f"Positions collected: {len(collected):,}")
            log(f"Time: {elapsed:.1f} seconds")
            if collected:
                save_positions(collected, config.output_file)
                log(f"Saved to: {config.output_file}")
            return collected

        elif config.method == "enumerate":
            if solver is None:
                raise ValueError("'enumerate' method requires cache")

            # Use stop_flag for signal-based interruption
            stop_flag = [False]

            def set_stop_flag(signum, frame):
                nonlocal shutdown_requested
                if stop_flag[0]:
                    log("Force quit")
                    sys.exit(1)
                stop_flag[0] = True
                shutdown_requested = True
                log("Interrupt received - will stop and save results...")

            # Override signal handlers for enumerate
            signal.signal(signal.SIGINT, set_stop_flag)
            signal.signal(signal.SIGTERM, set_stop_flag)

            if config.timeout_sec:
                log(f"Enumerating unsolved positions (timeout: {config.timeout_sec}s)...")
            else:
                log("Enumerating ALL unsolved positions with depth (Ctrl+C to stop and save)...")

            unsolved_with_depth = enumerate_all_unsolved_with_depth(
                solver,
                timeout_sec=config.timeout_sec,
                stop_flag=stop_flag,
                checkpoint_file=config.output_file,
                checkpoint_interval_sec=300.0,  # Save every 5 minutes
                use_encoding_queue=config.use_encoding_queue,
            )

            elapsed = time.time() - start_time
            log("")
            log("=" * 60)
            log("ENUMERATION " + ("COMPLETE" if not stop_flag[0] else "STOPPED"))
            log("=" * 60)
            log(f"Unsolved positions found: {len(unsolved_with_depth):,}")
            if unsolved_with_depth:
                log(f"Depth range: {min(unsolved_with_depth.values())} - {max(unsolved_with_depth.values())}")
            log(f"Time: {elapsed:.1f} seconds")
            if unsolved_with_depth:
                save_positions_with_depth(unsolved_with_depth, config.output_file)
                log(f"Saved to: {config.output_file}")
            return set(unsolved_with_depth.keys())

        else:
            raise ValueError(f"Unknown method: {config.method}")

    except KeyboardInterrupt:
        log("Interrupted")
        return set()


def main():
    """Entry point with command line argument support."""
    import argparse

    parser = argparse.ArgumentParser(description="Collect positions at target depth")
    parser.add_argument(
        "--depth", type=int, default=400,
        help="Target depth to collect positions at (default: 400)"
    )
    parser.add_argument(
        "--max", type=int, default=1000,
        help="Maximum positions to collect (default: 1000)"
    )
    parser.add_argument(
        "--method", choices=["random", "dfs", "unsolved", "enumerate"], default="unsolved",
        help="Collection method: 'enumerate' finds ALL unsolved with depth (default: unsolved)"
    )
    parser.add_argument(
        "--walks", type=int, default=10000,
        help="Number of random walks for random method (default: 10000)"
    )
    parser.add_argument(
        "--output", type=str, default="solver/depth_positions.json",
        help="Output file path (default: solver/depth_positions.json)"
    )
    parser.add_argument(
        "--no-cache", action="store_true",
        help="Don't skip positions already in transposition table"
    )
    parser.add_argument(
        "--timeout", type=float, default=None,
        help="Timeout in seconds for enumerate method (default: no timeout, Ctrl+C to stop)"
    )
    parser.add_argument(
        "--use-state-queue", action="store_true",
        help="Use GameState queue instead of encoding queue (for benchmarking)"
    )

    args = parser.parse_args()

    config = CollectConfig(
        target_depth=args.depth,
        max_positions=args.max,
        method=args.method,
        random_walks=args.walks,
        output_file=Path(args.output),
        use_cache=not args.no_cache,
        timeout_sec=args.timeout,
        use_encoding_queue=not args.use_state_queue,
    )

    collect_positions(config)


if __name__ == "__main__":
    main()
