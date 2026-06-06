/* tslint:disable */
/* eslint-disable */

export class WasmBoard {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Apply a move. Returns true if successful.
   * For placement: apply(toRow, toCol, null, null, size)
   * For slide: apply(toRow, toCol, fromRow, fromCol, null)
   */
  applyMove(to_row: number, to_col: number, from_row?: number | null, from_col?: number | null, size?: number | null): boolean;
  /**
   * Get cell stack at position as array of [player, size, player, size, ...]
   * Bottom to top order
   */
  cellStack(row: number, col: number): Uint8Array;
  /**
   * Clone the board
   */
  clone(): WasmBoard;
  /**
   * Get legal moves as JSON array
   * Each move is { to: [row, col], from: [row, col] | null, size: 1|2|3 | null }
   */
  legalMoves(): any;
  /**
   * Check for winner. Returns 0 (none), 1 (P1), or 2 (P2)
   */
  checkWinner(): number;
  /**
   * Check if game is over (has winner or no legal moves)
   */
  isGameOver(): boolean;
  /**
   * Get winning line as array of positions [row, col, row, col, row, col]
   * Returns empty array if no winner
   */
  winningLine(): Uint8Array;
  /**
   * Current player (1 or 2)
   */
  currentPlayer(): number;
  /**
   * Create a new empty board
   */
  constructor();
  /**
   * Get game result: "ongoing", "player_one_wins", "player_two_wins", or "draw"
   */
  result(): string;
  /**
   * Get u64 encoding of board
   */
  toU64(): bigint;
  /**
   * Create board from u64 encoding
   */
  static fromU64(bits: bigint): WasmBoard;
  /**
   * Get reserves for a player as [small, medium, large]
   */
  reserves(player: number): Uint8Array;
  /**
   * Get canonical position encoding (for tablebase lookups)
   */
  canonical(): bigint;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
  readonly memory: WebAssembly.Memory;
  readonly __wbg_wasmboard_free: (a: number, b: number) => void;
  readonly wasmboard_applyMove: (a: number, b: number, c: number, d: number, e: number, f: number) => number;
  readonly wasmboard_canonical: (a: number) => bigint;
  readonly wasmboard_cellStack: (a: number, b: number, c: number) => [number, number];
  readonly wasmboard_checkWinner: (a: number) => number;
  readonly wasmboard_clone: (a: number) => number;
  readonly wasmboard_currentPlayer: (a: number) => number;
  readonly wasmboard_fromU64: (a: bigint) => number;
  readonly wasmboard_isGameOver: (a: number) => number;
  readonly wasmboard_legalMoves: (a: number) => any;
  readonly wasmboard_new: () => number;
  readonly wasmboard_reserves: (a: number, b: number) => [number, number];
  readonly wasmboard_result: (a: number) => [number, number];
  readonly wasmboard_toU64: (a: number) => bigint;
  readonly wasmboard_winningLine: (a: number) => [number, number];
  readonly __wbindgen_malloc: (a: number, b: number) => number;
  readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
  readonly __wbindgen_externrefs: WebAssembly.Table;
  readonly __wbindgen_free: (a: number, b: number, c: number) => void;
  readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
* Instantiates the given `module`, which can either be bytes or
* a precompiled `WebAssembly.Module`.
*
* @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
*
* @returns {InitOutput}
*/
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
* If `module_or_path` is {RequestInfo} or {URL}, makes a request and
* for everything else, calls `WebAssembly.instantiate` directly.
*
* @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
*
* @returns {Promise<InitOutput>}
*/
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
