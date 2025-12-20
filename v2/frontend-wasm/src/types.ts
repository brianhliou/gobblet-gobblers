// API types matching the backend models

export interface Piece {
  player: 1 | 2;
  size: 1 | 2 | 3; // 1=small, 2=medium, 3=large
}

export interface Cell {
  stack: Piece[]; // Bottom to top
}

export interface Reserves {
  small: number;
  medium: number;
  large: number;
}

export interface GameState {
  board: Cell[][];
  reserves: { "1": Reserves; "2": Reserves };
  current_player: 1 | 2;
  result: "ongoing" | "player_one_wins" | "player_two_wins" | "draw";
  move_index: number;
  can_undo: boolean;
  can_redo: boolean;
  /** The winning line positions, if there's a winner (not for zugzwang/draw) */
  winning_line?: [number, number][];
}

export interface LegalMove {
  to_pos: [number, number];
  from_pos: [number, number] | null;
  size: number | null;
  /** Tablebase evaluation: 1 = P1 wins, 0 = draw, -1 = P2 wins, undefined = unknown */
  evaluation?: number;
}

export interface HistoryEntry {
  index: number;
  notation: string;
  player: 1 | 2;
}

export interface History {
  moves: HistoryEntry[];
  current_index: number;
  total_moves: number;
}

export type Selection =
  | { type: "reserve"; player: 1 | 2; size: 1 | 2 | 3 }
  | { type: "board"; row: number; col: number };
