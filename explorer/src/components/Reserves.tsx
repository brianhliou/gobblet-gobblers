import type { LegalMove, Reserves as ReservesType, Selection } from "../types";
import "./Reserves.css";

interface ReservesProps {
  player: 1 | 2;
  reserves: ReservesType;
  isCurrentPlayer: boolean;
  selection: Selection | null;
  hoveredMove: LegalMove | null;
  onSelect: (size: 1 | 2 | 3) => void;
}

const SIZE_KEYS: Record<1 | 2 | 3, keyof ReservesType> = {
  1: "small",
  2: "medium",
  3: "large",
};

export function Reserves({
  player,
  reserves,
  isCurrentPlayer,
  selection,
  hoveredMove,
  onSelect,
}: ReservesProps) {
  const sizes: (1 | 2 | 3)[] = [1, 2, 3];

  const isSelected = (size: 1 | 2 | 3): boolean => {
    return (
      selection?.type === "reserve" &&
      selection.player === player &&
      selection.size === size
    );
  };

  // Check if this reserve piece is the source of the hovered move
  const isHoverSource = (size: 1 | 2 | 3): boolean => {
    if (!hoveredMove || !isCurrentPlayer) return false;
    // Reserve placement move: from_pos is null and size matches
    return hoveredMove.from_pos === null && hoveredMove.size === size;
  };

  return (
    <div className={`reserves player-${player} ${isCurrentPlayer ? "active" : ""}`}>
      <div className="reserves-header">Player {player}</div>
      <div className="reserves-pieces">
        {sizes.map((size) => {
          const count = reserves[SIZE_KEYS[size]];
          const canSelect = isCurrentPlayer && count > 0;

          return (
            <div
              key={size}
              className={`reserve-piece ${canSelect ? "selectable" : ""} ${isSelected(size) ? "selected" : ""} ${isHoverSource(size) ? "hover-source" : ""} ${count === 0 ? "depleted" : ""}`}
              onClick={() => canSelect && onSelect(size)}
            >
              <div className="piece-preview-wrapper">
                <div className={`piece-preview player-${player} size-${size}`} />
              </div>
              <div className="piece-count">x{count}</div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
