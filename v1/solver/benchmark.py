#!/usr/bin/env python3
"""
Benchmark different components of the solver to identify bottlenecks.
"""

import time
from gobblet.state import GameState
from gobblet.game import play_move
from gobblet.moves import generate_moves
from solver.encoding import encode_state, canonicalize


def benchmark_move_generation(state: GameState, iterations: int = 10000) -> float:
    """Benchmark move generation."""
    start = time.perf_counter()
    for _ in range(iterations):
        list(generate_moves(state))
    elapsed = time.perf_counter() - start
    return elapsed / iterations


def benchmark_play_move(state: GameState, iterations: int = 10000) -> float:
    """Benchmark playing a move."""
    moves = list(generate_moves(state))
    if not moves:
        return 0.0
    move = moves[0]

    start = time.perf_counter()
    for _ in range(iterations):
        play_move(state, move)
    elapsed = time.perf_counter() - start
    return elapsed / iterations


def benchmark_encoding(state: GameState, iterations: int = 10000) -> float:
    """Benchmark state encoding."""
    start = time.perf_counter()
    for _ in range(iterations):
        encode_state(state)
    elapsed = time.perf_counter() - start
    return elapsed / iterations


def benchmark_canonicalization(state: GameState, iterations: int = 10000) -> float:
    """Benchmark canonicalization (includes encoding)."""
    encoded = encode_state(state)

    start = time.perf_counter()
    for _ in range(iterations):
        canonicalize(encoded)
    elapsed = time.perf_counter() - start
    return elapsed / iterations


def benchmark_full_child_generation(state: GameState, iterations: int = 1000) -> float:
    """Benchmark generating all children with canonicalization."""
    start = time.perf_counter()
    for _ in range(iterations):
        for move in generate_moves(state):
            child_state, result = play_move(state, move)
            encoded = encode_state(child_state)
            canonical = canonicalize(encoded)
    elapsed = time.perf_counter() - start
    return elapsed / iterations


def benchmark_dict_lookup(iterations: int = 100000) -> float:
    """Benchmark dict lookup with int keys."""
    # Simulate transposition table
    table = {i: i % 3 - 1 for i in range(1000000)}
    keys = list(range(0, 1000000, 100))  # 10000 keys to look up

    start = time.perf_counter()
    for _ in range(iterations // len(keys)):
        for k in keys:
            _ = table.get(k)
    elapsed = time.perf_counter() - start
    return elapsed / iterations


def run_benchmarks():
    """Run all benchmarks on different game states."""

    print("=" * 60)
    print("SOLVER COMPONENT BENCHMARKS")
    print("=" * 60)

    # Initial state
    initial = GameState()
    print(f"\n--- Initial State (empty board) ---")
    print(f"Move generation:     {benchmark_move_generation(initial) * 1e6:.1f} µs")
    print(f"Play move:           {benchmark_play_move(initial) * 1e6:.1f} µs")
    print(f"Encode state:        {benchmark_encoding(initial) * 1e6:.1f} µs")
    print(f"Canonicalize:        {benchmark_canonicalization(initial) * 1e6:.1f} µs")
    print(f"Full child gen:      {benchmark_full_child_generation(initial) * 1e6:.1f} µs")

    num_moves = len(list(generate_moves(initial)))
    print(f"Number of moves:     {num_moves}")

    # After a few moves
    state = initial
    moves_made = []
    for i, move in enumerate(generate_moves(state)):
        if i >= 4:
            break
        state, _ = play_move(state, move)
        moves_made.append(move)

    print(f"\n--- After 4 moves ---")
    print(f"Move generation:     {benchmark_move_generation(state) * 1e6:.1f} µs")
    print(f"Play move:           {benchmark_play_move(state) * 1e6:.1f} µs")
    print(f"Encode state:        {benchmark_encoding(state) * 1e6:.1f} µs")
    print(f"Canonicalize:        {benchmark_canonicalization(state) * 1e6:.1f} µs")
    print(f"Full child gen:      {benchmark_full_child_generation(state) * 1e6:.1f} µs")

    num_moves = len(list(generate_moves(state)))
    print(f"Number of moves:     {num_moves}")

    # Dict lookup baseline
    print(f"\n--- Transposition Table Lookup ---")
    print(f"Dict lookup:         {benchmark_dict_lookup() * 1e6:.3f} µs")

    # Estimate throughput
    print(f"\n--- Throughput Estimate ---")
    child_gen_time = benchmark_full_child_generation(initial)
    moves_per_pos = len(list(generate_moves(initial)))
    time_per_position = child_gen_time  # Time to process all children
    positions_per_sec = 1.0 / time_per_position if time_per_position > 0 else 0
    print(f"Time per position:   {time_per_position * 1000:.2f} ms")
    print(f"Estimated max rate:  {positions_per_sec:.0f} positions/sec")
    print(f"(This is upper bound - doesn't include stack management, sorting, etc.)")


if __name__ == "__main__":
    run_benchmarks()
