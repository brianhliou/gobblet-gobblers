import type { Cell, LegalMove, Piece, Selection } from "../types";
import "./Board.css";

interface BoardProps {
  board: Cell[][];
  currentPlayer: 1 | 2;
  selection: Selection | null;
  legalMoves: LegalMove[];
  hoveredMove: LegalMove | null;
  onCellClick: (row: number, col: number) => void;
  winningLine: [number, number][] | null;
}

function getPieceSizeClass(size: 1 | 2 | 3): string {
  return ["small", "medium", "large"][size - 1];
}

export function Board({
  board,
  currentPlayer,
  selection,
  legalMoves,
  hoveredMove,
  onCellClick,
  winningLine,
}: BoardProps) {
  const gameOver = winningLine !== null;
  // Check if a cell is a valid destination for current selection
  const isValidDestination = (row: number, col: number): boolean => {
    if (!selection) return false;

    return legalMoves.some((move) => {
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
  };

  // Check if a cell is selected
  const isSelected = (row: number, col: number): boolean => {
    return (
      selection?.type === "board" &&
      selection.row === row &&
      selection.col === col
    );
  };

  // Check if a cell has a piece owned by current player (selectable)
  const isSelectable = (cell: Cell): boolean => {
    const top = cell.stack[cell.stack.length - 1];
    return top !== undefined && top.player === currentPlayer;
  };

  // Check if a cell is the source of the hovered move
  const isHoverSource = (row: number, col: number): boolean => {
    if (!hoveredMove || hoveredMove.from_pos === null) return false;
    return hoveredMove.from_pos[0] === row && hoveredMove.from_pos[1] === col;
  };

  // Check if a cell is the destination of the hovered move
  const isHoverDestination = (row: number, col: number): boolean => {
    if (!hoveredMove) return false;
    return hoveredMove.to_pos[0] === row && hoveredMove.to_pos[1] === col;
  };

  // Check if a cell is part of the winning line
  const isWinningCell = (row: number, col: number): boolean => {
    if (!winningLine) return false;
    return winningLine.some(([r, c]) => r === row && c === col);
  };

  // Render a single piece
  const renderPiece = (piece: Piece, index: number) => (
    <div
      key={index}
      className={`piece player-${piece.player} ${getPieceSizeClass(piece.size)}`}
    />
  );

  return (
    <div className="board">
      {board.map((row, rowIdx) => (
        <div key={rowIdx} className="board-row">
          {row.map((cell, colIdx) => {
            const validDest = isValidDestination(rowIdx, colIdx);
            const selected = isSelected(rowIdx, colIdx);
            const selectable = !gameOver && isSelectable(cell);
            const hoverSource = isHoverSource(rowIdx, colIdx);
            const hoverDest = isHoverDestination(rowIdx, colIdx);
            const winningCell = isWinningCell(rowIdx, colIdx);

            return (
              <div
                key={colIdx}
                className={`cell ${validDest ? "valid-destination" : ""} ${selected ? "selected" : ""} ${selectable ? "selectable" : ""} ${hoverSource ? "hover-source" : ""} ${hoverDest ? "hover-destination" : ""} ${winningCell ? "winning-cell" : ""}`}
                onClick={() => onCellClick(rowIdx, colIdx)}
              >
                <div className="piece-stack">
                  {/* Render in reverse: top piece first (behind), inner pieces on top */}
                  {[...cell.stack].reverse().map((piece, idx) => renderPiece(piece, idx))}
                </div>
              </div>
            );
          })}
        </div>
      ))}
    </div>
  );
}
