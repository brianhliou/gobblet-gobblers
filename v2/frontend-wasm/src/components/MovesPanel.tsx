import type { LegalMove } from "../types";
import "./MovesPanel.css";

interface MovesPanelProps {
  moves: LegalMove[];
  currentPlayer: 1 | 2;
  onMoveClick: (move: LegalMove) => void;
  onMoveHover: (move: LegalMove | null) => void;
  result: "ongoing" | "player_one_wins" | "player_two_wins" | "draw";
}

function formatMove(move: LegalMove): string {
  if (move.from_pos === null) {
    // Reserve placement
    const sizeChar = { 1: "S", 2: "M", 3: "L" }[move.size!];
    return `${sizeChar}(${move.to_pos[0]},${move.to_pos[1]})`;
  } else {
    // Board move
    return `(${move.from_pos[0]},${move.from_pos[1]})→(${move.to_pos[0]},${move.to_pos[1]})`;
  }
}

function getMoveType(move: LegalMove): string {
  if (move.from_pos === null) {
    const sizeName = { 1: "Small", 2: "Medium", 3: "Large" }[move.size!];
    return `Reserve ${sizeName}`;
  } else {
    return "Board";
  }
}

/** Get evaluation display info for a move */
function getEvalInfo(evaluation: number | undefined, currentPlayer: 1 | 2): { label: string; className: string; title: string } {
  if (evaluation === undefined) {
    return { label: "?", className: "eval-unknown", title: "Position not in tablebase" };
  }

  // Evaluation is from the perspective of who's in the resulting position
  // If current player is P1 and eval is 1 (P1 wins), that's good for P1
  // If current player is P2 and eval is -1 (P2 wins), that's good for P2
  const isWinForCurrent =
    (currentPlayer === 1 && evaluation === 1) ||
    (currentPlayer === 2 && evaluation === -1);
  const isLossForCurrent =
    (currentPlayer === 1 && evaluation === -1) ||
    (currentPlayer === 2 && evaluation === 1);

  if (isWinForCurrent) {
    return { label: "W", className: "eval-win", title: "Winning move" };
  } else if (isLossForCurrent) {
    return { label: "L", className: "eval-loss", title: "Losing move" };
  } else {
    return { label: "D", className: "eval-draw", title: "Drawing move" };
  }
}

export function MovesPanel({ moves, currentPlayer, onMoveClick, onMoveHover, result }: MovesPanelProps) {
  // Group moves by type
  const reserveMoves = moves.filter((m) => m.from_pos === null);
  const boardMoves = moves.filter((m) => m.from_pos !== null);

  const gameOver = result !== "ongoing";

  // Get winner info for display
  const getWinnerInfo = () => {
    switch (result) {
      case "player_one_wins":
        return { winner: 1, message: "Player 1 Wins!" };
      case "player_two_wins":
        return { winner: 2, message: "Player 2 Wins!" };
      case "draw":
        return { winner: null, message: "Draw!" };
      default:
        return null;
    }
  };

  const winnerInfo = getWinnerInfo();

  return (
    <div className="moves-panel">
      {gameOver && winnerInfo ? (
        <div className={`game-result ${winnerInfo.winner ? `winner-p${winnerInfo.winner}` : "draw"}`}>
          <div className="result-icon">{winnerInfo.winner ? "🏆" : "🤝"}</div>
          <div className="result-message">{winnerInfo.message}</div>
        </div>
      ) : (
        <>
          <div className="moves-header">
            Legal Moves
            <span className="moves-count">({moves.length})</span>
          </div>

          {moves.length === 0 ? (
            <div className="moves-empty">No legal moves</div>
          ) : (
            <>
              <div className={`moves-subheader player-${currentPlayer}`}>Player {currentPlayer} to move</div>

              {reserveMoves.length > 0 && (
                <div className="moves-section">
                  <div className="section-header">From Reserve ({reserveMoves.length})</div>
                  <div className="moves-list">
                    {reserveMoves.map((move, idx) => {
                      const evalInfo = getEvalInfo(move.evaluation, currentPlayer);
                      return (
                        <div
                          key={`reserve-${idx}`}
                          className={`move-entry ${evalInfo.className}`}
                          onClick={() => onMoveClick(move)}
                          onMouseEnter={() => onMoveHover(move)}
                          onMouseLeave={() => onMoveHover(null)}
                          title={`${getMoveType(move)} - ${evalInfo.title}`}
                        >
                          <span className="move-notation">{formatMove(move)}</span>
                          <span className={`eval-badge ${evalInfo.className}`}>{evalInfo.label}</span>
                        </div>
                      );
                    })}
                  </div>
                </div>
              )}

              {boardMoves.length > 0 && (
                <div className="moves-section">
                  <div className="section-header">From Board ({boardMoves.length})</div>
                  <div className="moves-list">
                    {boardMoves.map((move, idx) => {
                      const evalInfo = getEvalInfo(move.evaluation, currentPlayer);
                      return (
                        <div
                          key={`board-${idx}`}
                          className={`move-entry ${evalInfo.className}`}
                          onClick={() => onMoveClick(move)}
                          onMouseEnter={() => onMoveHover(move)}
                          onMouseLeave={() => onMoveHover(null)}
                          title={`${getMoveType(move)} - ${evalInfo.title}`}
                        >
                          <span className="move-notation">{formatMove(move)}</span>
                          <span className={`eval-badge ${evalInfo.className}`}>{evalInfo.label}</span>
                        </div>
                      );
                    })}
                  </div>
                </div>
              )}
            </>
          )}
        </>
      )}
    </div>
  );
}
