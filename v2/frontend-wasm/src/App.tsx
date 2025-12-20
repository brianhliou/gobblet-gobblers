import { useCallback, useEffect, useState, useRef } from "react";
import init, { WasmBoard } from "gobblet-core";
import { lookupPositions } from "./api";
import { Board } from "./components/Board";
import { History } from "./components/History";
import { MovesPanel } from "./components/MovesPanel";
import { Reserves } from "./components/Reserves";
import type { Cell, GameState, History as HistoryType, LegalMove, Piece, Reserves as ReservesType, Selection } from "./types";
import "./App.css";

// Move notation for history display
interface MoveNotation {
  notation: string;
  player: 1 | 2;
}

// Convert WasmBoard to GameState format for components
function boardToGameState(
  board: WasmBoard,
  historyIndex: number,
  canUndo: boolean,
  canRedo: boolean
): GameState {
  // Build board cells
  const cells: Cell[][] = [];
  for (let row = 0; row < 3; row++) {
    const rowCells: Cell[] = [];
    for (let col = 0; col < 3; col++) {
      const stackData = board.cellStack(row, col);
      const stack: Piece[] = [];
      for (let i = 0; i < stackData.length; i += 2) {
        stack.push({
          player: stackData[i] as 1 | 2,
          size: stackData[i + 1] as 1 | 2 | 3,
        });
      }
      rowCells.push({ stack });
    }
    cells.push(rowCells);
  }

  // Get reserves
  const p1Reserves = board.reserves(1);
  const p2Reserves = board.reserves(2);
  const reserves = {
    "1": { small: p1Reserves[0], medium: p1Reserves[1], large: p1Reserves[2] } as ReservesType,
    "2": { small: p2Reserves[0], medium: p2Reserves[1], large: p2Reserves[2] } as ReservesType,
  };

  // Get winning line
  const winningLineData = board.winningLine();
  const winningLine: [number, number][] | undefined =
    winningLineData.length === 6
      ? [
          [winningLineData[0], winningLineData[1]],
          [winningLineData[2], winningLineData[3]],
          [winningLineData[4], winningLineData[5]],
        ]
      : undefined;

  return {
    board: cells,
    reserves,
    current_player: board.currentPlayer() as 1 | 2,
    result: board.result() as GameState["result"],
    move_index: historyIndex,
    can_undo: canUndo,
    can_redo: canRedo,
    winning_line: winningLine,
  };
}

// Convert WasmBoard legal moves to LegalMove format
function getLegalMoves(board: WasmBoard): LegalMove[] {
  const rawMoves = board.legalMoves();
  console.log("Raw moves from WASM:", rawMoves);

  // Handle both regular arrays and Uint8Arrays from WASM
  const toTuple = (arr: number[] | Uint8Array | [number, number]): [number, number] => [arr[0], arr[1]];

  const moves = rawMoves as Array<{ to: number[] | Uint8Array; from: number[] | Uint8Array | null; size: number | null }>;
  return moves.map((m) => ({
    to_pos: toTuple(m.to),
    from_pos: m.from ? toTuple(m.from) : null,
    size: m.size,
  }));
}

// Format move as notation string
function formatMoveNotation(move: LegalMove): string {
  if (move.from_pos === null && move.size !== null) {
    const sizeChar = { 1: "S", 2: "M", 3: "L" }[move.size];
    return `${sizeChar}(${move.to_pos[0]},${move.to_pos[1]})`;
  } else if (move.from_pos !== null) {
    return `(${move.from_pos[0]},${move.from_pos[1]})→(${move.to_pos[0]},${move.to_pos[1]})`;
  }
  return "?";
}

// Parse move notation string into a LegalMove
// Formats: "S(0,0)", "M(1,2)", "L(2,1)" for placements
//          "(0,0)→(1,1)" for slides
function parseMoveNotation(notation: string): LegalMove | null {
  const trimmed = notation.trim();

  // Try placement format: S(0,0), M(1,2), L(2,1)
  const placeMatch = trimmed.match(/^([SML])\((\d),(\d)\)$/);
  if (placeMatch) {
    const sizeMap: Record<string, number> = { S: 1, M: 2, L: 3 };
    return {
      to_pos: [parseInt(placeMatch[2]), parseInt(placeMatch[3])],
      from_pos: null,
      size: sizeMap[placeMatch[1]],
    };
  }

  // Try slide format: (0,0)→(1,1) or (0,0)->(1,1)
  const slideMatch = trimmed.match(/^\((\d),(\d)\)[→\->]+\((\d),(\d)\)$/);
  if (slideMatch) {
    return {
      to_pos: [parseInt(slideMatch[3]), parseInt(slideMatch[4])],
      from_pos: [parseInt(slideMatch[1]), parseInt(slideMatch[2])],
      size: null,
    };
  }

  return null;
}

function App() {
  // WASM initialization state
  const [wasmReady, setWasmReady] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Game state
  const boardRef = useRef<WasmBoard | null>(null);
  const [gameState, setGameState] = useState<GameState | null>(null);
  const [legalMoves, setLegalMoves] = useState<LegalMove[]>([]);

  // History: array of (board encoding, move notation, player)
  const [boardHistory, setBoardHistory] = useState<bigint[]>([]);
  const [moveHistory, setMoveHistory] = useState<MoveNotation[]>([]);
  const [historyIndex, setHistoryIndex] = useState(0);

  // UI state
  const [selection, setSelection] = useState<Selection | null>(null);
  const [hoveredMove, setHoveredMove] = useState<LegalMove | null>(null);
  const [showExportImport, setShowExportImport] = useState(false);
  const [notationText, setNotationText] = useState("");
  const [stateText, setStateText] = useState("");
  const [exportTab, setExportTab] = useState<"game" | "state">("game");

  // Initialize WASM
  useEffect(() => {
    init().then(() => {
      setWasmReady(true);
    }).catch((err) => {
      setError(`Failed to load WASM: ${err}`);
    });
  }, []);

  // Initialize game after WASM is ready
  useEffect(() => {
    if (!wasmReady) return;

    const board = new WasmBoard();
    boardRef.current = board;
    setBoardHistory([board.toU64()]);
    setMoveHistory([]);
    setHistoryIndex(0);
    updateGameState(board, 0, false, false);
  }, [wasmReady]);

  // Update game state and fetch evaluations
  const updateGameState = useCallback(async (
    board: WasmBoard,
    index: number,
    canUndo: boolean,
    canRedo: boolean
  ) => {
    setGameState(boardToGameState(board, index, canUndo, canRedo));

    if (board.result() === "ongoing") {
      const moves = getLegalMoves(board);

      // Show moves immediately without evaluations
      setLegalMoves(moves);

      // Get canonical position for this board to detect stale responses
      const currentCanonical = board.canonical();

      // Fetch evaluations for all moves
      const childPositions = moves.map((move) => {
        const child = board.clone();
        child.applyMove(
          move.to_pos[0],
          move.to_pos[1],
          move.from_pos?.[0] ?? null,
          move.from_pos?.[1] ?? null,
          move.size
        );
        const canonical = child.canonical();
        child.free();
        return canonical;
      });

      const evaluations = await lookupPositions(childPositions);

      // Only update if we're still on the same position (avoid stale updates)
      const boardNow = boardRef.current;
      if (boardNow && boardNow.canonical() === currentCanonical) {
        // Attach evaluations to moves
        const movesWithEval = moves.map((move, i) => ({
          ...move,
          evaluation: evaluations[i] ?? undefined,
        }));
        setLegalMoves(movesWithEval);
      }
    } else {
      setLegalMoves([]);
    }
  }, []);

  // Apply a move
  const applyMove = useCallback((move: LegalMove) => {
    const board = boardRef.current;
    if (!board || board.result() !== "ongoing") return;

    const success = board.applyMove(
      move.to_pos[0],
      move.to_pos[1],
      move.from_pos?.[0] ?? null,
      move.from_pos?.[1] ?? null,
      move.size
    );

    if (!success) {
      setError("Invalid move");
      return;
    }

    // Truncate future history if we're not at the end
    const newBoardHistory = boardHistory.slice(0, historyIndex + 1);
    const newMoveHistory = moveHistory.slice(0, historyIndex);

    // Add new state
    newBoardHistory.push(board.toU64());
    newMoveHistory.push({
      notation: formatMoveNotation(move),
      player: gameState!.current_player,
    });

    const newIndex = newBoardHistory.length - 1;
    setBoardHistory(newBoardHistory);
    setMoveHistory(newMoveHistory);
    setHistoryIndex(newIndex);
    setSelection(null);
    setHoveredMove(null);

    updateGameState(board, newIndex, newIndex > 0, false);
  }, [boardHistory, moveHistory, historyIndex, gameState, updateGameState]);

  // Handle reserve piece selection
  const handleReserveSelect = (player: 1 | 2, size: 1 | 2 | 3) => {
    if (gameState?.current_player !== player) return;
    if (gameState?.result !== "ongoing") return;

    if (
      selection?.type === "reserve" &&
      selection.player === player &&
      selection.size === size
    ) {
      setSelection(null);
    } else {
      setSelection({ type: "reserve", player, size });
    }
  };

  // Handle board cell click
  const handleCellClick = (row: number, col: number) => {
    if (!gameState || gameState.result !== "ongoing") return;

    const cell = gameState.board[row][col];

    // If we have a selection, try to make a move
    if (selection) {
      const validMove = legalMoves.find((move) => {
        if (move.to_pos[0] !== row || move.to_pos[1] !== col) return false;

        if (selection.type === "reserve") {
          return move.from_pos === null && move.size === selection.size;
        } else {
          return (
            move.from_pos !== null &&
            move.from_pos[0] === selection.row &&
            move.from_pos[1] === selection.col
          );
        }
      });

      if (validMove) {
        applyMove(validMove);
        return;
      }
    }

    // Otherwise, try to select a piece on the board
    const top = cell.stack[cell.stack.length - 1];
    if (top && top.player === gameState.current_player) {
      if (
        selection?.type === "board" &&
        selection.row === row &&
        selection.col === col
      ) {
        setSelection(null);
      } else {
        setSelection({ type: "board", row, col });
      }
    } else {
      setSelection(null);
    }
  };

  // Handle reset
  const handleReset = () => {
    const board = new WasmBoard();
    boardRef.current = board;
    setBoardHistory([board.toU64()]);
    setMoveHistory([]);
    setHistoryIndex(0);
    setSelection(null);
    setHoveredMove(null);
    updateGameState(board, 0, false, false);
  };

  // Handle undo
  const handleUndo = () => {
    if (historyIndex <= 0) return;

    const newIndex = historyIndex - 1;
    const board = WasmBoard.fromU64(boardHistory[newIndex]);
    boardRef.current = board;
    setHistoryIndex(newIndex);
    setSelection(null);
    setHoveredMove(null);
    updateGameState(board, newIndex, newIndex > 0, true);
  };

  // Handle redo
  const handleRedo = () => {
    if (historyIndex >= boardHistory.length - 1) return;

    const newIndex = historyIndex + 1;
    const board = WasmBoard.fromU64(boardHistory[newIndex]);
    boardRef.current = board;
    setHistoryIndex(newIndex);
    setSelection(null);
    setHoveredMove(null);
    updateGameState(board, newIndex, true, newIndex < boardHistory.length - 1);
  };

  // Handle goto move
  const handleGoto = (index: number) => {
    if (index < 0 || index >= boardHistory.length) return;

    const board = WasmBoard.fromU64(boardHistory[index]);
    boardRef.current = board;
    setHistoryIndex(index);
    setSelection(null);
    setHoveredMove(null);
    updateGameState(board, index, index > 0, index < boardHistory.length - 1);
  };

  // Handle clicking a move in the MovesPanel
  const handleMoveClick = (move: LegalMove) => {
    if (!gameState || gameState.result !== "ongoing") return;
    applyMove(move);
  };

  // Handle opening export/import modal
  const handleOpenExportImport = () => {
    // Generate notation from move history
    // If we started from a non-initial position, prepend FROM:<encoding>
    const initialEncoding = boardHistory[0];
    const isInitialPosition = initialEncoding === BigInt(0); // Empty board encoding

    let notation = moveHistory.map((m) => m.notation).join(" ");
    if (!isInitialPosition && moveHistory.length > 0) {
      notation = `FROM:${initialEncoding} ${notation}`;
    } else if (!isInitialPosition) {
      // No moves played, just show the FROM prefix
      notation = `FROM:${initialEncoding}`;
    }

    setNotationText(notation);
    setStateText(boardRef.current?.toU64().toString() ?? "");
    setShowExportImport(true);
  };

  // Handle state import (board position)
  const handleImportState = () => {
    try {
      const encoding = BigInt(stateText.trim());
      const board = WasmBoard.fromU64(encoding);
      boardRef.current = board;
      setBoardHistory([encoding]);
      setMoveHistory([]);
      setHistoryIndex(0);
      setShowExportImport(false);
      setSelection(null);
      setHoveredMove(null);
      updateGameState(board, 0, false, false);
    } catch {
      setError("Invalid state encoding");
    }
  };

  // Handle game import (move notation)
  const handleImportGame = () => {
    try {
      let text = notationText.trim();
      if (text.length === 0) {
        setError("No moves to import");
        return;
      }

      // Check for FROM:<encoding> prefix
      let board: WasmBoard;
      let initialEncoding: bigint;

      const fromMatch = text.match(/^FROM:(\d+)\s*/);
      if (fromMatch) {
        // Start from specified position
        initialEncoding = BigInt(fromMatch[1]);
        board = WasmBoard.fromU64(initialEncoding);
        text = text.slice(fromMatch[0].length); // Remove the FROM: prefix
      } else {
        // Start from fresh board
        board = new WasmBoard();
        initialEncoding = board.toU64();
      }

      const newBoardHistory: bigint[] = [initialEncoding];
      const newMoveHistory: MoveNotation[] = [];

      // Parse remaining moves (if any)
      const notations = text.split(/\s+/).filter(Boolean);

      for (const notation of notations) {
        const parsed = parseMoveNotation(notation);
        if (!parsed) {
          setError(`Invalid notation: ${notation}`);
          return;
        }

        const currentPlayer = board.currentPlayer() as 1 | 2;
        const success = board.applyMove(
          parsed.to_pos[0],
          parsed.to_pos[1],
          parsed.from_pos?.[0] ?? null,
          parsed.from_pos?.[1] ?? null,
          parsed.size
        );

        if (!success) {
          setError(`Illegal move: ${notation}`);
          return;
        }

        newBoardHistory.push(board.toU64());
        newMoveHistory.push({ notation, player: currentPlayer });
      }

      // Apply the imported game
      boardRef.current = board;
      setBoardHistory(newBoardHistory);
      setMoveHistory(newMoveHistory);
      setHistoryIndex(newBoardHistory.length - 1);
      setShowExportImport(false);
      setSelection(null);
      setHoveredMove(null);
      updateGameState(
        board,
        newBoardHistory.length - 1,
        newBoardHistory.length > 1,
        false
      );
    } catch (e) {
      setError(`Import failed: ${e}`);
    }
  };

  // Build History object for History component
  const historyData: HistoryType = {
    moves: moveHistory.map((m, i) => ({
      index: i + 1,
      notation: m.notation,
      player: m.player,
    })),
    current_index: historyIndex,
    total_moves: moveHistory.length,
  };

  // Loading state
  if (!wasmReady) {
    return <div className="app loading">Loading WASM...</div>;
  }

  if (error) {
    return (
      <div className="app error">
        <p>{error}</p>
        <button onClick={() => { setError(null); handleReset(); }}>Reset</button>
      </div>
    );
  }

  if (!gameState) {
    return <div className="app loading">Initializing...</div>;
  }

  return (
    <div className="app">
      <h1>Gobblet Gobblers</h1>

      <div className="game-layout">
        <div className="left-column">
          <History
            history={historyData}
            onGoto={handleGoto}
            onUndo={handleUndo}
            onRedo={handleRedo}
            onReset={handleReset}
            onExportImport={handleOpenExportImport}
            canUndo={historyIndex > 0}
            canRedo={historyIndex < boardHistory.length - 1}
          />
        </div>

        <div className="center-column">
          <Board
            board={gameState.board}
            currentPlayer={gameState.current_player}
            selection={selection}
            legalMoves={legalMoves}
            hoveredMove={hoveredMove}
            onCellClick={handleCellClick}
            winningLine={gameState.winning_line ?? null}
          />
          <div className="reserves-row">
            <Reserves
              player={1}
              reserves={gameState.reserves["1"]}
              isCurrentPlayer={gameState.current_player === 1}
              selection={selection}
              hoveredMove={hoveredMove}
              onSelect={(size) => handleReserveSelect(1, size)}
            />
            <Reserves
              player={2}
              reserves={gameState.reserves["2"]}
              isCurrentPlayer={gameState.current_player === 2}
              selection={selection}
              hoveredMove={hoveredMove}
              onSelect={(size) => handleReserveSelect(2, size)}
            />
          </div>
        </div>

        <MovesPanel
          moves={legalMoves}
          currentPlayer={gameState.current_player}
          onMoveClick={handleMoveClick}
          onMoveHover={setHoveredMove}
          result={gameState.result}
        />
      </div>

      {showExportImport && (
        <div className="modal-overlay" onClick={() => setShowExportImport(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">Export / Import</div>
            <div className="modal-tabs">
              <button
                className={`tab ${exportTab === "game" ? "active" : ""}`}
                onClick={() => setExportTab("game")}
              >
                Game (Moves)
              </button>
              <button
                className={`tab ${exportTab === "state" ? "active" : ""}`}
                onClick={() => setExportTab("state")}
              >
                State (Position)
              </button>
            </div>

            {exportTab === "game" ? (
              <>
                <div className="modal-description">
                  Move history notation (e.g., "S(0,0) L(1,1) M(2,2)")
                </div>
                <textarea
                  className="notation-input"
                  value={notationText}
                  onChange={(e) => setNotationText(e.target.value)}
                  placeholder="Paste notation here to import, or copy to export"
                  rows={4}
                />
                <div className="modal-buttons">
                  <button onClick={() => navigator.clipboard.writeText(notationText)}>Copy</button>
                  <button onClick={handleImportGame}>Load</button>
                  <button onClick={() => setShowExportImport(false)}>Close</button>
                </div>
              </>
            ) : (
              <>
                <div className="modal-description">
                  Board state as 64-bit encoding (e.g., "18014398509481984")
                </div>
                <textarea
                  className="notation-input"
                  value={stateText}
                  onChange={(e) => setStateText(e.target.value)}
                  placeholder="Paste encoding here to import, or copy to export"
                  rows={2}
                />
                <div className="modal-buttons">
                  <button onClick={() => navigator.clipboard.writeText(stateText)}>Copy</button>
                  <button onClick={handleImportState}>Load</button>
                  <button onClick={() => setShowExportImport(false)}>Close</button>
                </div>
              </>
            )}
          </div>
        </div>
      )}

      {selection && (
        <div className="selection-hint">
          Selected:{" "}
          {selection.type === "reserve"
            ? `Player ${selection.player} ${["Small", "Medium", "Large"][selection.size - 1]} from reserve`
            : `Piece at (${selection.row}, ${selection.col})`}
        </div>
      )}
    </div>
  );
}

export default App;
