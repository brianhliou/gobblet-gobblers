"""Tests for solver/minimax.py

These tests focus on verifying the solver logic without running full game tree
exploration, which would take too long.
"""

import pytest

from gobblet.game import GameResult, play_move
from gobblet.moves import generate_moves
from gobblet.state import GameState
from gobblet.types import Piece, Player, Size
from solver.encoding import canonicalize, encode_state
from solver.minimax import Outcome, Solver


class TestOutcomeEnum:
    """Test the Outcome enum properties."""

    def test_outcome_ordering(self):
        """P1 wins > Draw > P2 wins for minimax comparison."""
        assert Outcome.WIN_P1 > Outcome.DRAW > Outcome.WIN_P2

    def test_outcome_values(self):
        """Outcome values are -1, 0, 1 as expected."""
        assert Outcome.WIN_P2 == -1
        assert Outcome.DRAW == 0
        assert Outcome.WIN_P1 == 1


class TestGameResultConversion:
    """Test conversion from GameResult to Outcome."""

    def test_convert_p1_wins(self):
        solver = Solver()
        assert solver._game_result_to_outcome(GameResult.PLAYER_ONE_WINS) == Outcome.WIN_P1

    def test_convert_p2_wins(self):
        solver = Solver()
        assert solver._game_result_to_outcome(GameResult.PLAYER_TWO_WINS) == Outcome.WIN_P2

    def test_convert_draw(self):
        solver = Solver()
        assert solver._game_result_to_outcome(GameResult.DRAW) == Outcome.DRAW


class TestWinDetection:
    """Test that GameState correctly detects wins."""

    def test_p1_already_won(self):
        """P1 has three in a row - should be detected."""
        state = GameState()
        state._board[0][0].append(Piece(Player.ONE, Size.SMALL))
        state._board[0][1].append(Piece(Player.ONE, Size.MEDIUM))
        state._board[0][2].append(Piece(Player.ONE, Size.LARGE))
        state._reserves[(Player.ONE, Size.SMALL)] -= 1
        state._reserves[(Player.ONE, Size.MEDIUM)] -= 1
        state._reserves[(Player.ONE, Size.LARGE)] -= 1

        assert state.check_winner() == Player.ONE

    def test_p2_already_won(self):
        """P2 has three in a column - should be detected."""
        state = GameState()
        state._board[0][0].append(Piece(Player.TWO, Size.SMALL))
        state._board[1][0].append(Piece(Player.TWO, Size.MEDIUM))
        state._board[2][0].append(Piece(Player.TWO, Size.LARGE))
        state._reserves[(Player.TWO, Size.SMALL)] -= 1
        state._reserves[(Player.TWO, Size.MEDIUM)] -= 1
        state._reserves[(Player.TWO, Size.LARGE)] -= 1

        assert state.check_winner() == Player.TWO


class TestImmediateWinFromMove:
    """Test that a move creating 3-in-a-row is correctly detected by play_move."""

    def test_move_creates_p1_win(self):
        """Making a winning move should return P1 wins."""
        state = GameState()
        # P1 has two in a row
        state._board[0][0].append(Piece(Player.ONE, Size.SMALL))
        state._board[0][1].append(Piece(Player.ONE, Size.MEDIUM))
        state._reserves[(Player.ONE, Size.SMALL)] -= 1
        state._reserves[(Player.ONE, Size.MEDIUM)] -= 1
        state.current_player = Player.ONE

        # Find a move to (0,2) that completes the row
        winning_move = None
        for move in generate_moves(state):
            if move.to_pos == (0, 2):
                winning_move = move
                break

        assert winning_move is not None

        new_state, result = play_move(state, winning_move)
        assert result == GameResult.PLAYER_ONE_WINS


class TestSolverTableOperations:
    """Test the transposition table lookup/store operations."""

    def test_get_outcome_returns_stored_value(self):
        """get_outcome returns what's in the table."""
        state = GameState()
        solver = Solver()

        canonical = canonicalize(encode_state(state))
        solver.table[canonical] = Outcome.DRAW

        assert solver.get_outcome(state) == Outcome.DRAW

    def test_get_outcome_returns_none_if_missing(self):
        """get_outcome returns None if not solved."""
        state = GameState()
        solver = Solver()

        assert solver.get_outcome(state) is None

    def test_symmetric_states_share_outcome(self):
        """Two symmetric positions should map to the same canonical key."""
        # Position with P1 piece at (0,0)
        state1 = GameState()
        state1._board[0][0].append(Piece(Player.ONE, Size.LARGE))
        state1._reserves[(Player.ONE, Size.LARGE)] -= 1

        # Position with P1 piece at (0,2) - rotation of (0,0)
        state2 = GameState()
        state2._board[0][2].append(Piece(Player.ONE, Size.LARGE))
        state2._reserves[(Player.ONE, Size.LARGE)] -= 1

        # They should have the same canonical encoding
        c1 = canonicalize(encode_state(state1))
        c2 = canonicalize(encode_state(state2))
        assert c1 == c2

        # So if we store for one, we get for the other
        solver = Solver()
        solver.table[c1] = Outcome.WIN_P1

        assert solver.get_outcome(state1) == Outcome.WIN_P1
        assert solver.get_outcome(state2) == Outcome.WIN_P1


# Note: Full tree solving tests are intentionally omitted as they take too long.
# The solver logic is verified through unit tests above.
# Full solving will be tested via the CLI with checkpointing.


class TestGetBestMove:
    """Test get_best_move after manually populating table."""

    def test_best_move_chooses_win(self):
        """get_best_move returns a winning move when one exists."""
        state = GameState()
        state._board[0][0].append(Piece(Player.ONE, Size.SMALL))
        state._board[0][1].append(Piece(Player.ONE, Size.MEDIUM))
        state._reserves[(Player.ONE, Size.SMALL)] -= 1
        state._reserves[(Player.ONE, Size.MEDIUM)] -= 1
        state.current_player = Player.ONE

        solver = Solver()

        # Manually populate table: winning moves go to WIN_P1 positions
        for move in generate_moves(state):
            child_state, result = play_move(state, move)
            if result == GameResult.PLAYER_ONE_WINS:
                child_canonical = canonicalize(encode_state(child_state))
                solver.table[child_canonical] = Outcome.WIN_P1
            else:
                child_canonical = canonicalize(encode_state(child_state))
                solver.table[child_canonical] = Outcome.WIN_P2  # Assume others lose

        # Now get_best_move should pick a winning move
        result = solver.get_best_move(state)
        assert result is not None
        move, outcome = result
        assert outcome == Outcome.WIN_P1
        assert move.to_pos == (0, 2)  # The winning square


class TestGetAllMoveOutcomes:
    """Test get_all_move_outcomes."""

    def test_returns_all_moves(self):
        """Returns an entry for every legal move."""
        state = GameState()
        state.current_player = Player.ONE

        solver = Solver()
        # Don't solve - just check structure

        outcomes = solver.get_all_move_outcomes(state)
        moves = generate_moves(state)

        assert len(outcomes) == len(moves)

    def test_returns_none_for_unsolved(self):
        """Returns None outcome for positions not in table."""
        state = GameState()
        state.current_player = Player.ONE

        solver = Solver()
        outcomes = solver.get_all_move_outcomes(state)

        # Most outcomes should be None (not solved)
        # But immediate wins will have outcomes
        none_count = sum(1 for _, o in outcomes if o is None)
        assert none_count > 0  # At least some unsolved
