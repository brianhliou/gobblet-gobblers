"""Tests for fast in-place move application with undo."""

import pytest

from gobblet.game import GameResult, play_move
from gobblet.moves import Move, generate_moves
from gobblet.state import GameState
from gobblet.types import Piece, Player, Size
from solver.encoding import encode_state
from solver.fast_move import apply_move_in_place, undo_move_in_place


def states_equal(s1: GameState, s2: GameState) -> bool:
    """Check if two states are equal by comparing their encodings and reserves."""
    # Compare board state via encoding
    if encode_state(s1) != encode_state(s2):
        return False

    # Compare reserves (encoding doesn't include reserves)
    for player in Player:
        for size in Size:
            if s1.get_reserve(player, size) != s2.get_reserve(player, size):
                return False

    return True


def state_snapshot(state: GameState) -> tuple:
    """Create a hashable snapshot of state for comparison."""
    board = tuple(
        tuple(tuple(p for p in stack) for stack in row)
        for row in state._board
    )
    # Sort by (player.value, size.value) to make it deterministic
    reserves = tuple(sorted(
        state._reserves.items(),
        key=lambda x: (x[0][0].value, x[0][1].value)
    ))
    return (board, reserves, state.current_player)


class TestApplyUndoRoundtrip:
    """Test that apply followed by undo restores original state."""

    def test_reserve_placement_roundtrip(self):
        """Reserve placement can be undone."""
        state = GameState()
        original = state_snapshot(state)

        move = Move(player=Player.ONE, to_pos=(1, 1), size=Size.SMALL)
        result, undo = apply_move_in_place(state, move)

        assert result == GameResult.ONGOING
        assert state.current_player == Player.TWO
        assert state.get_top((1, 1)) == Piece(Player.ONE, Size.SMALL)
        assert state.get_reserve(Player.ONE, Size.SMALL) == 1

        undo_move_in_place(state, undo)

        assert state_snapshot(state) == original

    def test_board_move_roundtrip(self):
        """Board move can be undone."""
        state = GameState()

        # Place a piece first
        state._board[0][0].append(Piece(Player.ONE, Size.SMALL))
        state._reserves[(Player.ONE, Size.SMALL)] -= 1

        original = state_snapshot(state)

        move = Move(player=Player.ONE, to_pos=(1, 1), from_pos=(0, 0))
        result, undo = apply_move_in_place(state, move)

        assert result == GameResult.ONGOING
        assert state.is_empty((0, 0))
        assert state.get_top((1, 1)) == Piece(Player.ONE, Size.SMALL)

        undo_move_in_place(state, undo)

        assert state_snapshot(state) == original

    def test_gobble_move_roundtrip(self):
        """Gobbling move can be undone."""
        state = GameState()

        # Set up: P1 Large at (0,0), P2 Small at (1,1)
        state._board[0][0].append(Piece(Player.ONE, Size.LARGE))
        state._board[1][1].append(Piece(Player.TWO, Size.SMALL))
        state._reserves[(Player.ONE, Size.LARGE)] -= 1
        state._reserves[(Player.TWO, Size.SMALL)] -= 1

        original = state_snapshot(state)

        # P1 gobbles P2's piece
        move = Move(player=Player.ONE, to_pos=(1, 1), from_pos=(0, 0))
        result, undo = apply_move_in_place(state, move)

        assert result == GameResult.ONGOING
        assert state.is_empty((0, 0))
        # Stack should have P2S on bottom, P1L on top
        stack = state.get_stack((1, 1))
        assert len(stack) == 2
        assert stack[0] == Piece(Player.TWO, Size.SMALL)
        assert stack[1] == Piece(Player.ONE, Size.LARGE)

        undo_move_in_place(state, undo)

        assert state_snapshot(state) == original

    def test_winning_move_roundtrip(self):
        """Winning move can be undone."""
        state = GameState()

        # Set up P1 about to win (two in a row)
        state._board[0][0].append(Piece(Player.ONE, Size.SMALL))
        state._board[0][1].append(Piece(Player.ONE, Size.MEDIUM))
        state._reserves[(Player.ONE, Size.SMALL)] -= 1
        state._reserves[(Player.ONE, Size.MEDIUM)] -= 1

        original = state_snapshot(state)

        # P1 completes the row
        move = Move(player=Player.ONE, to_pos=(0, 2), size=Size.LARGE)
        result, undo = apply_move_in_place(state, move)

        assert result == GameResult.PLAYER_ONE_WINS

        undo_move_in_place(state, undo)

        assert state_snapshot(state) == original


class TestRevealRuleUndo:
    """Test undo for reveal rule scenarios."""

    def test_reveal_loss_roundtrip(self):
        """Reveal loss move can be undone."""
        state = GameState()

        # Set up: P2 Large on top of P1 Small at (0,0)
        # P1 has two more pieces in column 0
        state._board[0][0].append(Piece(Player.ONE, Size.SMALL))
        state._board[0][0].append(Piece(Player.TWO, Size.LARGE))
        state._board[1][0].append(Piece(Player.ONE, Size.MEDIUM))
        state._board[2][0].append(Piece(Player.ONE, Size.LARGE))

        state._reserves[(Player.ONE, Size.SMALL)] -= 1
        state._reserves[(Player.ONE, Size.MEDIUM)] -= 1
        state._reserves[(Player.ONE, Size.LARGE)] -= 1
        state._reserves[(Player.TWO, Size.LARGE)] -= 1

        state.current_player = Player.TWO

        original = state_snapshot(state)

        # P2 lifts large from (0,0), revealing P1's winning column
        # P2 tries to move to (1,1) which doesn't block - reveal loss
        move = Move(player=Player.TWO, to_pos=(1, 1), from_pos=(0, 0))
        result, undo = apply_move_in_place(state, move)

        assert result == GameResult.PLAYER_ONE_WINS
        assert not undo.move_completed  # Piece was not placed

        undo_move_in_place(state, undo)

        assert state_snapshot(state) == original

    def test_reveal_save_roundtrip(self):
        """Reveal with successful save can be undone."""
        state = GameState()

        # Set up: P2 Large at (0,0) on top of P1 Small
        # P1 has winning column 0 when P2 lifts
        state._board[0][0].append(Piece(Player.ONE, Size.SMALL))
        state._board[0][0].append(Piece(Player.TWO, Size.LARGE))
        state._board[1][0].append(Piece(Player.ONE, Size.MEDIUM))
        state._board[2][0].append(Piece(Player.ONE, Size.SMALL))

        state._reserves[(Player.ONE, Size.SMALL)] -= 2
        state._reserves[(Player.ONE, Size.MEDIUM)] -= 1
        state._reserves[(Player.TWO, Size.LARGE)] -= 1

        state.current_player = Player.TWO

        original = state_snapshot(state)

        # P2 lifts and gobbles into the winning line at (1,0)
        move = Move(player=Player.TWO, to_pos=(1, 0), from_pos=(0, 0))
        result, undo = apply_move_in_place(state, move)

        # Should be ongoing - P2 saved by gobbling into the line
        assert result == GameResult.ONGOING
        assert undo.move_completed

        undo_move_in_place(state, undo)

        assert state_snapshot(state) == original


class TestEquivalenceWithPlayMove:
    """Test that apply_move_in_place gives same results as play_move."""

    def test_equivalence_on_random_positions(self):
        """Apply should give same result as play_move for many positions."""
        # Start from initial state and play some random moves
        state = GameState()

        for _ in range(100):  # Test 100 move sequences
            test_state = state.copy()
            moves = generate_moves(test_state)

            if not moves:
                break

            for move in moves[:5]:  # Test first 5 moves from each position
                # Get result from copy-based play_move
                expected_state, expected_result = play_move(test_state, move)

                # Get result from in-place apply
                test_copy = test_state.copy()
                actual_result, undo = apply_move_in_place(test_copy, move)

                # Results should match
                assert actual_result == expected_result, f"Result mismatch for {move}"

                # States should match (for completed moves)
                if undo.move_completed:
                    assert states_equal(test_copy, expected_state), f"State mismatch for {move}"

                # Undo should restore original
                original_snapshot = state_snapshot(test_state.copy())
                undo_move_in_place(test_copy, undo)
                assert state_snapshot(test_copy) == original_snapshot, f"Undo failed for {move}"

            # Make a move and continue
            if moves:
                state, _ = play_move(state, moves[0])


class TestAllMovesFromInitial:
    """Test all moves from initial position."""

    def test_all_initial_moves_roundtrip(self):
        """Every legal move from initial position can be undone."""
        state = GameState()
        original = state_snapshot(state)

        moves = generate_moves(state)
        assert len(moves) == 27  # 9 positions × 3 sizes

        for move in moves:
            result, undo = apply_move_in_place(state, move)
            assert result == GameResult.ONGOING
            undo_move_in_place(state, undo)
            assert state_snapshot(state) == original, f"Failed for {move}"


class TestEdgeCases:
    """Test edge cases."""

    def test_deep_stack_roundtrip(self):
        """Moving from/to deep stacks works correctly."""
        state = GameState()

        # Build a 3-piece stack at (0,0)
        state._board[0][0].append(Piece(Player.ONE, Size.SMALL))
        state._board[0][0].append(Piece(Player.TWO, Size.MEDIUM))
        state._board[0][0].append(Piece(Player.ONE, Size.LARGE))

        state._reserves[(Player.ONE, Size.SMALL)] -= 1
        state._reserves[(Player.TWO, Size.MEDIUM)] -= 1
        state._reserves[(Player.ONE, Size.LARGE)] -= 1

        original = state_snapshot(state)

        # Move top piece
        move = Move(player=Player.ONE, to_pos=(1, 1), from_pos=(0, 0))
        result, undo = apply_move_in_place(state, move)

        # Verify intermediate state
        assert len(state.get_stack((0, 0))) == 2
        assert state.get_top((0, 0)) == Piece(Player.TWO, Size.MEDIUM)
        assert state.get_top((1, 1)) == Piece(Player.ONE, Size.LARGE)

        undo_move_in_place(state, undo)

        assert state_snapshot(state) == original

    def test_multiple_moves_sequence(self):
        """Multiple apply/undo in sequence."""
        state = GameState()

        moves_and_undos = []

        # Apply several moves
        for i in range(5):
            moves = generate_moves(state)
            if not moves:
                break
            move = moves[0]
            result, undo = apply_move_in_place(state, move)
            moves_and_undos.append((move, undo, result))

        # Undo in reverse order
        original = GameState()
        while moves_and_undos:
            move, undo, _ = moves_and_undos.pop()
            undo_move_in_place(state, undo)

        assert state_snapshot(state) == state_snapshot(original)
