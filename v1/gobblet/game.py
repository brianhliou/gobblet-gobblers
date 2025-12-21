from __future__ import annotations

from dataclasses import dataclass
from enum import Enum

from gobblet.moves import Move, generate_moves
from gobblet.state import GameState
from gobblet.types import Piece, Player


class GameResult(Enum):
    """Possible game outcomes."""

    ONGOING = "ongoing"
    PLAYER_ONE_WINS = "player_one_wins"
    PLAYER_TWO_WINS = "player_two_wins"
    DRAW = "draw"

    @staticmethod
    def winner(player: Player) -> "GameResult":
        """Get the result for a player winning."""
        return (
            GameResult.PLAYER_ONE_WINS if player == Player.ONE else GameResult.PLAYER_TWO_WINS
        )


@dataclass
class MoveResult:
    """Result of applying a move."""

    new_state: GameState
    game_result: GameResult
    revealed_loss: bool = False  # True if player lost due to reveal rule


class Game:
    """
    Manages a game of Gobblet Gobblers.

    Handles move application, game flow, and result determination.
    """

    def __init__(self, state: GameState | None = None) -> None:
        self.state = state if state is not None else GameState()
        self.result = GameResult.ONGOING
        self.move_history: list[Move] = []

    def get_legal_moves(self) -> list[Move]:
        """Get all legal moves for the current player."""
        if self.result != GameResult.ONGOING:
            return []
        return generate_moves(self.state)

    def apply_move(self, move: Move) -> MoveResult:
        """
        Apply a move to the game state.

        Returns the result including the new state and game outcome.
        """
        if self.result != GameResult.ONGOING:
            raise ValueError("Game is already over")

        new_state = self.state.copy()
        player = new_state.current_player
        opponent = player.opponent()

        # Handle the move based on type
        if move.is_from_reserve:
            # Place from reserve
            assert move.size is not None
            piece = Piece(player, move.size)
            new_state.use_reserve(player, move.size)
            new_state.place_piece(piece, move.to_pos)
        else:
            # Move from board
            assert move.from_pos is not None

            # Check for reveal rule BEFORE completing the move
            piece = new_state.remove_top(move.from_pos)

            # Check if opponent wins after lift (reveal)
            opponent_winning_lines = new_state.get_winning_lines(opponent)

            if opponent_winning_lines:
                # Check if the destination breaks all winning lines
                can_save = False
                for line in opponent_winning_lines:
                    if move.to_pos in line:
                        target = new_state.get_top(move.to_pos)
                        if target is not None and piece.can_gobble(target):
                            can_save = True
                            break

                if not can_save:
                    # Player loses due to reveal rule
                    # Don't complete the move, opponent wins
                    self.state = new_state
                    self.result = GameResult.winner(opponent)
                    return MoveResult(
                        new_state=new_state,
                        game_result=self.result,
                        revealed_loss=True,
                    )

            # Complete the move
            new_state.place_piece(piece, move.to_pos)

        # Record position for repetition detection
        new_state.record_position()

        # Check for winner
        winner = new_state.check_winner()
        if winner is not None:
            self.state = new_state
            self.result = GameResult.winner(winner)
            self.move_history.append(move)
            return MoveResult(new_state=new_state, game_result=self.result)

        # Check for threefold repetition
        if new_state.is_threefold_repetition():
            self.state = new_state
            self.result = GameResult.DRAW
            self.move_history.append(move)
            return MoveResult(new_state=new_state, game_result=self.result)

        # Switch player
        new_state.current_player = opponent

        # Check if next player has any legal moves
        # (if not, they're in zugzwang and will lose on their turn)

        self.state = new_state
        self.move_history.append(move)
        return MoveResult(new_state=new_state, game_result=GameResult.ONGOING)

    def is_over(self) -> bool:
        """Check if the game is over."""
        return self.result != GameResult.ONGOING

    def __repr__(self) -> str:
        return f"Game(result={self.result.value})\n{self.state}"


def play_move(state: GameState, move: Move) -> tuple[GameState, GameResult]:
    """
    Functional interface: apply a move to a state and return new state + result.

    This is useful for solver code that doesn't need the full Game object.
    """
    game = Game(state.copy())
    result = game.apply_move(move)
    return result.new_state, result.game_result
