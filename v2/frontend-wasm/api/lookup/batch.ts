import { createClient } from '@libsql/client/web';

export const config = { runtime: 'edge' };

interface BatchRequest {
  positions: string[];
}

const db = createClient({
  url: process.env.TURSO_URL!,
  authToken: process.env.TURSO_AUTH_TOKEN!,
});

export default async function handler(req: Request) {
  if (req.method !== 'POST') {
    return new Response(JSON.stringify({ error: 'Method not allowed' }), {
      status: 405,
      headers: { 'Content-Type': 'application/json' },
    });
  }

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

    // Query each position
    const evaluations = await Promise.all(
      positions.map(async (canonical) => {
        try {
          const result = await db.execute({
            sql: 'SELECT outcome FROM positions WHERE canonical = ?',
            args: [canonical],
          });
          return result.rows[0]?.outcome ?? null;
        } catch {
          return null;
        }
      })
    );

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
