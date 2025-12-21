from __future__ import annotations

from copy import deepcopy
from typing import Iterator

from gobblet.types import STARTING_PIECES, Piece, Player, Position, Size


class GameState:
    """
    Represents the complete state of a Gobblet Gobblers game.

    The board is a 3x3 grid where each cell contains a stack of pieces.
    Stacks are represented as lists where index 0 is bottom, -1 is top (visible).
    """

    def __init__(self) -> None:
        # Board: 3x3 grid of stacks (each stack is a list of Pieces)
        self._board: list[list[list[Piece]]] = [[[] for _ in range(3)] for _ in range(3)]

        # Reserves: pieces not yet on the board, tracked by (player, size) -> count
        self._reserves: dict[tuple[Player, Size], int] = {}
        for player in Player:
            for size, count in STARTING_PIECES.items():
                self._reserves[(player, size)] = count

        # Current player to move
        self.current_player: Player = Player.ONE

        # Position history for threefold repetition (list of board hashes)
        self._position_history: list[int] = []

    def copy(self) -> GameState:
        """Create a deep copy of the game state."""
        new_state = GameState.__new__(GameState)
        new_state._board = deepcopy(self._board)
        new_state._reserves = self._reserves.copy()
        new_state.current_player = self.current_player
        new_state._position_history = self._position_history.copy()
        return new_state

    # --- Board access ---

    def get_stack(self, pos: Position) -> list[Piece]:
        """Get the stack of pieces at a position (bottom to top)."""
        row, col = pos
        return self._board[row][col]

    def get_top(self, pos: Position) -> Piece | None:
        """Get the top (visible) piece at a position, or None if empty."""
        stack = self.get_stack(pos)
        return stack[-1] if stack else None

    def is_empty(self, pos: Position) -> bool:
        """Check if a position has no pieces."""
        return len(self.get_stack(pos)) == 0

    # --- Reserve access ---

    def get_reserve(self, player: Player, size: Size) -> int:
        """Get the count of pieces of given size in player's reserve."""
        return self._reserves[(player, size)]

    def has_reserve(self, player: Player, size: Size) -> bool:
        """Check if player has at least one piece of given size in reserve."""
        return self._reserves[(player, size)] > 0

    def get_all_reserves(self, player: Player) -> dict[Size, int]:
        """Get all reserve counts for a player."""
        return {size: self._reserves[(player, size)] for size in Size}

    # --- Board modification ---

    def place_piece(self, piece: Piece, pos: Position) -> None:
        """Place a piece on top of the stack at position."""
        row, col = pos
        self._board[row][col].append(piece)

    def remove_top(self, pos: Position) -> Piece:
        """Remove and return the top piece from a position."""
        row, col = pos
        return self._board[row][col].pop()

    def use_reserve(self, player: Player, size: Size) -> None:
        """Decrement reserve count when placing from reserve."""
        self._reserves[(player, size)] -= 1

    # --- Position iteration ---

    @staticmethod
    def all_positions() -> Iterator[Position]:
        """Iterate over all board positions."""
        for row in range(3):
            for col in range(3):
                yield (row, col)

    # --- Win detection ---

    WINNING_LINES: list[list[Position]] = [
        # Rows
        [(0, 0), (0, 1), (0, 2)],
        [(1, 0), (1, 1), (1, 2)],
        [(2, 0), (2, 1), (2, 2)],
        # Columns
        [(0, 0), (1, 0), (2, 0)],
        [(0, 1), (1, 1), (2, 1)],
        [(0, 2), (1, 2), (2, 2)],
        # Diagonals
        [(0, 0), (1, 1), (2, 2)],
        [(0, 2), (1, 1), (2, 0)],
    ]

    def check_winner(self) -> Player | None:
        """
        Check if there's a winner (3 in a row of visible pieces).
        Returns the winning player or None.
        """
        for line in self.WINNING_LINES:
            pieces = [self.get_top(pos) for pos in line]
            if all(p is not None for p in pieces):
                players = [p.player for p in pieces]  # type: ignore[union-attr]
                if players[0] == players[1] == players[2]:
                    return players[0]
        return None

    def get_winning_lines(self, player: Player) -> list[list[Position]]:
        """Get all winning lines for a player (used for reveal rule checking)."""
        winning = []
        for line in self.WINNING_LINES:
            pieces = [self.get_top(pos) for pos in line]
            if all(p is not None and p.player == player for p in pieces):
                winning.append(line)
        return winning

    # --- Position hashing for repetition detection ---

    def board_hash(self) -> int:
        """
        Compute a hash of the current board position.
        Used for threefold repetition detection.
        """
        # Convert board to a hashable tuple structure
        board_tuple = tuple(
            tuple(tuple(piece for piece in stack) for stack in row) for row in self._board
        )
        return hash((board_tuple, self.current_player))

    def record_position(self) -> None:
        """Record current position in history."""
        self._position_history.append(self.board_hash())

    def is_threefold_repetition(self) -> bool:
        """Check if current position has occurred 3 times."""
        current_hash = self.board_hash()
        return self._position_history.count(current_hash) >= 3

    # --- Display ---

    def __repr__(self) -> str:
        """Text representation of the board state."""
        lines = []
        lines.append(f"Current player: {self.current_player.name}")
        lines.append("")

        # Show reserves
        for player in Player:
            reserves = self.get_all_reserves(player)
            reserve_str = ", ".join(f"{s.name[0]}:{c}" for s, c in reserves.items())
            lines.append(f"P{player.value} reserves: {reserve_str}")
        lines.append("")

        # Show board
        lines.append("  0   1   2")
        for row in range(3):
            row_str = f"{row} "
            for col in range(3):
                top = self.get_top((row, col))
                if top is None:
                    row_str += ".. "
                else:
                    row_str += f"{top!r} "
            lines.append(row_str)

        return "\n".join(lines)
