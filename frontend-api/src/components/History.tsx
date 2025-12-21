import type { History as HistoryType } from "../types";
import "./History.css";

interface HistoryProps {
  history: HistoryType;
  onGoto: (index: number) => void;
  onUndo: () => void;
  onRedo: () => void;
  onReset: () => void;
  onExportImport: () => void;
  canUndo: boolean;
  canRedo: boolean;
}

export function History({
  history,
  onGoto,
  onUndo,
  onRedo,
  onReset,
  onExportImport,
  canUndo,
  canRedo,
}: HistoryProps) {
  return (
    <div className="history-panel">
      <div className="history-header">Move History</div>

      <div className="history-controls">
        <button onClick={onUndo} disabled={!canUndo} title="Undo (go back one move)">
          ← Undo
        </button>
        <button onClick={onRedo} disabled={!canRedo} title="Redo (go forward one move)">
          Redo →
        </button>
      </div>

      <div className="history-list">
        <div
          className={`history-entry ${history.current_index === 0 ? "current" : ""}`}
          onClick={() => onGoto(0)}
        >
          <span className="move-number">Start</span>
          <span className="move-notation">Initial position</span>
        </div>

        {history.moves.map((entry) => (
          <div
            key={entry.index}
            className={`history-entry player-${entry.player} ${history.current_index === entry.index ? "current" : ""}`}
            onClick={() => onGoto(entry.index)}
          >
            <span className="move-number">{entry.index}.</span>
            <span className="move-notation">{entry.notation}</span>
          </div>
        ))}
      </div>

      {history.moves.length === 0 && (
        <div className="history-empty">No moves yet</div>
      )}

      <div className="history-actions">
        <button onClick={onReset}>Reset</button>
        <button onClick={onExportImport}>Export / Import</button>
      </div>
    </div>
  );
}
