from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING

from gobblet.types import Piece, Player, Position, Size

if TYPE_CHECKING:
    from gobblet.state import GameState


@dataclass(frozen=True)
class Move:
    """
    Represents a move in the game.

    A move is either:
    - Place from reserve: from_pos is None, piece specifies size
    - Move on board: from_pos is the source position

    In both cases, to_pos is the destination.
    """

    player: Player
    to_pos: Position
    from_pos: Position | None = None  # None means place from reserve
    size: Size | None = None  # Only used for reserve placement

    @property
    def is_from_reserve(self) -> bool:
        """True if this move places a piece from reserve."""
        return self.from_pos is None

    def get_piece(self, state: GameState) -> Piece:
        """Get the piece being moved."""
        if self.is_from_reserve:
            assert self.size is not None
            return Piece(self.player, self.size)
        else:
            assert self.from_pos is not None
            top = state.get_top(self.from_pos)
            assert top is not None
            return top

    def __repr__(self) -> str:
        if self.is_from_reserve:
            size_char = {Size.SMALL: "S", Size.MEDIUM: "M", Size.LARGE: "L"}[self.size]  # type: ignore[index]
            return f"Place {size_char} -> {self.to_pos}"
        else:
            return f"Move {self.from_pos} -> {self.to_pos}"


def can_place_at(piece: Piece, pos: Position, state: GameState) -> bool:
    """Check if a piece can be placed at a position (gobbling rules)."""
    top = state.get_top(pos)
    if top is None:
        return True  # Empty square
    return piece.can_gobble(top)


def generate_basic_moves(state: GameState) -> list[Move]:
    """
    Generate all basic legal moves without considering the reveal rule.

    Returns moves from reserve and moves from board positions.
    """
    moves: list[Move] = []
    player = state.current_player

    # Moves from reserve
    for size in Size:
        if state.has_reserve(player, size):
            piece = Piece(player, size)
            for pos in state.all_positions():
                if can_place_at(piece, pos, state):
                    moves.append(Move(player=player, to_pos=pos, size=size))

    # Moves from board (moving visible pieces owned by current player)
    for from_pos in state.all_positions():
        top = state.get_top(from_pos)
        if top is not None and top.player == player:
            for to_pos in state.all_positions():
                if from_pos != to_pos and can_place_at(top, to_pos, state):
                    moves.append(Move(player=player, to_pos=to_pos, from_pos=from_pos))

    return moves


def generate_moves_with_reveal_check(state: GameState) -> list[Move]:
    """
    Generate all legal moves, accounting for the reveal rule.

    When lifting a piece reveals opponent's winning line:
    - Only moves that gobble into that line are legal
    - If no such moves exist, there are no legal moves (player loses)

    Returns list of legal moves.
    """
    moves: list[Move] = []
    player = state.current_player
    opponent = player.opponent()

    # Reserve moves are always safe (no reveal)
    for size in Size:
        if state.has_reserve(player, size):
            piece = Piece(player, size)
            for pos in state.all_positions():
                if can_place_at(piece, pos, state):
                    moves.append(Move(player=player, to_pos=pos, size=size))

    # Board moves need reveal checking
    for from_pos in state.all_positions():
        top = state.get_top(from_pos)
        if top is None or top.player != player:
            continue

        # Simulate lifting the piece
        state_after_lift = state.copy()
        state_after_lift.remove_top(from_pos)

        # Check if opponent wins after lift
        opponent_winning_lines = state_after_lift.get_winning_lines(opponent)

        if opponent_winning_lines:
            # Reveal rule: can only place on squares in the winning line(s)
            # where we can legally gobble.
            # IMPORTANT: Cannot place back on the same square (no same-square moves).
            valid_targets: set[Position] = set()
            for line in opponent_winning_lines:
                for pos in line:
                    if pos == from_pos:
                        continue  # Cannot return piece to starting square
                    target_top = state_after_lift.get_top(pos)
                    if target_top is not None and top.can_gobble(target_top):
                        valid_targets.add(pos)

            for to_pos in valid_targets:
                moves.append(Move(player=player, to_pos=to_pos, from_pos=from_pos))
        else:
            # No reveal issue, normal move generation
            for to_pos in state.all_positions():
                if from_pos != to_pos and can_place_at(top, to_pos, state):
                    moves.append(Move(player=player, to_pos=to_pos, from_pos=from_pos))

    return moves


def generate_moves(state: GameState, check_reveal: bool = True) -> list[Move]:
    """
    Generate all legal moves from the current state.

    Args:
        state: Current game state
        check_reveal: If True, apply the reveal rule. Set to False for
                      basic move generation (useful for testing).
    """
    if check_reveal:
        return generate_moves_with_reveal_check(state)
    else:
        return generate_basic_moves(state)


def move_to_notation(move: Move) -> str:
    """
    Convert a move to coordinate-based notation.

    Reserve placement: L(1,1), S(0,2), M(2,1)
    Board move: (0,0)→(2,2), (1,1)→(0,0)
    """
    if move.is_from_reserve:
        size_char = {Size.SMALL: "S", Size.MEDIUM: "M", Size.LARGE: "L"}[move.size]  # type: ignore[index]
        return f"{size_char}({move.to_pos[0]},{move.to_pos[1]})"
    else:
        assert move.from_pos is not None
        return f"({move.from_pos[0]},{move.from_pos[1]})→({move.to_pos[0]},{move.to_pos[1]})"


def notation_to_move(notation: str, player: Player) -> Move:
    """
    Parse a notation string back into a Move.

    Reserve placement: S(0,0), M(1,1), L(2,2)
    Board move: (0,0)→(1,1)

    Raises ValueError if notation is invalid.
    """
    import re

    notation = notation.strip()

    # Reserve placement: S(r,c), M(r,c), L(r,c)
    reserve_match = re.match(r"^([SML])\((\d),(\d)\)$", notation)
    if reserve_match:
        size_char = reserve_match.group(1)
        row = int(reserve_match.group(2))
        col = int(reserve_match.group(3))

        size_map = {"S": Size.SMALL, "M": Size.MEDIUM, "L": Size.LARGE}
        size = size_map[size_char]

        if not (0 <= row <= 2 and 0 <= col <= 2):
            raise ValueError(f"Invalid position in notation: {notation}")

        return Move(player=player, to_pos=(row, col), size=size)

    # Board move: (r1,c1)→(r2,c2)
    board_match = re.match(r"^\((\d),(\d)\)→\((\d),(\d)\)$", notation)
    if board_match:
        from_row = int(board_match.group(1))
        from_col = int(board_match.group(2))
        to_row = int(board_match.group(3))
        to_col = int(board_match.group(4))

        if not (0 <= from_row <= 2 and 0 <= from_col <= 2):
            raise ValueError(f"Invalid from position in notation: {notation}")
        if not (0 <= to_row <= 2 and 0 <= to_col <= 2):
            raise ValueError(f"Invalid to position in notation: {notation}")

        return Move(player=player, to_pos=(to_row, to_col), from_pos=(from_row, from_col))

    raise ValueError(f"Invalid move notation: {notation}")
