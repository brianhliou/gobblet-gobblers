"""
Fast in-place move application with undo support.

These functions mutate GameState directly instead of copying,
providing significant speedup for the solver.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING

from gobblet.game import GameResult
from gobblet.types import Piece, Player

if TYPE_CHECKING:
    from gobblet.moves import Move
    from gobblet.state import GameState


@dataclass
class UndoInfo:
    """Information needed to reverse a move."""

    move: Move
    piece: Piece
    move_completed: bool  # False if reveal loss (piece removed but not placed)
    player_switched: bool  # Whether current_player was changed


def apply_move_in_place(state: GameState, move: Move) -> tuple[GameResult, UndoInfo]:
    """
    Apply a move directly to the state (mutating).

    Returns:
        (game_result, undo_info) - The result and info needed to undo.

    Note: This skips position history tracking since the solver
    uses its own cycle detection.
    """
    player = state.current_player
    opponent = player.opponent()

    if move.is_from_reserve:
        # Reserve placement
        assert move.size is not None
        piece = Piece(player, move.size)

        # Decrement reserve
        state._reserves[(player, move.size)] -= 1

        # Place piece
        row, col = move.to_pos
        state._board[row][col].append(piece)

        move_completed = True

    else:
        # Board move
        assert move.from_pos is not None
        from_row, from_col = move.from_pos

        # Remove piece from source
        piece = state._board[from_row][from_col].pop()

        # Check reveal rule: does opponent win after lift?
        opponent_winning_lines = state.get_winning_lines(opponent)

        if opponent_winning_lines:
            # Check if destination breaks all winning lines
            to_row, to_col = move.to_pos
            can_save = False

            for line in opponent_winning_lines:
                if move.to_pos in line:
                    target = state.get_top(move.to_pos)
                    if target is not None and piece.can_gobble(target):
                        can_save = True
                        break

            if not can_save:
                # Reveal loss - piece was lifted but cannot save
                # Don't place the piece, opponent wins
                # Don't switch player for terminal states (matches play_move)
                return GameResult.winner(opponent), UndoInfo(move, piece, False, False)

        # Complete the move - place piece at destination
        to_row, to_col = move.to_pos
        state._board[to_row][to_col].append(piece)
        move_completed = True

    # Check for winner
    winner = state.check_winner()
    if winner is not None:
        # Don't switch player for terminal states (matches play_move behavior)
        return GameResult.winner(winner), UndoInfo(move, piece, move_completed, False)

    # Check for draw (threefold repetition) - skip for solver
    # The solver uses path-based cycle detection instead

    # Switch player (only for ongoing games)
    state.current_player = opponent

    return GameResult.ONGOING, UndoInfo(move, piece, move_completed, True)


def undo_move_in_place(state: GameState, undo: UndoInfo) -> None:
    """
    Reverse a move, restoring the state to before the move.

    Must be called with the same state that apply_move_in_place was called on.
    """
    move = undo.move
    piece = undo.piece

    # Switch player back only if it was switched during apply
    if undo.player_switched:
        state.current_player = state.current_player.opponent()

    if not undo.move_completed:
        # Reveal loss - piece was removed but not placed
        # Just put it back at the source
        assert move.from_pos is not None
        from_row, from_col = move.from_pos
        state._board[from_row][from_col].append(piece)

    elif move.is_from_reserve:
        # Reserve placement - remove from board, add back to reserve
        to_row, to_col = move.to_pos
        state._board[to_row][to_col].pop()
        state._reserves[(piece.player, piece.size)] += 1

    else:
        # Board move - move piece back to source
        assert move.from_pos is not None
        to_row, to_col = move.to_pos
        from_row, from_col = move.from_pos

        state._board[to_row][to_col].pop()
        state._board[from_row][from_col].append(piece)
