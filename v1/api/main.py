"""FastAPI backend for Gobblet Gobblers."""

from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel

from gobblet import Game, GameResult, GameState, Move, Player, Size, generate_moves, move_to_notation, notation_to_move
from solver.encoding import state_to_base64, base64_to_state

app = FastAPI(title="Gobblet Gobblers API")

# Allow CORS for local development
app.add_middleware(
    CORSMiddleware,
    allow_origins=["http://localhost:5173"],  # Vite dev server
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

# --- Game state and history ---

game = Game()
state_snapshots: list[GameState] = [game.state.copy()]  # index 0 = initial state
move_notations: list[str] = []  # move_notations[i] = move that led to state_snapshots[i+1]
current_index: int = 0  # current position in state_snapshots


def _reset_history() -> None:
    """Reset history to initial state."""
    global game, state_snapshots, move_notations, current_index
    game = Game()
    state_snapshots = [game.state.copy()]
    move_notations = []
    current_index = 0


def _get_current_result() -> GameResult:
    """Get the game result for current state."""
    current_state = state_snapshots[current_index]

    # Check if there's a winner in current state
    winner = current_state.check_winner()
    if winner == Player.ONE:
        return GameResult.PLAYER_ONE_WINS
    elif winner == Player.TWO:
        return GameResult.PLAYER_TWO_WINS

    # Check for draw (threefold repetition)
    if current_state.is_threefold_repetition():
        return GameResult.DRAW

    # Check for zugzwang (no legal moves = current player loses)
    legal_moves = generate_moves(current_state)
    if len(legal_moves) == 0:
        # Current player has no legal moves, opponent wins
        if current_state.current_player == Player.ONE:
            return GameResult.PLAYER_TWO_WINS
        else:
            return GameResult.PLAYER_ONE_WINS

    return GameResult.ONGOING


# --- Pydantic models for API ---


class PieceModel(BaseModel):
    player: int  # 1 or 2
    size: int  # 1=small, 2=medium, 3=large


class CellModel(BaseModel):
    stack: list[PieceModel]  # Bottom to top


class ReservesModel(BaseModel):
    small: int
    medium: int
    large: int


class GameStateModel(BaseModel):
    board: list[list[CellModel]]  # 3x3 grid
    reserves: dict[str, ReservesModel]  # "1" and "2" for players
    current_player: int
    result: str  # "ongoing", "player_one_wins", "player_two_wins", "draw"
    move_index: int  # current position in history
    can_undo: bool
    can_redo: bool


class MoveModel(BaseModel):
    to_row: int
    to_col: int
    from_row: int | None = None  # None means from reserve
    from_col: int | None = None
    size: int | None = None  # Only for reserve placement (1=S, 2=M, 3=L)


class LegalMoveModel(BaseModel):
    to_pos: tuple[int, int]
    from_pos: tuple[int, int] | None
    size: int | None


class HistoryEntryModel(BaseModel):
    index: int  # 1-indexed for display (move 1, move 2, ...)
    notation: str
    player: int  # who made this move


class HistoryModel(BaseModel):
    moves: list[HistoryEntryModel]
    current_index: int  # 0 = initial state, 1 = after move 1, etc.
    total_moves: int


class ExportModel(BaseModel):
    notation: str


class ImportModel(BaseModel):
    notation: str


class StateExportModel(BaseModel):
    state: str  # Compact state notation


class StateImportModel(BaseModel):
    state: str  # Compact state notation


# --- Helper functions ---


def state_to_model(state: GameState, result: GameResult) -> GameStateModel:
    """Convert GameState to API model."""
    board: list[list[CellModel]] = []
    for row in range(3):
        board_row: list[CellModel] = []
        for col in range(3):
            stack = state.get_stack((row, col))
            cell = CellModel(
                stack=[PieceModel(player=p.player.value, size=p.size.value) for p in stack],
            )
            board_row.append(cell)
        board.append(board_row)

    reserves = {}
    for player in Player:
        player_reserves = state.get_all_reserves(player)
        reserves[str(player.value)] = ReservesModel(
            small=player_reserves[Size.SMALL],
            medium=player_reserves[Size.MEDIUM],
            large=player_reserves[Size.LARGE],
        )

    return GameStateModel(
        board=board,
        reserves=reserves,
        current_player=state.current_player.value,
        result=result.value,
        move_index=current_index,
        can_undo=current_index > 0,
        can_redo=current_index < len(state_snapshots) - 1,
    )


def move_to_model(move: Move) -> LegalMoveModel:
    """Convert Move to API model."""
    return LegalMoveModel(
        to_pos=move.to_pos,
        from_pos=move.from_pos,
        size=move.size.value if move.size else None,
    )


# --- API endpoints ---


@app.get("/game", response_model=GameStateModel)
def get_game():
    """Get current game state."""
    return state_to_model(state_snapshots[current_index], _get_current_result())


@app.get("/moves", response_model=list[LegalMoveModel])
def get_moves():
    """Get all legal moves for current player."""
    # Rebuild game from current state to get legal moves
    temp_game = Game(state_snapshots[current_index].copy())
    moves = temp_game.get_legal_moves()
    return [move_to_model(m) for m in moves]


@app.post("/move", response_model=GameStateModel)
def make_move(move: MoveModel):
    """Make a move."""
    global game, state_snapshots, move_notations, current_index

    current_state = state_snapshots[current_index]
    result = _get_current_result()

    if result != GameResult.ONGOING:
        raise HTTPException(status_code=400, detail="Game is already over")

    player = current_state.current_player

    # Build the Move object
    to_pos = (move.to_row, move.to_col)

    if move.from_row is not None and move.from_col is not None:
        # Move from board
        from_pos = (move.from_row, move.from_col)
        game_move = Move(player=player, to_pos=to_pos, from_pos=from_pos)
    else:
        # Place from reserve
        if move.size is None:
            raise HTTPException(status_code=400, detail="Size required for reserve placement")
        size = Size(move.size)
        game_move = Move(player=player, to_pos=to_pos, size=size)

    # Validate move is legal
    temp_game = Game(current_state.copy())
    legal_moves = temp_game.get_legal_moves()
    if game_move not in legal_moves:
        raise HTTPException(status_code=400, detail="Illegal move")

    # Truncate any "future" history if we've undone
    state_snapshots = state_snapshots[: current_index + 1]
    move_notations = move_notations[:current_index]

    # Apply move
    temp_game.apply_move(game_move)

    # Store new state and notation
    state_snapshots.append(temp_game.state.copy())
    move_notations.append(move_to_notation(game_move))
    current_index += 1

    # Update main game reference
    game = temp_game

    return state_to_model(state_snapshots[current_index], game.result)


@app.post("/reset", response_model=GameStateModel)
def reset_game():
    """Reset to a new game."""
    _reset_history()
    return state_to_model(state_snapshots[current_index], GameResult.ONGOING)


@app.get("/history", response_model=HistoryModel)
def get_history():
    """Get move history."""
    moves = []
    for i, notation in enumerate(move_notations):
        # Determine which player made this move (alternates, P1 starts)
        player = 1 if i % 2 == 0 else 2
        moves.append(HistoryEntryModel(index=i + 1, notation=notation, player=player))

    return HistoryModel(
        moves=moves,
        current_index=current_index,
        total_moves=len(move_notations),
    )


@app.post("/undo", response_model=GameStateModel)
def undo():
    """Undo the last move."""
    global game, current_index

    if current_index <= 0:
        raise HTTPException(status_code=400, detail="Nothing to undo")

    current_index -= 1
    game = Game(state_snapshots[current_index].copy())

    return state_to_model(state_snapshots[current_index], _get_current_result())


@app.post("/redo", response_model=GameStateModel)
def redo():
    """Redo a previously undone move."""
    global game, current_index

    if current_index >= len(state_snapshots) - 1:
        raise HTTPException(status_code=400, detail="Nothing to redo")

    current_index += 1
    game = Game(state_snapshots[current_index].copy())

    return state_to_model(state_snapshots[current_index], _get_current_result())


@app.post("/goto/{move_index}", response_model=GameStateModel)
def goto_move(move_index: int):
    """Jump to a specific point in history."""
    global game, current_index

    if move_index < 0 or move_index >= len(state_snapshots):
        raise HTTPException(status_code=400, detail="Invalid move index")

    current_index = move_index
    game = Game(state_snapshots[current_index].copy())

    return state_to_model(state_snapshots[current_index], _get_current_result())


@app.get("/health")
def health():
    """Health check endpoint."""
    return {"status": "ok"}


@app.get("/export", response_model=ExportModel)
def export_game():
    """Export current game as notation string."""
    return ExportModel(notation=" ".join(move_notations))


@app.post("/import", response_model=GameStateModel)
def import_game(data: ImportModel):
    """Import a game from notation string."""
    global game, state_snapshots, move_notations, current_index

    # Parse notation string into individual moves
    notation_str = data.notation.strip()
    if not notation_str:
        # Empty string = reset to initial state
        _reset_history()
        return state_to_model(state_snapshots[current_index], GameResult.ONGOING)

    notations = notation_str.split()

    # Reset to initial state
    _reset_history()

    # Replay each move
    for i, notation in enumerate(notations):
        current_state = state_snapshots[current_index]
        player = current_state.current_player

        try:
            move = notation_to_move(notation, player)
        except ValueError as e:
            raise HTTPException(status_code=400, detail=f"Move {i + 1}: {e}")

        # Validate move is legal
        temp_game = Game(current_state.copy())
        legal_moves = temp_game.get_legal_moves()
        if move not in legal_moves:
            raise HTTPException(status_code=400, detail=f"Move {i + 1} ({notation}): Illegal move")

        # Apply move
        temp_game.apply_move(move)

        # Store new state and notation
        state_snapshots.append(temp_game.state.copy())
        move_notations.append(notation)
        current_index += 1

        # Update main game reference
        game = temp_game

        # Check if game is over
        if game.result != GameResult.ONGOING:
            break

    return state_to_model(state_snapshots[current_index], _get_current_result())


@app.get("/state/export", response_model=StateExportModel)
def export_state():
    """Export current game state as base64-encoded binary."""
    current_state = state_snapshots[current_index]
    return StateExportModel(state=state_to_base64(current_state))


@app.post("/state/import", response_model=GameStateModel)
def import_state(data: StateImportModel):
    """
    Import a game state from base64-encoded binary.

    This replaces the current game state entirely (clears history).
    """
    global game, state_snapshots, move_notations, current_index

    try:
        new_state = base64_to_state(data.state.strip())
    except Exception as e:
        raise HTTPException(status_code=400, detail=f"Invalid state encoding: {e}")

    # Reset history and set the new state as initial
    game = Game(new_state)
    state_snapshots = [new_state.copy()]
    move_notations = []
    current_index = 0

    return state_to_model(state_snapshots[current_index], _get_current_result())
