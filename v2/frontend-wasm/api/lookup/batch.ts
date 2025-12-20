import { readFileSync } from 'fs';
import { join } from 'path';

// Use Node.js runtime (not Edge) for file system access
export const config = { runtime: 'nodejs' };

interface BatchRequest {
  positions: string[];
}

// Cache tablebase in memory between invocations (warm function)
let tablebaseBuffer: Buffer | null = null;
let entryCount = 0;

function loadTablebase() {
  if (tablebaseBuffer) return;

  const binPath = join(process.cwd(), 'api', 'tablebase.bin');
  tablebaseBuffer = readFileSync(binPath);
  entryCount = tablebaseBuffer.length / 9; // 8 bytes canonical + 1 byte outcome
  console.log(`Loaded tablebase: ${entryCount} positions`);
}

function lookupPosition(canonical: bigint): number | null {
  if (!tablebaseBuffer) return null;

  // Binary search in sorted array
  let lo = 0;
  let hi = entryCount - 1;

  while (lo <= hi) {
    const mid = Math.floor((lo + hi) / 2);
    const offset = mid * 9;

    // Read 8-byte little-endian BigInt
    const key = tablebaseBuffer.readBigInt64LE(offset);

    if (key === canonical) {
      // Read 1-byte signed outcome
      return tablebaseBuffer.readInt8(offset + 8);
    } else if (key < canonical) {
      lo = mid + 1;
    } else {
      hi = mid - 1;
    }
  }

  return null; // Not found
}

export default function handler(req: { method: string; body: BatchRequest }, res: { status: (code: number) => { json: (data: unknown) => void }; json: (data: unknown) => void }) {
  if (req.method !== 'POST') {
    return res.status(405).json({ error: 'Method not allowed' });
  }

  // Load tablebase on first request (cold start)
  loadTablebase();

  const { positions } = req.body;

  if (!positions || !Array.isArray(positions)) {
    return res.status(400).json({ error: 'positions array required' });
  }

  if (positions.length === 0) {
    return res.json({ evaluations: [] });
  }

  // Lookup all positions
  const evaluations = positions.map(pos => lookupPosition(BigInt(pos)));

  return res.json({ evaluations });
}
