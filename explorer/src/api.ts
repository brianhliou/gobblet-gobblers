// Tablebase API client
// This is the only backend call needed - everything else is handled by WASM
// VITE_API_URL points at the Railway tablebase probe (falls back to /api for local dev).
const API_BASE = import.meta.env.VITE_API_URL ?? "/api";

export interface PositionEval {
  /** 1 = P1 wins, 0 = draw, -1 = P2 wins, null = not found */
  evaluation: number | null;
  /** Plies to the result from this position (0 for terminal/draw), null = not found */
  dtm: number | null;
}

/**
 * Batch lookup for position evaluations + distance-to-result from the tablebase.
 * @param positions - Array of canonical position encodings
 */
export async function lookupPositions(positions: bigint[]): Promise<PositionEval[]> {
  const empty = (): PositionEval[] => positions.map(() => ({ evaluation: null, dtm: null }));
  if (positions.length === 0) return [];

  try {
    const res = await fetch(`${API_BASE}/lookup/batch`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ positions: positions.map(p => p.toString()) }),
    });
    if (!res.ok) return empty();
    const data = await res.json();
    const evals: (number | null)[] = data.evaluations ?? [];
    const dtms: (number | null)[] = data.dtm ?? [];
    return positions.map((_, i) => ({
      evaluation: evals[i] ?? null,
      dtm: dtms[i] ?? null,
    }));
  } catch {
    return empty();
  }
}
