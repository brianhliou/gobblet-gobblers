"""
Minimax solver for Gobblet Gobblers.

Solves the game by exhaustively exploring all reachable positions
and storing outcomes in a transposition table.

Uses an iterative approach with explicit stack to avoid Python's recursion limit.
"""

from __future__ import annotations

import gc
from dataclasses import dataclass, field
from enum import IntEnum
from typing import TYPE_CHECKING

from gobblet.game import GameResult, play_move
from gobblet.moves import generate_moves, move_to_notation
from gobblet.state import GameState
from gobblet.types import Player

from solver.encoding import canonicalize, encode_state
from solver.fast_move import UndoInfo, apply_move_in_place, undo_move_in_place

if TYPE_CHECKING:
    from gobblet.moves import Move


class Outcome(IntEnum):
    """Game outcome from solver's perspective."""
    WIN_P2 = -1  # Player 2 wins with optimal play
    DRAW = 0     # Draw with optimal play
    WIN_P1 = 1   # Player 1 wins with optimal play


@dataclass
class SolverStats:
    """Statistics from a solver run."""
    positions_evaluated: int = 0
    cache_hits: int = 0
    terminal_positions: int = 0
    cycle_draws: int = 0
    max_depth: int = 0


@dataclass
class StackFrame:
    """A frame on the explicit call stack for iterative minimax."""
    state: GameState
    canonical: int
    moves: list  # List of (Move, child_state, child_canonical) or (Move, child_canonical, GameResult)
    move_idx: int = 0
    child_outcomes: list[Outcome] = field(default_factory=list)
    undo_on_pop: UndoInfo | None = None  # For undo-based solver: how to restore parent state
    # Note: path tracking moved to shared mutable set for memory efficiency


class Solver:
    """
    Minimax solver with transposition table and cycle detection.

    Uses iterative deepening with explicit stack to avoid recursion limits.

    Usage:
        solver = Solver()
        outcome = solver.solve()  # Solve from initial position

        # Query any position
        outcome = solver.get_outcome(some_state)
        best_move = solver.get_best_move(some_state)
    """

    def __init__(self) -> None:
        # Transposition table: canonical state -> outcome
        self.table: dict[int, Outcome] = {}
        self.stats = SolverStats()

        # For progress reporting
        self._last_report_count = 0
        self._report_interval = 100_000
        self._prune = True  # Alpha-beta pruning enabled by default
        self._force = False  # Don't re-explore solved positions by default

    def solve(self, state: GameState | None = None, fast: bool = True, prune: bool = True, force: bool = False) -> Outcome:
        """
        Solve the game from a given state (default: initial position).

        Args:
            state: Starting position (default: initial)
            fast: Use optimized undo-based implementation (default: True)
            prune: Use alpha-beta pruning (default: True). Set False to explore all positions.
            force: Force re-exploration even if position is already solved (default: False).
                   Useful with prune=False to explore positions that were previously pruned.

        Returns the outcome with optimal play from both sides.
        """
        if state is None:
            state = GameState()

        self.stats = SolverStats()  # Reset stats
        self._prune = prune
        self._force = force

        if fast:
            return self._solve_iterative_fast(state)
        else:
            return self._solve_iterative(state)

    def _solve_iterative(self, initial_state: GameState) -> Outcome:
        """
        Iterative minimax with explicit stack and alpha-beta pruning.

        Alpha-beta pruning allows us to stop exploring a position once we know
        the outcome can't be improved (P1 found a win, or P2 found a win).

        Uses a shared mutable set for path tracking (cycle detection) instead of
        frozenset per frame, reducing memory from O(depth²) to O(depth).
        """
        initial_canonical = canonicalize(encode_state(initial_state))

        # Check if already solved (unless force=True)
        if not self._force and initial_canonical in self.table:
            return self.table[initial_canonical]

        # Stack of frames (simulating recursive call stack)
        stack: list[StackFrame] = []

        # Shared path set for cycle detection - add when pushing, remove when popping
        path_set: set[int] = set()

        # Push initial frame
        initial_frame = self._create_frame(initial_state, initial_canonical)
        if initial_frame is None:
            # Terminal position, already handled
            return self.table[initial_canonical]
        stack.append(initial_frame)
        path_set.add(initial_canonical)

        # Disable cyclic GC during solve (see _solve_iterative_fast for rationale)
        gc.disable()

        try:
            while stack:
                frame = stack[-1]

                # Alpha-beta pruning: check if we can stop early
                if self._prune and frame.child_outcomes:
                    current_best = max(frame.child_outcomes) if frame.state.current_player == Player.ONE else min(frame.child_outcomes)
                    # P1 found a win - no need to explore more (P1 maximizes)
                    if frame.state.current_player == Player.ONE and current_best == Outcome.WIN_P1:
                        frame.move_idx = len(frame.moves)  # Skip remaining moves
                    # P2 found a win - no need to explore more (P2 minimizes)
                    elif frame.state.current_player == Player.TWO and current_best == Outcome.WIN_P2:
                        frame.move_idx = len(frame.moves)  # Skip remaining moves

                # Process next child move
                if frame.move_idx < len(frame.moves):
                    move, child_state, child_canonical = frame.moves[frame.move_idx]
                    frame.move_idx += 1

                    # Check for cycle using shared path set
                    if child_canonical in path_set:
                        self.stats.cycle_draws += 1
                        frame.child_outcomes.append(Outcome.DRAW)
                        continue

                    # Check transposition table
                    if child_canonical in self.table:
                        self.stats.cache_hits += 1
                        frame.child_outcomes.append(self.table[child_canonical])
                        continue

                    # Need to explore this child - push new frame
                    child_frame = self._create_frame(child_state, child_canonical)

                    if child_frame is None:
                        # Terminal position, outcome already in table
                        frame.child_outcomes.append(self.table[child_canonical])
                    else:
                        stack.append(child_frame)
                        path_set.add(child_canonical)  # Add to path when pushing
                        self.stats.max_depth = max(self.stats.max_depth, len(stack))

                else:
                    # All children processed, compute outcome for this frame
                    stack.pop()
                    path_set.discard(frame.canonical)  # Remove from path when popping

                    if frame.child_outcomes:
                        if frame.state.current_player == Player.ONE:
                            outcome = max(frame.child_outcomes)
                        else:
                            outcome = min(frame.child_outcomes)
                    else:
                        # No moves = zugzwang, current player loses
                        self.stats.terminal_positions += 1
                        outcome = Outcome.WIN_P2 if frame.state.current_player == Player.ONE else Outcome.WIN_P1

                    self.table[frame.canonical] = outcome

                    # Report progress
                    self.stats.positions_evaluated += 1
                    if self.stats.positions_evaluated - self._last_report_count >= self._report_interval:
                        self._report_progress()

                    # Pass outcome to parent frame
                    if stack:
                        stack[-1].child_outcomes.append(outcome)

        finally:
            gc.enable()

        return self.table[initial_canonical]

    def _solve_iterative_fast(self, initial_state: GameState) -> Outcome:
        """
        Fast iterative minimax using in-place move application with undo.

        This version avoids copying GameState for each child, instead mutating
        the state and undoing when backtracking.

        Uses a shared mutable set for path tracking (cycle detection) instead of
        frozenset per frame, reducing memory from O(depth²) to O(depth).
        """
        initial_canonical = canonicalize(encode_state(initial_state))

        # Check if already solved (unless force=True)
        if not self._force and initial_canonical in self.table:
            return self.table[initial_canonical]

        # Stack of frames
        stack: list[StackFrame] = []

        # Shared path set for cycle detection - add when pushing, remove when popping
        # This is O(max_depth) memory instead of O(depth²) with frozenset per frame
        path_set: set[int] = set()

        # Push initial frame
        initial_frame = self._create_frame_fast(initial_state, initial_canonical)
        if initial_frame is None:
            return self.table[initial_canonical]
        stack.append(initial_frame)
        path_set.add(initial_canonical)

        # Disable cyclic GC during solve - our data structures (dicts/sets of ints)
        # have no reference cycles. With millions of objects, GC wastes enormous
        # time traversing them. Reference counting still handles cleanup.
        gc.disable()

        try:
            while stack:
                frame = stack[-1]

                # Alpha-beta pruning
                if self._prune and frame.child_outcomes:
                    current_best = max(frame.child_outcomes) if frame.state.current_player == Player.ONE else min(frame.child_outcomes)
                    if frame.state.current_player == Player.ONE and current_best == Outcome.WIN_P1:
                        frame.move_idx = len(frame.moves)
                    elif frame.state.current_player == Player.TWO and current_best == Outcome.WIN_P2:
                        frame.move_idx = len(frame.moves)

                # Process next child move
                if frame.move_idx < len(frame.moves):
                    move, child_canonical, game_result = frame.moves[frame.move_idx]
                    frame.move_idx += 1

                    # Check for cycle using shared path set
                    if child_canonical in path_set:
                        self.stats.cycle_draws += 1
                        frame.child_outcomes.append(Outcome.DRAW)
                        continue

                    # Check transposition table
                    if child_canonical in self.table:
                        self.stats.cache_hits += 1
                        frame.child_outcomes.append(self.table[child_canonical])
                        continue

                    # Terminal positions were already added to table in _create_frame_fast
                    if game_result != GameResult.ONGOING:
                        frame.child_outcomes.append(self.table[child_canonical])
                        continue

                    # Need to explore this child - apply move and push frame
                    result, undo = apply_move_in_place(frame.state, move)

                    child_frame = self._create_frame_fast(frame.state, child_canonical)

                    if child_frame is None:
                        # Terminal position, outcome already in table
                        frame.child_outcomes.append(self.table[child_canonical])
                        undo_move_in_place(frame.state, undo)
                    else:
                        child_frame.undo_on_pop = undo
                        stack.append(child_frame)
                        path_set.add(child_canonical)  # Add to path when pushing
                        self.stats.max_depth = max(self.stats.max_depth, len(stack))

                else:
                    # All children processed, pop and compute outcome
                    stack.pop()
                    path_set.discard(frame.canonical)  # Remove from path when popping

                    # IMPORTANT: Compute outcome BEFORE undoing, while state still reflects this frame
                    if frame.child_outcomes:
                        if frame.state.current_player == Player.ONE:
                            outcome = max(frame.child_outcomes)
                        else:
                            outcome = min(frame.child_outcomes)
                    else:
                        # No moves = zugzwang
                        self.stats.terminal_positions += 1
                        outcome = Outcome.WIN_P2 if frame.state.current_player == Player.ONE else Outcome.WIN_P1

                    self.table[frame.canonical] = outcome

                    # Undo the move that led to this frame (restore parent state)
                    # Must happen AFTER computing outcome since we need frame's current_player
                    if frame.undo_on_pop:
                        undo_move_in_place(frame.state, frame.undo_on_pop)

                    self.stats.positions_evaluated += 1
                    if self.stats.positions_evaluated - self._last_report_count >= self._report_interval:
                        self._report_progress()

                    if stack:
                        stack[-1].child_outcomes.append(outcome)

        finally:
            gc.enable()

        return self.table[initial_canonical]

    def _create_frame_fast(
        self, state: GameState, canonical: int
    ) -> StackFrame | None:
        """
        Create a stack frame using fast apply/undo for move generation.

        Returns None if the state is terminal (and stores outcome in table).
        Note: Path tracking is handled externally via shared mutable set.
        """
        moves_info: list[tuple[Move, int, GameResult]] = []

        for move in generate_moves(state):
            # Apply move in place
            game_result, undo = apply_move_in_place(state, move)
            child_canonical = canonicalize(encode_state(state))

            if game_result != GameResult.ONGOING:
                # Game ended with this move
                self.stats.terminal_positions += 1
                child_outcome = self._game_result_to_outcome(game_result)
                self.table[child_canonical] = child_outcome

            moves_info.append((move, child_canonical, game_result))

            # Undo to restore state for next move
            undo_move_in_place(state, undo)

        if not moves_info:
            # No legal moves = zugzwang
            self.stats.terminal_positions += 1
            outcome = Outcome.WIN_P2 if state.current_player == Player.ONE else Outcome.WIN_P1
            self.table[canonical] = outcome
            return None

        # Move ordering
        best_outcome = Outcome.WIN_P1 if state.current_player == Player.ONE else Outcome.WIN_P2

        def move_priority(item: tuple) -> int:
            _, child_canonical, _ = item
            if child_canonical in self.table:
                outcome = self.table[child_canonical]
                if outcome == best_outcome:
                    return 0
                elif outcome == Outcome.DRAW:
                    return 1
                else:
                    return 2
            return 1

        moves_info.sort(key=move_priority)

        return StackFrame(
            state=state,
            canonical=canonical,
            moves=moves_info,
        )

    def _create_frame(
        self, state: GameState, canonical: int
    ) -> StackFrame | None:
        """
        Create a stack frame for a state.

        Returns None if the state is terminal (and stores outcome in table).
        Note: Path tracking is handled externally via shared mutable set.
        """
        # Get legal moves and check for terminal/game-ending moves
        moves_with_children: list[tuple[Move, GameState, int]] = []

        for move in generate_moves(state):
            child_state, game_result = play_move(state, move)

            if game_result != GameResult.ONGOING:
                # Game ended with this move
                self.stats.terminal_positions += 1
                child_outcome = self._game_result_to_outcome(game_result)
                child_canonical = canonicalize(encode_state(child_state))
                self.table[child_canonical] = child_outcome
                moves_with_children.append((move, child_state, child_canonical))
            else:
                child_canonical = canonicalize(encode_state(child_state))
                moves_with_children.append((move, child_state, child_canonical))

        if not moves_with_children:
            # No legal moves = zugzwang
            self.stats.terminal_positions += 1
            outcome = Outcome.WIN_P2 if state.current_player == Player.ONE else Outcome.WIN_P1
            self.table[canonical] = outcome
            return None

        # Move ordering: prioritize moves that are best for current player
        # This dramatically improves alpha-beta pruning efficiency
        best_outcome = Outcome.WIN_P1 if state.current_player == Player.ONE else Outcome.WIN_P2

        def move_priority(item: tuple) -> int:
            """Lower value = higher priority (explored first)."""
            _, _, child_canonical = item
            if child_canonical in self.table:
                outcome = self.table[child_canonical]
                if outcome == best_outcome:
                    return 0  # Best outcome for current player - explore first!
                elif outcome == Outcome.DRAW:
                    return 1  # Draw - second priority
                else:
                    return 2  # Losing - explore last
            return 1  # Unknown - middle priority

        moves_with_children.sort(key=move_priority)

        return StackFrame(
            state=state,
            canonical=canonical,
            moves=moves_with_children,
        )

    def _game_result_to_outcome(self, result: GameResult) -> Outcome:
        """Convert GameResult to solver Outcome."""
        if result == GameResult.PLAYER_ONE_WINS:
            return Outcome.WIN_P1
        elif result == GameResult.PLAYER_TWO_WINS:
            return Outcome.WIN_P2
        elif result == GameResult.DRAW:
            return Outcome.DRAW
        else:
            raise ValueError(f"Cannot convert ongoing game to outcome: {result}")

    def _report_progress(self) -> None:
        """Print progress update."""
        self._last_report_count = self.stats.positions_evaluated
        print(
            f"Progress: {self.stats.positions_evaluated:,} positions, "
            f"{len(self.table):,} unique, "
            f"{self.stats.cache_hits:,} cache hits, "
            f"depth {self.stats.max_depth}"
        )

    def get_outcome(self, state: GameState) -> Outcome | None:
        """
        Get the solved outcome for a position.

        Returns None if position hasn't been solved yet.
        """
        canonical = canonicalize(encode_state(state))
        return self.table.get(canonical)

    def get_best_move(self, state: GameState) -> tuple[Move, Outcome] | None:
        """
        Get the best move for the current player.

        Returns (move, resulting_outcome) or None if no moves or unsolved.
        """
        moves = generate_moves(state)
        if not moves:
            return None

        current_player = state.current_player
        best_move = None
        best_outcome = None

        for move in moves:
            child_state, game_result = play_move(state, move)

            if game_result != GameResult.ONGOING:
                child_outcome = self._game_result_to_outcome(game_result)
            else:
                child_outcome = self.get_outcome(child_state)
                if child_outcome is None:
                    continue  # Position not solved

            # Update best based on current player's preference
            if best_outcome is None:
                best_move, best_outcome = move, child_outcome
            elif current_player == Player.ONE and child_outcome > best_outcome:
                best_move, best_outcome = move, child_outcome
            elif current_player == Player.TWO and child_outcome < best_outcome:
                best_move, best_outcome = move, child_outcome

        if best_move is None:
            return None
        return best_move, best_outcome

    def get_all_move_outcomes(self, state: GameState) -> list[tuple[Move, Outcome | None]]:
        """
        Get outcomes for all legal moves from a position.

        Returns list of (move, outcome) pairs. Outcome is None if unsolved.
        """
        results = []

        for move in generate_moves(state):
            child_state, game_result = play_move(state, move)

            if game_result != GameResult.ONGOING:
                child_outcome = self._game_result_to_outcome(game_result)
            else:
                child_outcome = self.get_outcome(child_state)

            results.append((move, child_outcome))

        return results


def solve_game() -> Solver:
    """
    Convenience function to solve the entire game from initial position.

    Returns the solver with populated transposition table.
    """
    solver = Solver()
    outcome = solver.solve()

    print(f"\nSolving complete!")
    print(f"  Initial position outcome: {outcome.name}")
    print(f"  Positions evaluated: {solver.stats.positions_evaluated:,}")
    print(f"  Unique positions: {len(solver.table):,}")
    print(f"  Cache hits: {solver.stats.cache_hits:,}")
    print(f"  Terminal positions: {solver.stats.terminal_positions:,}")
    print(f"  Cycle draws: {solver.stats.cycle_draws:,}")
    print(f"  Max depth: {solver.stats.max_depth}")

    return solver
