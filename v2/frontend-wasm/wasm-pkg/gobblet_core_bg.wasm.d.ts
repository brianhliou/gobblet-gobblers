/* tslint:disable */
/* eslint-disable */
export const memory: WebAssembly.Memory;
export const __wbg_wasmboard_free: (a: number, b: number) => void;
export const wasmboard_applyMove: (a: number, b: number, c: number, d: number, e: number, f: number) => number;
export const wasmboard_canonical: (a: number) => bigint;
export const wasmboard_cellStack: (a: number, b: number, c: number) => [number, number];
export const wasmboard_checkWinner: (a: number) => number;
export const wasmboard_clone: (a: number) => number;
export const wasmboard_currentPlayer: (a: number) => number;
export const wasmboard_fromU64: (a: bigint) => number;
export const wasmboard_isGameOver: (a: number) => number;
export const wasmboard_legalMoves: (a: number) => any;
export const wasmboard_new: () => number;
export const wasmboard_reserves: (a: number, b: number) => [number, number];
export const wasmboard_result: (a: number) => [number, number];
export const wasmboard_toU64: (a: number) => bigint;
export const wasmboard_winningLine: (a: number) => [number, number];
export const __wbindgen_malloc: (a: number, b: number) => number;
export const __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
export const __wbindgen_externrefs: WebAssembly.Table;
export const __wbindgen_free: (a: number, b: number, c: number) => void;
export const __wbindgen_start: () => void;
