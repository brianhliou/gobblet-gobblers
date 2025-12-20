import { createClient } from '@libsql/client/web';

export const config = { runtime: 'edge' };

interface BatchRequest {
  positions: string[];
}

export default async function handler(req: Request) {
  if (req.method !== 'POST') {
    return new Response(JSON.stringify({ error: 'Method not allowed' }), {
      status: 405,
      headers: { 'Content-Type': 'application/json' },
    });
  }

  const db = createClient({
    url: process.env.TURSO_URL!,
    authToken: process.env.TURSO_AUTH_TOKEN!,
  });

  try {
    const { positions }: BatchRequest = await req.json();

    if (!positions || !Array.isArray(positions)) {
      return new Response(JSON.stringify({ error: 'positions array required' }), {
        status: 400,
        headers: { 'Content-Type': 'application/json' },
      });
    }

    if (positions.length === 0) {
      return new Response(JSON.stringify({ evaluations: [] }), {
        headers: { 'Content-Type': 'application/json' },
      });
    }

    // Single query with IN clause for all positions
    const placeholders = positions.map(() => '?').join(',');
    const result = await db.execute({
      sql: `SELECT canonical, outcome FROM positions WHERE canonical IN (${placeholders})`,
      args: positions,
    });

    // Build a map for fast lookup
    const resultMap = new Map<string, number>();
    for (const row of result.rows) {
      resultMap.set(String(row.canonical), row.outcome as number);
    }

    // Return evaluations in same order as input positions
    const evaluations = positions.map(pos => resultMap.get(pos) ?? null);

    return new Response(JSON.stringify({ evaluations }), {
      headers: { 'Content-Type': 'application/json' },
    });
  } catch (error) {
    return new Response(JSON.stringify({
      error: 'Internal server error',
      message: error instanceof Error ? error.message : String(error),
    }), {
      status: 500,
      headers: { 'Content-Type': 'application/json' },
    });
  }
}
