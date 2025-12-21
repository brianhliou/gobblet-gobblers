// Tablebase API client
// This is the only backend call needed - everything else is handled by WASM

// Uses /api path for Vercel serverless function (reads from tablebase.bin)
// For local dev with API: run `vercel dev` instead of `npm run dev`
const API_BASE = import.meta.env.VITE_API_URL ?? "/api";

/**
 * Batch lookup for position evaluations from tablebase
 * @param positions - Array of canonical position encodings
 * @returns Array of evaluations (1 = P1 wins, 0 = draw, -1 = P2 wins, null = not found)
 */
export async function lookupPositions(positions: bigint[]): Promise<(number | null)[]> {
  if (positions.length === 0) return [];

  try {
    const res = await fetch(`${API_BASE}/lookup/batch`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ positions: positions.map(p => p.toString()) }),
    });
    if (!res.ok) return positions.map(() => null);
    const data = await res.json();
    return data.evaluations ?? positions.map(() => null);
  } catch {
    return positions.map(() => null);
  }
}
