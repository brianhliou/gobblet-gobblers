import { useCallback, useEffect, useState } from "react";
import {
  exportGame,
  exportState,
  getGame,
  getHistory,
  getMoves,
  gotoMove,
  importGame,
  importState,
  makeMove,
  redo,
  resetGame,
  undo,
} from "./api";
import { Board } from "./components/Board";
import { History } from "./components/History";
import { MovesPanel } from "./components/MovesPanel";
import { Reserves } from "./components/Reserves";
import type { GameState, History as HistoryType, LegalMove, Selection } from "./types";
import "./App.css";

function App() {
  const [gameState, setGameState] = useState<GameState | null>(null);
  const [legalMoves, setLegalMoves] = useState<LegalMove[]>([]);
  const [history, setHistory] = useState<HistoryType | null>(null);
  const [selection, setSelection] = useState<Selection | null>(null);
  const [hoveredMove, setHoveredMove] = useState<LegalMove | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [showExportImport, setShowExportImport] = useState(false);
  const [notationText, setNotationText] = useState("");
  const [stateText, setStateText] = useState("");
  const [exportTab, setExportTab] = useState<"game" | "state">("game");

  // Load game state, moves, and history
  const loadGame = useCallback(async () => {
    try {
      const [state, moves, hist] = await Promise.all([
        getGame(),
        getMoves(),
        getHistory(),
      ]);
      setGameState(state);
      setLegalMoves(moves);
      setHistory(hist);
      setError(null);
    } catch (err) {
      setError("Failed to connect to server. Is the backend running?");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadGame();
  }, [loadGame]);

  // Reload moves and history after state change
  const reloadAfterStateChange = async (newState: GameState) => {
    setGameState(newState);
    setSelection(null);
    setHoveredMove(null);

    if (newState.result === "ongoing") {
      const moves = await getMoves();
      setLegalMoves(moves);
    } else {
      setLegalMoves([]);
    }

    const hist = await getHistory();
    setHistory(hist);
  };

  // Handle reserve piece selection
  const handleReserveSelect = (player: 1 | 2, size: 1 | 2 | 3) => {
    if (gameState?.current_player !== player) return;
    if (gameState?.result !== "ongoing") return;

    // Toggle selection
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
  const handleCellClick = async (row: number, col: number) => {
    if (!gameState || gameState.result !== "ongoing") return;

    const cell = gameState.board[row][col];

    // If we have a selection, try to make a move
    if (selection) {
      // Check if this is a valid destination
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
        try {
          const moveData =
            selection.type === "reserve"
              ? { to_row: row, to_col: col, size: selection.size }
              : {
                  to_row: row,
                  to_col: col,
                  from_row: selection.row,
                  from_col: selection.col,
                };

          const newState = await makeMove(moveData);
          await reloadAfterStateChange(newState);
        } catch (err) {
          setError(err instanceof Error ? err.message : "Failed to make move");
        }
        return;
      }
    }

    // Otherwise, try to select a piece on the board
    const top = cell.stack[cell.stack.length - 1];
    if (top && top.player === gameState.current_player) {
      // Toggle selection
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
      // Clicked on empty or opponent's piece without valid selection
      setSelection(null);
    }
  };

  // Handle reset
  const handleReset = async () => {
    try {
      const state = await resetGame();
      await reloadAfterStateChange(state);
      setError(null);
    } catch (err) {
      setError("Failed to reset game");
    }
  };

  // Handle undo
  const handleUndo = async () => {
    try {
      const newState = await undo();
      await reloadAfterStateChange(newState);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to undo");
    }
  };

  // Handle redo
  const handleRedo = async () => {
    try {
      const newState = await redo();
      await reloadAfterStateChange(newState);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to redo");
    }
  };

  // Handle goto move
  const handleGoto = async (index: number) => {
    try {
      const newState = await gotoMove(index);
      await reloadAfterStateChange(newState);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to go to move");
    }
  };

  // Handle clicking a move in the MovesPanel
  const handleMoveClick = async (move: LegalMove) => {
    if (!gameState || gameState.result !== "ongoing") return;

    try {
      const moveData =
        move.from_pos === null
          ? { to_row: move.to_pos[0], to_col: move.to_pos[1], size: move.size ?? undefined }
          : {
              to_row: move.to_pos[0],
              to_col: move.to_pos[1],
              from_row: move.from_pos[0],
              from_col: move.from_pos[1],
            };

      const newState = await makeMove(moveData);
      await reloadAfterStateChange(newState);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to make move");
    }
  };

  // Handle opening export/import modal
  const handleOpenExportImport = async () => {
    try {
      const [notation, state] = await Promise.all([exportGame(), exportState()]);
      setNotationText(notation);
      setStateText(state);
      setShowExportImport(true);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load game data");
    }
  };

  // Handle game import (move history)
  const handleImportGame = async () => {
    try {
      const newState = await importGame(notationText);
      await reloadAfterStateChange(newState);
      setShowExportImport(false);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to import game");
    }
  };

  // Handle state import (board position)
  const handleImportState = async () => {
    try {
      const newState = await importState(stateText);
      await reloadAfterStateChange(newState);
      setShowExportImport(false);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to import state");
    }
  };

  if (loading) {
    return <div className="app loading">Loading...</div>;
  }

  if (error) {
    return (
      <div className="app error">
        <p>{error}</p>
        <button onClick={loadGame}>Retry</button>
      </div>
    );
  }

  if (!gameState || !history) {
    return <div className="app">No game state</div>;
  }

  return (
    <div className="app">
      <h1>Gobblet Gobblers</h1>

      <div className="game-layout">
        <div className="left-column">
          <History
            history={history}
            onGoto={handleGoto}
            onUndo={handleUndo}
            onRedo={handleRedo}
            onReset={handleReset}
            onExportImport={handleOpenExportImport}
            canUndo={gameState.can_undo}
            canRedo={gameState.can_redo}
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
