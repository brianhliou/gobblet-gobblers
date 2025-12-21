//! Gobblet Gobblers Web API (DEPRECATED)
//!
//! This was the original Rust backend that served game logic via REST API.
//! It has been superseded by the WASM architecture where gobblet-core is
//! compiled to WebAssembly and runs directly in the browser.
//!
//! The only remaining backend call is for tablebase lookups, which is now
//! handled by a Vercel serverless function reading from a binary file
//! (see v2/frontend-wasm/api/lookup/batch.ts).
//!
//! This code is kept for reference but is no longer used in production.
//! It reads from SQLite (tablebase.db) which has also been deprecated in
//! favor of the binary format (tablebase.bin).

use std::path::Path as FilePath;
use std::sync::{Arc, Mutex};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tower_http::cors::{Any, CorsLayer};

use gobblet_core::{Board, Move, Player, Pos, Size};

// =============================================================================
// Tablebase
// =============================================================================

/// SQLite-backed tablebase for position evaluations
struct Tablebase {
    conn: Mutex<Connection>,
}

impl Tablebase {
    /// Load tablebase from SQLite file
    fn load(path: &FilePath) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(path)?;
        // Enable read-only optimizations
        conn.pragma_update(None, "query_only", true)?;
        Ok(Tablebase { conn: Mutex::new(conn) })
    }

    /// Look up position evaluation
    /// Returns: 1 (P1 wins), 0 (draw), -1 (P2 wins), or None if not found
    fn lookup(&self, canonical: u64) -> Option<i8> {
        let conn = self.conn.lock().unwrap();
        let result: Result<i32, _> = conn.query_row(
            "SELECT outcome FROM positions WHERE canonical = ?1",
            [canonical as i64],
            |row| row.get(0),
        );
        result.ok().map(|v| v as i8)
    }
}

// =============================================================================
// Session State
// =============================================================================

/// Global game session state
struct GameSession {
    /// History of board states (index 0 = starting position)
    states: Vec<Board>,
    /// Move notations (notations[i] = move that led to states[i+1])
    notations: Vec<String>,
    /// Current position in history
    current_index: usize,
    /// Starting position encoding (None = initial empty board)
    starting_position: Option<u64>,
}

impl GameSession {
    fn new() -> Self {
        Self {
            states: vec![Board::new()],
            notations: vec![],
            current_index: 0,
            starting_position: None,
        }
    }

    fn current_board(&self) -> &Board {
        &self.states[self.current_index]
    }

    fn reset(&mut self) {
        self.states = vec![Board::new()];
        self.notations = vec![];
        self.current_index = 0;
        self.starting_position = None;
    }

    fn reset_to_state(&mut self, board: Board) {
        let encoding = board.to_u64();
        self.starting_position = if encoding == 0 { None } else { Some(encoding) };
        self.states = vec![board];
        self.notations = vec![];
        self.current_index = 0;
    }

    fn can_undo(&self) -> bool {
        self.current_index > 0
    }

    fn can_redo(&self) -> bool {
        self.current_index < self.states.len() - 1
    }
}

/// Shared application state
struct AppStateInner {
    session: Mutex<GameSession>,
    tablebase: Option<Tablebase>,
}

type AppState = Arc<AppStateInner>;

// =============================================================================
// JSON Models (matching V1 API contract)
// =============================================================================

#[derive(Serialize)]
struct PieceModel {
    player: u8,
    size: u8, // 1=small, 2=medium, 3=large
}

#[derive(Serialize)]
struct CellModel {
    stack: Vec<PieceModel>,
}

#[derive(Serialize)]
struct ReservesModel {
    small: u8,
    medium: u8,
    large: u8,
}

#[derive(Serialize)]
struct GameStateModel {
    board: Vec<Vec<CellModel>>,
    reserves: std::collections::HashMap<String, ReservesModel>,
    current_player: u8,
    result: String,
    move_index: usize,
    can_undo: bool,
    can_redo: bool,
    encoding: u64, // V2 addition: raw board encoding
    /// The winning line positions, if there's a winner (not for zugzwang/draw)
    #[serde(skip_serializing_if = "Option::is_none")]
    winning_line: Option<Vec<(u8, u8)>>,
}

#[derive(Serialize)]
struct LegalMoveModel {
    to_pos: (u8, u8),
    from_pos: Option<(u8, u8)>,
    size: Option<u8>,
    /// Evaluation from tablebase: 1 (P1 wins), 0 (draw), -1 (P2 wins), null if not found
    #[serde(skip_serializing_if = "Option::is_none")]
    evaluation: Option<i8>,
}

#[derive(Deserialize)]
struct MoveRequest {
    to_row: u8,
    to_col: u8,
    from_row: Option<u8>,
    from_col: Option<u8>,
    size: Option<u8>,
}

#[derive(Serialize)]
struct HistoryEntryModel {
    index: usize,
    notation: String,
    player: u8,
}

#[derive(Serialize)]
struct HistoryModel {
    moves: Vec<HistoryEntryModel>,
    current_index: usize,
    total_moves: usize,
}

#[derive(Serialize)]
struct ExportModel {
    notation: String,
}

#[derive(Deserialize)]
struct ImportRequest {
    notation: String,
}

#[derive(Serialize)]
struct StateExportModel {
    encoding: u64,
}

#[derive(Deserialize)]
struct StateImportRequest {
    encoding: u64,
}

#[derive(Serialize)]
struct HealthModel {
    status: String,
}

#[derive(Serialize)]
struct ErrorModel {
    detail: String,
}

// =============================================================================
// Conversion Functions
// =============================================================================

/// Convert Board to JSON-serializable GameStateModel
fn board_to_model(board: &Board, session: &GameSession) -> GameStateModel {
    let mut rows = Vec::with_capacity(3);

    for row in 0..3 {
        let mut cells = Vec::with_capacity(3);
        for col in 0..3 {
            let pos = Pos::from_row_col(row, col);
            let mut stack = Vec::new();

            // Extract stack from cell (small, medium, large order = bottom to top)
            let cell = board.cell(pos);
            for size_idx in 0..3 {
                let owner = ((cell >> (size_idx * 2)) & 0b11) as u8;
                if owner != 0 {
                    stack.push(PieceModel {
                        player: owner,
                        size: size_idx as u8 + 1, // 1=small, 2=medium, 3=large
                    });
                }
            }

            cells.push(CellModel { stack });
        }
        rows.push(cells);
    }

    let mut reserves = std::collections::HashMap::new();
    for player in [Player::One, Player::Two] {
        let r = board.reserves(player);
        reserves.insert(
            (player as u8).to_string(),
            ReservesModel {
                small: r[0],
                medium: r[1],
                large: r[2],
            },
        );
    }

    let (result, winning_line) = if let Some(winner) = board.check_winner() {
        let line = board.winning_line(winner).map(|positions| {
            positions
                .iter()
                .map(|pos| (pos.row(), pos.col()))
                .collect()
        });
        let result_str = match winner {
            Player::One => "player_one_wins",
            Player::Two => "player_two_wins",
        };
        (result_str, line)
    } else if board.legal_moves().is_empty() {
        // Zugzwang - current player loses (no winning line to show)
        let result_str = match board.current_player() {
            Player::One => "player_two_wins",
            Player::Two => "player_one_wins",
        };
        (result_str, None)
    } else {
        ("ongoing", None)
    };

    GameStateModel {
        board: rows,
        reserves,
        current_player: board.current_player() as u8,
        result: result.to_string(),
        move_index: session.current_index,
        can_undo: session.can_undo(),
        can_redo: session.can_redo(),
        encoding: board.to_u64(),
        winning_line,
    }
}

/// Convert internal Move to JSON model
fn move_to_model(m: &Move, evaluation: Option<i8>) -> LegalMoveModel {
    match m {
        Move::Place { size, to } => LegalMoveModel {
            to_pos: (to.row(), to.col()),
            from_pos: None,
            size: Some(*size as u8 + 1),
            evaluation,
        },
        Move::Slide { from, to } => LegalMoveModel {
            to_pos: (to.row(), to.col()),
            from_pos: Some((from.row(), from.col())),
            size: None,
            evaluation,
        },
    }
}

/// Convert Move to notation string
fn move_to_notation(m: &Move) -> String {
    match m {
        Move::Place { size, to } => {
            let size_char = match size {
                Size::Small => 'S',
                Size::Medium => 'M',
                Size::Large => 'L',
            };
            format!("{}({},{})", size_char, to.row(), to.col())
        }
        Move::Slide { from, to } => {
            format!("({},{})→({},{})", from.row(), from.col(), to.row(), to.col())
        }
    }
}

/// Parse notation string to Move
fn notation_to_move(notation: &str, _player: Player) -> Result<Move, String> {
    let notation = notation.trim();

    // Reserve placement: S(0,0), M(1,2), L(2,1)
    if notation.starts_with('S') || notation.starts_with('M') || notation.starts_with('L') {
        let size = match notation.chars().next().unwrap() {
            'S' => Size::Small,
            'M' => Size::Medium,
            'L' => Size::Large,
            _ => return Err("Invalid size".to_string()),
        };

        // Parse (row,col)
        let coords = &notation[1..];
        if !coords.starts_with('(') || !coords.ends_with(')') {
            return Err("Invalid format: expected (row,col)".to_string());
        }
        let inner = &coords[1..coords.len() - 1];
        let parts: Vec<&str> = inner.split(',').collect();
        if parts.len() != 2 {
            return Err("Invalid format: expected row,col".to_string());
        }
        let row: u8 = parts[0].trim().parse().map_err(|_| "Invalid row")?;
        let col: u8 = parts[1].trim().parse().map_err(|_| "Invalid col")?;

        if row > 2 || col > 2 {
            return Err("Position out of range".to_string());
        }

        Ok(Move::Place {
            size,
            to: Pos::from_row_col(row, col),
        })
    }
    // Board move: (0,0)→(1,1) or (0,0)->(1,1)
    else if notation.starts_with('(') {
        let arrow_pos = notation.find('→').or_else(|| notation.find("->"));
        let arrow_pos = arrow_pos.ok_or("Invalid format: expected arrow")?;
        let arrow_len = if notation.contains('→') { 3 } else { 2 }; // UTF-8 arrow is 3 bytes

        let from_str = &notation[..arrow_pos];
        let to_str = &notation[arrow_pos + arrow_len..];

        let parse_pos = |s: &str| -> Result<(u8, u8), String> {
            let s = s.trim();
            if !s.starts_with('(') || !s.ends_with(')') {
                return Err("Invalid position format".to_string());
            }
            let inner = &s[1..s.len() - 1];
            let parts: Vec<&str> = inner.split(',').collect();
            if parts.len() != 2 {
                return Err("Invalid position format".to_string());
            }
            let row: u8 = parts[0].trim().parse().map_err(|_| "Invalid row")?;
            let col: u8 = parts[1].trim().parse().map_err(|_| "Invalid col")?;
            if row > 2 || col > 2 {
                return Err("Position out of range".to_string());
            }
            Ok((row, col))
        };

        let (fr, fc) = parse_pos(from_str)?;
        let (tr, tc) = parse_pos(to_str)?;

        Ok(Move::Slide {
            from: Pos::from_row_col(fr, fc),
            to: Pos::from_row_col(tr, tc),
        })
    } else {
        Err("Invalid notation format".to_string())
    }
}

/// Parse MoveRequest to internal Move
fn request_to_move(req: &MoveRequest, _player: Player) -> Result<Move, String> {
    let to = Pos::from_row_col(req.to_row, req.to_col);

    if let (Some(fr), Some(fc)) = (req.from_row, req.from_col) {
        // Slide move
        Ok(Move::Slide {
            from: Pos::from_row_col(fr, fc),
            to,
        })
    } else if let Some(size) = req.size {
        // Place from reserve
        let size = match size {
            1 => Size::Small,
            2 => Size::Medium,
            3 => Size::Large,
            _ => return Err("Invalid size".to_string()),
        };
        Ok(Move::Place { size, to })
    } else {
        Err("Must specify from_row/from_col or size".to_string())
    }
}

// =============================================================================
// API Endpoints
// =============================================================================

async fn get_game(State(state): State<AppState>) -> Json<GameStateModel> {
    let session = state.session.lock().unwrap();
    let board = session.current_board();
    Json(board_to_model(board, &session))
}

async fn get_moves(State(state): State<AppState>) -> Json<Vec<LegalMoveModel>> {
    let session = state.session.lock().unwrap();
    let board = session.current_board().clone();
    drop(session); // Release lock before tablebase queries

    let moves = board.legal_moves();

    // Look up evaluations for each resulting position
    let move_models: Vec<LegalMoveModel> = moves
        .iter()
        .map(|m| {
            let evaluation = if let Some(ref tablebase) = state.tablebase {
                // Apply the move to get the child position
                let mut child = board.clone();
                child.apply(m.clone());

                // Check for terminal states first
                if let Some(winner) = child.check_winner() {
                    // Terminal win - return from perspective of who just moved
                    Some(if winner == Player::One { 1 } else { -1 })
                } else if child.legal_moves().is_empty() {
                    // Zugzwang - current player loses
                    Some(if child.current_player() == Player::One { -1 } else { 1 })
                } else {
                    // Look up in tablebase
                    tablebase.lookup(child.canonical())
                }
            } else {
                None
            };
            move_to_model(m, evaluation)
        })
        .collect();

    Json(move_models)
}

async fn make_move(
    State(state): State<AppState>,
    Json(req): Json<MoveRequest>,
) -> Result<Json<GameStateModel>, (StatusCode, Json<ErrorModel>)> {
    let mut session = state.session.lock().unwrap();
    let board = session.current_board().clone();

    // Check game not over
    if board.check_winner().is_some() || board.legal_moves().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorModel {
                detail: "Game is already over".to_string(),
            }),
        ));
    }

    // Parse move
    let player = board.current_player();
    let mov = request_to_move(&req, player).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorModel { detail: e }),
        )
    })?;

    // Validate move is legal
    let legal_moves = board.legal_moves();
    if !legal_moves.contains(&mov) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorModel {
                detail: "Illegal move".to_string(),
            }),
        ));
    }

    // Truncate future history if we've undone
    let idx = session.current_index;
    session.states.truncate(idx + 1);
    session.notations.truncate(idx);

    // Apply move
    let mut new_board = board.clone();
    new_board.apply(mov.clone());

    // Store new state
    let notation = move_to_notation(&mov);
    session.states.push(new_board);
    session.notations.push(notation);
    session.current_index += 1;

    let new_board = session.current_board();
    Ok(Json(board_to_model(new_board, &session)))
}

async fn reset_game(State(state): State<AppState>) -> Json<GameStateModel> {
    let mut session = state.session.lock().unwrap();
    session.reset();
    let board = session.current_board();
    Json(board_to_model(board, &session))
}

async fn get_history(State(state): State<AppState>) -> Json<HistoryModel> {
    let session = state.session.lock().unwrap();

    let moves: Vec<HistoryEntryModel> = session
        .notations
        .iter()
        .enumerate()
        .map(|(i, notation)| HistoryEntryModel {
            index: i + 1,
            notation: notation.clone(),
            player: if i % 2 == 0 { 1 } else { 2 },
        })
        .collect();

    Json(HistoryModel {
        total_moves: session.notations.len(),
        current_index: session.current_index,
        moves,
    })
}

async fn undo(
    State(state): State<AppState>,
) -> Result<Json<GameStateModel>, (StatusCode, Json<ErrorModel>)> {
    let mut session = state.session.lock().unwrap();

    if !session.can_undo() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorModel {
                detail: "Nothing to undo".to_string(),
            }),
        ));
    }

    session.current_index -= 1;
    let board = session.current_board();
    Ok(Json(board_to_model(board, &session)))
}

async fn redo(
    State(state): State<AppState>,
) -> Result<Json<GameStateModel>, (StatusCode, Json<ErrorModel>)> {
    let mut session = state.session.lock().unwrap();

    if !session.can_redo() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorModel {
                detail: "Nothing to redo".to_string(),
            }),
        ));
    }

    session.current_index += 1;
    let board = session.current_board();
    Ok(Json(board_to_model(board, &session)))
}

async fn goto_move(
    State(state): State<AppState>,
    Path(move_index): Path<usize>,
) -> Result<Json<GameStateModel>, (StatusCode, Json<ErrorModel>)> {
    let mut session = state.session.lock().unwrap();

    if move_index >= session.states.len() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorModel {
                detail: "Invalid move index".to_string(),
            }),
        ));
    }

    session.current_index = move_index;
    let board = session.current_board();
    Ok(Json(board_to_model(board, &session)))
}

async fn export_game(State(state): State<AppState>) -> Json<ExportModel> {
    let session = state.session.lock().unwrap();
    let moves = session.notations.join(" ");

    // If we started from a non-initial position, prefix with FROM:<encoding>
    let notation = if let Some(encoding) = session.starting_position {
        if moves.is_empty() {
            format!("FROM:{}", encoding)
        } else {
            format!("FROM:{} {}", encoding, moves)
        }
    } else {
        moves
    };

    Json(ExportModel { notation })
}

async fn import_game(
    State(state): State<AppState>,
    Json(req): Json<ImportRequest>,
) -> Result<Json<GameStateModel>, (StatusCode, Json<ErrorModel>)> {
    let mut session = state.session.lock().unwrap();

    let notation_str = req.notation.trim();
    if notation_str.is_empty() {
        session.reset();
        let board = session.current_board();
        return Ok(Json(board_to_model(board, &session)));
    }

    // Check for FROM: prefix (combined format with starting position)
    let (starting_board, move_notations) = if notation_str.starts_with("FROM:") {
        let rest = &notation_str[5..]; // Skip "FROM:"
        let mut parts = rest.split_whitespace();

        // First token is the encoding
        let encoding_str = parts.next().ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorModel {
                    detail: "FROM: prefix requires an encoding".to_string(),
                }),
            )
        })?;

        let encoding: u64 = encoding_str.parse().map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorModel {
                    detail: format!("Invalid encoding: {}", encoding_str),
                }),
            )
        })?;

        let board = Board::from_u64(encoding);
        let moves: Vec<&str> = parts.collect();
        (board, moves)
    } else {
        // Normal format: start from initial position
        let moves: Vec<&str> = notation_str.split_whitespace().collect();
        (Board::new(), moves)
    };

    // Reset to the starting position
    session.reset_to_state(starting_board);

    // Replay moves
    for (i, notation) in move_notations.iter().enumerate() {
        let board = session.current_board().clone();
        let player = board.current_player();

        let mov = notation_to_move(notation, player).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorModel {
                    detail: format!("Move {}: {}", i + 1, e),
                }),
            )
        })?;

        // Validate move is legal
        let legal_moves = board.legal_moves();
        if !legal_moves.contains(&mov) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorModel {
                    detail: format!("Move {} ({}): Illegal move", i + 1, notation),
                }),
            ));
        }

        // Apply move
        let mut new_board = board;
        new_board.apply(mov);

        session.states.push(new_board);
        session.notations.push(notation.to_string());
        session.current_index += 1;

        // Check if game is over
        let current = session.current_board();
        if current.check_winner().is_some() || current.legal_moves().is_empty() {
            break;
        }
    }

    let board = session.current_board();
    Ok(Json(board_to_model(board, &session)))
}

async fn export_state(State(state): State<AppState>) -> Json<StateExportModel> {
    let session = state.session.lock().unwrap();
    Json(StateExportModel {
        encoding: session.current_board().to_u64(),
    })
}

async fn import_state(
    State(state): State<AppState>,
    Json(req): Json<StateImportRequest>,
) -> Result<Json<GameStateModel>, (StatusCode, Json<ErrorModel>)> {
    let mut session = state.session.lock().unwrap();

    let board = Board::from_u64(req.encoding);
    session.reset_to_state(board);

    let board = session.current_board();
    Ok(Json(board_to_model(board, &session)))
}

async fn health() -> Json<HealthModel> {
    Json(HealthModel {
        status: "ok".to_string(),
    })
}

// Batch tablebase lookup endpoint
#[derive(Deserialize)]
struct BatchLookupRequest {
    positions: Vec<String>,
}

#[derive(Serialize)]
struct BatchLookupResponse {
    evaluations: Vec<Option<i8>>,
}

async fn lookup_positions_batch(
    State(state): State<AppState>,
    Json(request): Json<BatchLookupRequest>,
) -> Json<BatchLookupResponse> {
    let evaluations = if let Some(ref tablebase) = state.tablebase {
        request
            .positions
            .iter()
            .map(|pos| {
                pos.parse::<u64>()
                    .ok()
                    .and_then(|canonical| tablebase.lookup(canonical))
            })
            .collect()
    } else {
        request.positions.iter().map(|_| None).collect()
    };
    Json(BatchLookupResponse { evaluations })
}

// =============================================================================
// Main
// =============================================================================

#[tokio::main]
async fn main() {
    // Try to load tablebase from common locations
    let tablebase_paths = [
        "data/tablebase.db",
        "../gobblet-solver/data/tablebase.db",
        "tablebase.db",
    ];

    let tablebase = tablebase_paths
        .iter()
        .find_map(|path| {
            let path = FilePath::new(path);
            if path.exists() {
                match Tablebase::load(path) {
                    Ok(tb) => {
                        println!("Loaded tablebase from {:?}", path);
                        Some(tb)
                    }
                    Err(e) => {
                        eprintln!("Failed to load tablebase from {:?}: {}", path, e);
                        None
                    }
                }
            } else {
                None
            }
        });

    if tablebase.is_none() {
        println!("No tablebase found - move evaluations will be unavailable");
    }

    let state: AppState = Arc::new(AppStateInner {
        session: Mutex::new(GameSession::new()),
        tablebase,
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/game", get(get_game))
        .route("/moves", get(get_moves))
        .route("/move", post(make_move))
        .route("/reset", post(reset_game))
        .route("/history", get(get_history))
        .route("/undo", post(undo))
        .route("/redo", post(redo))
        .route("/goto/{move_index}", post(goto_move))
        .route("/export", get(export_game))
        .route("/import", post(import_game))
        .route("/state/export", get(export_state))
        .route("/state/import", post(import_state))
        .route("/lookup/batch", post(lookup_positions_batch))
        .route("/health", get(health))
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8000").await.unwrap();
    println!("Gobblet API running on http://localhost:8000");
    axum::serve(listener, app).await.unwrap();
}
