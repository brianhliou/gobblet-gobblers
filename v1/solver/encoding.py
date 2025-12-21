"""
Binary encoding for Gobblet Gobblers game states.

Encoding scheme (55 bits total, fits in 64-bit integer):
- 9 cells × 6 bits = 54 bits
- Each cell: 2 bits per slot (small, medium, large)
- Slot values: 0=empty, 1=P1, 2=P2
- Bit 54: current player (0=P1, 1=P2)

Bit layout (LSB first):
- Bits 0-5: Cell (0,0) [small:2, medium:2, large:2]
- Bits 6-11: Cell (0,1)
- ...
- Bits 48-53: Cell (2,2)
- Bit 54: Current player
"""

from __future__ import annotations

import base64
from typing import TYPE_CHECKING

from gobblet.types import Piece, Player, Size

if TYPE_CHECKING:
    from gobblet.state import GameState


def encode_state(state: GameState) -> int:
    """
    Encode a GameState into a 64-bit integer.

    Returns an integer where:
    - Bits 0-53: Board state (9 cells × 6 bits)
    - Bit 54: Current player (0=P1, 1=P2)
    """
    encoded = 0
    bit_pos = 0

    # Encode each cell (row-major order)
    for row in range(3):
        for col in range(3):
            stack = state.get_stack((row, col))

            # Build a lookup of what's at each size level
            small_owner = 0  # 0=empty, 1=P1, 2=P2
            medium_owner = 0
            large_owner = 0

            for piece in stack:
                if piece.size == Size.SMALL:
                    small_owner = piece.player.value
                elif piece.size == Size.MEDIUM:
                    medium_owner = piece.player.value
                elif piece.size == Size.LARGE:
                    large_owner = piece.player.value

            # Encode cell: 6 bits (2 per slot)
            cell_bits = small_owner | (medium_owner << 2) | (large_owner << 4)
            encoded |= cell_bits << bit_pos
            bit_pos += 6

    # Encode current player at bit 54
    player_bit = 0 if state.current_player == Player.ONE else 1
    encoded |= player_bit << 54

    return encoded


def decode_state(encoded: int) -> GameState:
    """
    Decode a 64-bit integer back into a GameState.

    Note: Position history is NOT restored (not part of encoding).
    """
    from gobblet.state import GameState
    from gobblet.types import STARTING_PIECES

    state = GameState()

    # Clear the board (already empty from __init__)
    # Reset reserves to starting values (already done in __init__)

    bit_pos = 0

    # Decode each cell
    for row in range(3):
        for col in range(3):
            cell_bits = (encoded >> bit_pos) & 0b111111  # 6 bits
            bit_pos += 6

            small_owner = cell_bits & 0b11
            medium_owner = (cell_bits >> 2) & 0b11
            large_owner = (cell_bits >> 4) & 0b11

            # Place pieces in correct order (small first, then medium, then large)
            # This maintains the stack invariant (larger on top)
            if small_owner != 0:
                player = Player(small_owner)
                piece = Piece(player, Size.SMALL)
                state._board[row][col].append(piece)
                state._reserves[(player, Size.SMALL)] -= 1

            if medium_owner != 0:
                player = Player(medium_owner)
                piece = Piece(player, Size.MEDIUM)
                state._board[row][col].append(piece)
                state._reserves[(player, Size.MEDIUM)] -= 1

            if large_owner != 0:
                player = Player(large_owner)
                piece = Piece(player, Size.LARGE)
                state._board[row][col].append(piece)
                state._reserves[(player, Size.LARGE)] -= 1

    # Decode current player from bit 54
    player_bit = (encoded >> 54) & 1
    state.current_player = Player.ONE if player_bit == 0 else Player.TWO

    return state


def int_to_base64(n: int) -> str:
    """
    Convert a 64-bit integer to a compact base64 string.

    Returns a string of ~11 characters (without padding).
    """
    # Convert to 8 bytes (big-endian)
    raw_bytes = n.to_bytes(8, byteorder='big')
    # Base64 encode and strip padding
    return base64.b64encode(raw_bytes).decode('ascii').rstrip('=')


def base64_to_int(s: str) -> int:
    """
    Convert a base64 string back to a 64-bit integer.

    Handles strings with or without padding.
    """
    # Add padding if needed (base64 requires length multiple of 4)
    padded = s + '=' * (-len(s) % 4)
    raw_bytes = base64.b64decode(padded)
    return int.from_bytes(raw_bytes, byteorder='big')


def state_to_base64(state: GameState) -> str:
    """Encode a GameState directly to base64 string."""
    return int_to_base64(encode_state(state))


def base64_to_state(s: str) -> GameState:
    """Decode a base64 string directly to GameState."""
    return decode_state(base64_to_int(s))


# --- D₄ Symmetry Transforms ---

def _rotate_90(encoded: int) -> int:
    """
    Rotate the board 90° clockwise.

    Position mapping:
    (0,0) -> (0,2)    (0,1) -> (1,2)    (0,2) -> (2,2)
    (1,0) -> (0,1)    (1,1) -> (1,1)    (1,2) -> (2,1)
    (2,0) -> (0,0)    (2,1) -> (1,0)    (2,2) -> (2,0)

    General: (r, c) -> (c, 2-r)
    """
    # Extract current player bit
    player_bit = (encoded >> 54) & 1

    # Build new encoding
    new_encoded = 0

    for old_row in range(3):
        for old_col in range(3):
            old_idx = old_row * 3 + old_col
            old_cell = (encoded >> (old_idx * 6)) & 0b111111

            # New position after 90° rotation
            new_row = old_col
            new_col = 2 - old_row
            new_idx = new_row * 3 + new_col

            new_encoded |= old_cell << (new_idx * 6)

    # Restore player bit
    new_encoded |= player_bit << 54

    return new_encoded


def _reflect_horizontal(encoded: int) -> int:
    """
    Reflect the board horizontally (flip left-right).

    Position mapping: (r, c) -> (r, 2-c)
    """
    player_bit = (encoded >> 54) & 1
    new_encoded = 0

    for row in range(3):
        for col in range(3):
            old_idx = row * 3 + col
            old_cell = (encoded >> (old_idx * 6)) & 0b111111

            new_col = 2 - col
            new_idx = row * 3 + new_col

            new_encoded |= old_cell << (new_idx * 6)

    new_encoded |= player_bit << 54
    return new_encoded


def get_all_symmetries(encoded: int) -> list[int]:
    """
    Generate all 8 symmetric variants of a position (D₄ group).

    Returns list of 8 encoded states:
    - 4 rotations (0°, 90°, 180°, 270°)
    - 4 reflections (horizontal flip of each rotation)
    """
    symmetries = []

    current = encoded
    for _ in range(4):
        symmetries.append(current)
        symmetries.append(_reflect_horizontal(current))
        current = _rotate_90(current)

    return symmetries


def canonicalize(encoded: int) -> int:
    """
    Return the canonical form of an encoded state.

    The canonical form is the lexicographically smallest of all 8
    symmetric variants. This ensures that symmetric positions map
    to the same canonical key.
    """
    return min(get_all_symmetries(encoded))


def canonicalize_state(state: GameState) -> int:
    """Encode a GameState and return its canonical form."""
    return canonicalize(encode_state(state))
