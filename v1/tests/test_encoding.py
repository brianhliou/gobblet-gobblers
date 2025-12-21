"""Tests for solver/encoding.py"""

import pytest

from gobblet.state import GameState
from gobblet.types import Piece, Player, Size
from solver.encoding import (
    base64_to_int,
    base64_to_state,
    canonicalize,
    decode_state,
    encode_state,
    get_all_symmetries,
    int_to_base64,
    state_to_base64,
    _rotate_90,
    _reflect_horizontal,
)


class TestBinaryEncoding:
    """Tests for encode_state and decode_state."""

    def test_empty_board_player_one(self):
        """Empty board with P1 to move encodes to 0."""
        state = GameState()
        encoded = encode_state(state)
        assert encoded == 0

    def test_empty_board_player_two(self):
        """Empty board with P2 to move has only bit 54 set."""
        state = GameState()
        state.current_player = Player.TWO
        encoded = encode_state(state)
        assert encoded == (1 << 54)

    def test_single_piece(self):
        """Single piece at (0,0) encodes correctly."""
        state = GameState()
        state._board[0][0].append(Piece(Player.ONE, Size.SMALL))
        state._reserves[(Player.ONE, Size.SMALL)] -= 1

        encoded = encode_state(state)
        # Small slot at cell 0 should be 1 (P1)
        assert (encoded & 0b11) == 1

    def test_roundtrip_empty(self):
        """Empty board survives encode/decode roundtrip."""
        state = GameState()
        encoded = encode_state(state)
        decoded = decode_state(encoded)

        assert decoded.current_player == state.current_player
        for row in range(3):
            for col in range(3):
                assert decoded.get_stack((row, col)) == []

    def test_roundtrip_with_pieces(self):
        """Board with pieces survives encode/decode roundtrip."""
        state = GameState()

        # Place some pieces
        state._board[0][0].append(Piece(Player.ONE, Size.SMALL))
        state._reserves[(Player.ONE, Size.SMALL)] -= 1

        state._board[0][0].append(Piece(Player.TWO, Size.LARGE))
        state._reserves[(Player.TWO, Size.LARGE)] -= 1

        state._board[1][1].append(Piece(Player.TWO, Size.MEDIUM))
        state._reserves[(Player.TWO, Size.MEDIUM)] -= 1

        state._board[2][2].append(Piece(Player.ONE, Size.LARGE))
        state._reserves[(Player.ONE, Size.LARGE)] -= 1

        state.current_player = Player.TWO

        # Encode and decode
        encoded = encode_state(state)
        decoded = decode_state(encoded)

        # Verify current player
        assert decoded.current_player == Player.TWO

        # Verify board state
        stack_00 = decoded.get_stack((0, 0))
        assert len(stack_00) == 2
        assert stack_00[0] == Piece(Player.ONE, Size.SMALL)
        assert stack_00[1] == Piece(Player.TWO, Size.LARGE)

        stack_11 = decoded.get_stack((1, 1))
        assert len(stack_11) == 1
        assert stack_11[0] == Piece(Player.TWO, Size.MEDIUM)

        stack_22 = decoded.get_stack((2, 2))
        assert len(stack_22) == 1
        assert stack_22[0] == Piece(Player.ONE, Size.LARGE)

        # Verify reserves
        assert decoded.get_reserve(Player.ONE, Size.SMALL) == 1
        assert decoded.get_reserve(Player.TWO, Size.LARGE) == 1
        assert decoded.get_reserve(Player.TWO, Size.MEDIUM) == 1
        assert decoded.get_reserve(Player.ONE, Size.LARGE) == 1

    def test_full_stack(self):
        """Cell with all three sizes encodes correctly."""
        state = GameState()

        # Full stack: P1 small, P2 medium, P1 large
        state._board[0][0].append(Piece(Player.ONE, Size.SMALL))
        state._board[0][0].append(Piece(Player.TWO, Size.MEDIUM))
        state._board[0][0].append(Piece(Player.ONE, Size.LARGE))
        state._reserves[(Player.ONE, Size.SMALL)] -= 1
        state._reserves[(Player.TWO, Size.MEDIUM)] -= 1
        state._reserves[(Player.ONE, Size.LARGE)] -= 1

        encoded = encode_state(state)
        decoded = decode_state(encoded)

        stack = decoded.get_stack((0, 0))
        assert len(stack) == 3
        assert stack[0] == Piece(Player.ONE, Size.SMALL)
        assert stack[1] == Piece(Player.TWO, Size.MEDIUM)
        assert stack[2] == Piece(Player.ONE, Size.LARGE)


class TestBase64Encoding:
    """Tests for base64 conversion."""

    def test_roundtrip_zero(self):
        """Zero survives base64 roundtrip."""
        assert base64_to_int(int_to_base64(0)) == 0

    def test_roundtrip_large(self):
        """Large number survives base64 roundtrip."""
        n = (1 << 54) | 0x123456789ABC
        assert base64_to_int(int_to_base64(n)) == n

    def test_base64_length(self):
        """Base64 encoding produces reasonable length."""
        n = (1 << 54) | 0xFFFFFFFFFFFF
        b64 = int_to_base64(n)
        # 8 bytes -> 11 chars (without padding)
        assert len(b64) <= 12

    def test_state_to_base64_roundtrip(self):
        """GameState survives base64 roundtrip."""
        state = GameState()
        state._board[1][1].append(Piece(Player.ONE, Size.LARGE))
        state._reserves[(Player.ONE, Size.LARGE)] -= 1
        state.current_player = Player.TWO

        b64 = state_to_base64(state)
        decoded = base64_to_state(b64)

        assert decoded.current_player == Player.TWO
        assert decoded.get_top((1, 1)) == Piece(Player.ONE, Size.LARGE)


class TestSymmetryTransforms:
    """Tests for rotation and reflection."""

    def test_rotate_90_single_piece(self):
        """90° rotation moves piece correctly."""
        state = GameState()
        # Place at (0,0)
        state._board[0][0].append(Piece(Player.ONE, Size.SMALL))
        state._reserves[(Player.ONE, Size.SMALL)] -= 1

        encoded = encode_state(state)
        rotated = _rotate_90(encoded)
        decoded = decode_state(rotated)

        # (0,0) -> (0,2) after 90° clockwise
        assert decoded.get_top((0, 0)) is None
        assert decoded.get_top((0, 2)) == Piece(Player.ONE, Size.SMALL)

    def test_rotate_90_corner_piece(self):
        """90° rotation of corner piece."""
        state = GameState()
        # Place at (2,0) - bottom left
        state._board[2][0].append(Piece(Player.TWO, Size.LARGE))
        state._reserves[(Player.TWO, Size.LARGE)] -= 1

        encoded = encode_state(state)
        rotated = _rotate_90(encoded)
        decoded = decode_state(rotated)

        # (2,0) -> (0,0) after 90° clockwise
        assert decoded.get_top((0, 0)) == Piece(Player.TWO, Size.LARGE)

    def test_rotate_360_identity(self):
        """Four 90° rotations return to original."""
        state = GameState()
        state._board[0][1].append(Piece(Player.ONE, Size.MEDIUM))
        state._reserves[(Player.ONE, Size.MEDIUM)] -= 1

        encoded = encode_state(state)
        rotated = encoded
        for _ in range(4):
            rotated = _rotate_90(rotated)

        assert rotated == encoded

    def test_reflect_horizontal(self):
        """Horizontal reflection works correctly."""
        state = GameState()
        # Place at (0,0) - left side
        state._board[0][0].append(Piece(Player.ONE, Size.SMALL))
        state._reserves[(Player.ONE, Size.SMALL)] -= 1

        encoded = encode_state(state)
        reflected = _reflect_horizontal(encoded)
        decoded = decode_state(reflected)

        # (0,0) -> (0,2) after horizontal flip
        assert decoded.get_top((0, 0)) is None
        assert decoded.get_top((0, 2)) == Piece(Player.ONE, Size.SMALL)

    def test_reflect_twice_identity(self):
        """Two horizontal reflections return to original."""
        state = GameState()
        state._board[1][0].append(Piece(Player.TWO, Size.MEDIUM))
        state._reserves[(Player.TWO, Size.MEDIUM)] -= 1

        encoded = encode_state(state)
        reflected = _reflect_horizontal(_reflect_horizontal(encoded))

        assert reflected == encoded

    def test_center_invariant(self):
        """Center piece (1,1) unchanged by rotation."""
        state = GameState()
        state._board[1][1].append(Piece(Player.ONE, Size.LARGE))
        state._reserves[(Player.ONE, Size.LARGE)] -= 1

        encoded = encode_state(state)

        # All rotations should keep piece at center
        rotated = _rotate_90(encoded)
        decoded = decode_state(rotated)
        assert decoded.get_top((1, 1)) == Piece(Player.ONE, Size.LARGE)


class TestCanonicalization:
    """Tests for canonicalize and get_all_symmetries."""

    def test_all_symmetries_count(self):
        """get_all_symmetries returns 8 variants."""
        state = GameState()
        state._board[0][0].append(Piece(Player.ONE, Size.SMALL))
        state._reserves[(Player.ONE, Size.SMALL)] -= 1

        encoded = encode_state(state)
        symmetries = get_all_symmetries(encoded)

        assert len(symmetries) == 8

    def test_symmetric_positions_same_canonical(self):
        """Symmetric positions produce same canonical form."""
        # Create state with piece at (0,0)
        state1 = GameState()
        state1._board[0][0].append(Piece(Player.ONE, Size.SMALL))
        state1._reserves[(Player.ONE, Size.SMALL)] -= 1

        # Create state with piece at (0,2) - 90° rotated
        state2 = GameState()
        state2._board[0][2].append(Piece(Player.ONE, Size.SMALL))
        state2._reserves[(Player.ONE, Size.SMALL)] -= 1

        # Create state with piece at (2,2) - 180° rotated
        state3 = GameState()
        state3._board[2][2].append(Piece(Player.ONE, Size.SMALL))
        state3._reserves[(Player.ONE, Size.SMALL)] -= 1

        canonical1 = canonicalize(encode_state(state1))
        canonical2 = canonicalize(encode_state(state2))
        canonical3 = canonicalize(encode_state(state3))

        assert canonical1 == canonical2 == canonical3

    def test_different_positions_different_canonical(self):
        """Non-symmetric positions have different canonical forms."""
        # Piece at (0,0)
        state1 = GameState()
        state1._board[0][0].append(Piece(Player.ONE, Size.SMALL))
        state1._reserves[(Player.ONE, Size.SMALL)] -= 1

        # Piece at (0,1) - not symmetric to (0,0)
        state2 = GameState()
        state2._board[0][1].append(Piece(Player.ONE, Size.SMALL))
        state2._reserves[(Player.ONE, Size.SMALL)] -= 1

        canonical1 = canonicalize(encode_state(state1))
        canonical2 = canonicalize(encode_state(state2))

        assert canonical1 != canonical2

    def test_canonical_is_minimum(self):
        """Canonical form is the minimum of all symmetries."""
        state = GameState()
        state._board[0][0].append(Piece(Player.ONE, Size.SMALL))
        state._reserves[(Player.ONE, Size.SMALL)] -= 1

        encoded = encode_state(state)
        symmetries = get_all_symmetries(encoded)
        canonical = canonicalize(encoded)

        assert canonical == min(symmetries)

    def test_canonical_idempotent(self):
        """Canonicalizing a canonical form returns the same value."""
        state = GameState()
        state._board[1][2].append(Piece(Player.TWO, Size.MEDIUM))
        state._reserves[(Player.TWO, Size.MEDIUM)] -= 1

        encoded = encode_state(state)
        canonical1 = canonicalize(encoded)
        canonical2 = canonicalize(canonical1)

        assert canonical1 == canonical2
