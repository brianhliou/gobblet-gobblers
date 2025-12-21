"""Tests for the Gobblet Gobblers game logic."""

import pytest

from gobblet import (
    Game,
    GameResult,
    GameState,
    Move,
    Piece,
    Player,
    Size,
    generate_moves,
)


class TestTypes:
    """Tests for basic types."""

    def test_player_opponent(self) -> None:
        assert Player.ONE.opponent() == Player.TWO
        assert Player.TWO.opponent() == Player.ONE

    def test_size_can_gobble(self) -> None:
        assert Size.LARGE.can_gobble(Size.MEDIUM)
        assert Size.LARGE.can_gobble(Size.SMALL)
        assert Size.MEDIUM.can_gobble(Size.SMALL)
        assert not Size.SMALL.can_gobble(Size.SMALL)
        assert not Size.SMALL.can_gobble(Size.MEDIUM)
        assert not Size.MEDIUM.can_gobble(Size.MEDIUM)
        assert not Size.MEDIUM.can_gobble(Size.LARGE)

    def test_piece_can_gobble(self) -> None:
        p1_large = Piece(Player.ONE, Size.LARGE)
        p2_small = Piece(Player.TWO, Size.SMALL)
        p1_small = Piece(Player.ONE, Size.SMALL)

        assert p1_large.can_gobble(p2_small)
        assert p1_large.can_gobble(p1_small)  # Can gobble own pieces
        assert not p1_small.can_gobble(p1_large)


class TestGameState:
    """Tests for game state."""

    def test_initial_state(self) -> None:
        state = GameState()
        assert state.current_player == Player.ONE

        # All positions empty
        for pos in state.all_positions():
            assert state.is_empty(pos)
            assert state.get_top(pos) is None

        # Reserves full
        for player in Player:
            for size in Size:
                assert state.get_reserve(player, size) == 2

    def test_place_and_get(self) -> None:
        state = GameState()
        piece = Piece(Player.ONE, Size.SMALL)
        pos = (0, 0)

        state.place_piece(piece, pos)
        assert state.get_top(pos) == piece
        assert not state.is_empty(pos)

    def test_stack_operations(self) -> None:
        state = GameState()
        pos = (1, 1)

        small = Piece(Player.ONE, Size.SMALL)
        large = Piece(Player.TWO, Size.LARGE)

        state.place_piece(small, pos)
        state.place_piece(large, pos)

        # Large is on top
        assert state.get_top(pos) == large

        # Stack contains both
        stack = state.get_stack(pos)
        assert len(stack) == 2
        assert stack[0] == small  # Bottom
        assert stack[1] == large  # Top

        # Remove top
        removed = state.remove_top(pos)
        assert removed == large
        assert state.get_top(pos) == small

    def test_reserve_operations(self) -> None:
        state = GameState()
        player = Player.ONE
        size = Size.SMALL

        assert state.has_reserve(player, size)
        assert state.get_reserve(player, size) == 2

        state.use_reserve(player, size)
        assert state.get_reserve(player, size) == 1

        state.use_reserve(player, size)
        assert state.get_reserve(player, size) == 0
        assert not state.has_reserve(player, size)

    def test_copy(self) -> None:
        state = GameState()
        state.place_piece(Piece(Player.ONE, Size.SMALL), (0, 0))
        state.current_player = Player.TWO

        copy = state.copy()

        # Modifications to copy don't affect original
        copy.place_piece(Piece(Player.TWO, Size.LARGE), (0, 0))
        assert state.get_top((0, 0)) == Piece(Player.ONE, Size.SMALL)


class TestWinDetection:
    """Tests for win detection."""

    def test_no_winner_empty_board(self) -> None:
        state = GameState()
        assert state.check_winner() is None

    def test_horizontal_win(self) -> None:
        state = GameState()
        for col in range(3):
            state.place_piece(Piece(Player.ONE, Size.SMALL), (0, col))

        assert state.check_winner() == Player.ONE

    def test_vertical_win(self) -> None:
        state = GameState()
        for row in range(3):
            state.place_piece(Piece(Player.TWO, Size.MEDIUM), (row, 1))

        assert state.check_winner() == Player.TWO

    def test_diagonal_win(self) -> None:
        state = GameState()
        for i in range(3):
            state.place_piece(Piece(Player.ONE, Size.LARGE), (i, i))

        assert state.check_winner() == Player.ONE

    def test_anti_diagonal_win(self) -> None:
        state = GameState()
        positions = [(0, 2), (1, 1), (2, 0)]
        for pos in positions:
            state.place_piece(Piece(Player.TWO, Size.SMALL), pos)

        assert state.check_winner() == Player.TWO

    def test_mixed_pieces_no_win(self) -> None:
        state = GameState()
        state.place_piece(Piece(Player.ONE, Size.SMALL), (0, 0))
        state.place_piece(Piece(Player.ONE, Size.SMALL), (0, 1))
        state.place_piece(Piece(Player.TWO, Size.SMALL), (0, 2))

        assert state.check_winner() is None

    def test_get_winning_lines(self) -> None:
        state = GameState()
        for col in range(3):
            state.place_piece(Piece(Player.ONE, Size.SMALL), (0, col))

        lines = state.get_winning_lines(Player.ONE)
        assert len(lines) == 1
        assert set(lines[0]) == {(0, 0), (0, 1), (0, 2)}

        assert state.get_winning_lines(Player.TWO) == []


class TestMoveGeneration:
    """Tests for move generation."""

    def test_initial_moves(self) -> None:
        state = GameState()
        moves = generate_moves(state)

        # 3 sizes * 9 positions = 27 moves for reserve placements
        # (all empty, no gobbling needed)
        assert len(moves) == 27

        # All moves should be from reserve
        for move in moves:
            assert move.is_from_reserve

    def test_gobbling_moves(self) -> None:
        state = GameState()
        state.place_piece(Piece(Player.TWO, Size.SMALL), (0, 0))

        moves = generate_moves(state)

        # Can place on (0,0) with medium or large (not small)
        gobble_moves = [m for m in moves if m.to_pos == (0, 0)]
        sizes = {m.size for m in gobble_moves}
        assert sizes == {Size.MEDIUM, Size.LARGE}

    def test_board_move_generation(self) -> None:
        state = GameState()
        state.place_piece(Piece(Player.ONE, Size.LARGE), (1, 1))

        moves = generate_moves(state)

        # Should have moves from board AND from reserve
        board_moves = [m for m in moves if not m.is_from_reserve]
        reserve_moves = [m for m in moves if m.is_from_reserve]

        # Large piece can move to any other square (8 destinations)
        assert len(board_moves) == 8
        assert len(reserve_moves) > 0


class TestRevealRule:
    """Tests for the reveal rule (Hail Mary)."""

    def test_reveal_blocks_move(self) -> None:
        """Test that revealing opponent's win limits move options."""
        state = GameState()

        # Set up opponent's near-win under our piece
        state.place_piece(Piece(Player.TWO, Size.SMALL), (0, 0))
        state.place_piece(Piece(Player.TWO, Size.SMALL), (0, 1))
        # P1 large on top of P2 small at (0,2) - lifting reveals P2 win potential
        state.place_piece(Piece(Player.TWO, Size.SMALL), (0, 2))
        state.place_piece(Piece(Player.ONE, Size.LARGE), (0, 2))

        moves = generate_moves(state)

        # Board moves from (0,2) should only target the winning line
        board_moves = [m for m in moves if m.from_pos == (0, 2)]

        # Can go to (0,0) or (0,1) - these are in the winning line
        # and P1's large can gobble the P2 smalls there.
        # CANNOT go to (0,2) - same square moves are not allowed!
        targets = {m.to_pos for m in board_moves}
        assert targets == {(0, 0), (0, 1)}

    def test_reveal_no_save_possible(self) -> None:
        """Test when reveal has no legal save (piece too small)."""
        state = GameState()

        # Set up opponent's near-win with large pieces
        state.place_piece(Piece(Player.TWO, Size.LARGE), (0, 0))
        state.place_piece(Piece(Player.TWO, Size.LARGE), (0, 1))
        # P1 medium on top of P2 large at (0,2) - can't gobble large with medium
        state.place_piece(Piece(Player.TWO, Size.LARGE), (0, 2))
        state.place_piece(Piece(Player.ONE, Size.MEDIUM), (0, 2))

        moves = generate_moves(state)

        # Board moves from (0,2) should be empty (can't save)
        board_moves = [m for m in moves if m.from_pos == (0, 2)]
        assert len(board_moves) == 0

    def test_no_same_square_moves_in_reveal(self) -> None:
        """Test that you cannot place a piece back on the same square in reveal situation.

        This is the critical edge case from official rules:
        "you can't return the piece to its starting location"
        """
        state = GameState()

        # Reproduce the bug scenario:
        # P1: S(0,0), P2: M(0,0), P1: M(1,0), P2: S(0,2), P1: L(2,0)
        # Now P2 has M at (0,0) covering P1's S
        # P1 has column 0: M at (1,0), L at (2,0)
        # If P2 lifts M from (0,0), P1's S is revealed -> P1 wins column 0

        # Set up the position directly
        state.place_piece(Piece(Player.ONE, Size.SMALL), (0, 0))  # P1 S underneath
        state.place_piece(Piece(Player.TWO, Size.MEDIUM), (0, 0))  # P2 M on top
        state.place_piece(Piece(Player.ONE, Size.MEDIUM), (1, 0))  # P1 M
        state.place_piece(Piece(Player.TWO, Size.SMALL), (0, 2))  # P2 S
        state.place_piece(Piece(Player.ONE, Size.LARGE), (2, 0))  # P1 L
        state.current_player = Player.TWO  # P2's turn

        moves = generate_moves(state)

        # P2's moves from (0,0) - lifting reveals P1's win in column 0
        # P2 could gobble at (0,0) but that's same square - NOT ALLOWED
        # P2 cannot gobble (1,0) M with M (same size)
        # P2 cannot gobble (2,0) L with M (L > M)
        # So NO legal moves from (0,0)
        board_moves_from_00 = [m for m in moves if m.from_pos == (0, 0)]
        assert len(board_moves_from_00) == 0

        # Verify that (0,0) -> (0,0) is specifically not in the moves
        same_square_move = Move(Player.TWO, to_pos=(0, 0), from_pos=(0, 0))
        assert same_square_move not in moves

    def test_same_square_not_allowed_even_when_valid_gobble(self) -> None:
        """Ensure same-square is excluded even if gobbling would be size-legal."""
        state = GameState()

        # P2 has near-win, P1's Large covers P2's Small
        state.place_piece(Piece(Player.TWO, Size.SMALL), (0, 0))
        state.place_piece(Piece(Player.TWO, Size.SMALL), (0, 1))
        state.place_piece(Piece(Player.TWO, Size.SMALL), (0, 2))  # P2 S underneath
        state.place_piece(Piece(Player.ONE, Size.LARGE), (0, 2))  # P1 L on top

        moves = generate_moves(state)

        # Lifting L from (0,2) reveals P2's row
        # L can gobble S at (0,0), (0,1) - these are valid
        # L can gobble S at (0,2) size-wise, but it's same square - NOT ALLOWED
        board_moves_from_02 = [m for m in moves if m.from_pos == (0, 2)]
        targets = {m.to_pos for m in board_moves_from_02}

        # Should only be (0,0) and (0,1), NOT (0,2)
        assert targets == {(0, 0), (0, 1)}


class TestGame:
    """Tests for full game flow."""

    def test_simple_game_win(self) -> None:
        game = Game()

        # P1 places three in a row (simplified, ignoring P2 moves)
        moves = [
            Move(Player.ONE, to_pos=(0, 0), size=Size.LARGE),
            Move(Player.TWO, to_pos=(1, 0), size=Size.LARGE),
            Move(Player.ONE, to_pos=(0, 1), size=Size.LARGE),
            Move(Player.TWO, to_pos=(1, 1), size=Size.LARGE),
            Move(Player.ONE, to_pos=(0, 2), size=Size.SMALL),
        ]

        for move in moves:
            result = game.apply_move(move)

        assert result.game_result == GameResult.PLAYER_ONE_WINS
        assert game.is_over()

    def test_reveal_loss(self) -> None:
        """Test that revealing opponent's win causes loss."""
        game = Game()
        state = game.state

        # Manually set up a reveal scenario
        state.place_piece(Piece(Player.TWO, Size.LARGE), (0, 0))
        state.place_piece(Piece(Player.TWO, Size.LARGE), (0, 1))
        state.place_piece(Piece(Player.TWO, Size.LARGE), (0, 2))
        # P1 small is covering P2's win
        state.place_piece(Piece(Player.ONE, Size.SMALL), (0, 2))

        # P1 tries to move small - can't save, should lose
        # But actually this move won't be in legal moves
        legal_moves = game.get_legal_moves()
        board_moves_from_02 = [m for m in legal_moves if m.from_pos == (0, 2)]
        assert len(board_moves_from_02) == 0

    def test_game_not_over_initially(self) -> None:
        game = Game()
        assert not game.is_over()
        assert game.result == GameResult.ONGOING


class TestThreefoldRepetition:
    """Tests for draw by repetition."""

    def test_position_hash_consistency(self) -> None:
        state = GameState()
        state.place_piece(Piece(Player.ONE, Size.SMALL), (0, 0))

        hash1 = state.board_hash()
        hash2 = state.board_hash()

        assert hash1 == hash2

    def test_different_positions_different_hash(self) -> None:
        state1 = GameState()
        state1.place_piece(Piece(Player.ONE, Size.SMALL), (0, 0))

        state2 = GameState()
        state2.place_piece(Piece(Player.ONE, Size.SMALL), (0, 1))

        assert state1.board_hash() != state2.board_hash()
