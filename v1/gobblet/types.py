from enum import Enum
from typing import NamedTuple


class Player(Enum):
    """The two players in the game."""

    ONE = 1
    TWO = 2

    def opponent(self) -> "Player":
        """Return the opposing player."""
        return Player.TWO if self == Player.ONE else Player.ONE


class Size(Enum):
    """Piece sizes, ordered small to large."""

    SMALL = 1
    MEDIUM = 2
    LARGE = 3

    def can_gobble(self, other: "Size") -> bool:
        """Return True if this size can gobble (cover) the other size."""
        return self.value > other.value


class Piece(NamedTuple):
    """A game piece with an owner and size."""

    player: Player
    size: Size

    def can_gobble(self, other: "Piece") -> bool:
        """Return True if this piece can gobble the other piece."""
        return self.size.can_gobble(other.size)

    def __repr__(self) -> str:
        p = "1" if self.player == Player.ONE else "2"
        s = {Size.SMALL: "S", Size.MEDIUM: "M", Size.LARGE: "L"}[self.size]
        return f"{p}{s}"


# Board coordinates
Position = tuple[int, int]  # (row, col), 0-indexed

# Standard starting pieces for each player
STARTING_PIECES: dict[Size, int] = {
    Size.SMALL: 2,
    Size.MEDIUM: 2,
    Size.LARGE: 2,
}
