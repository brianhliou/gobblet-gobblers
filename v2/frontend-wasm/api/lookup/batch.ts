import { createClient } from '@libsql/client/web';

export const config = { runtime: 'edge' };

interface BatchRequest {
  positions: string[];
}

export default async function handler(req: Request) {
  const url = process.env.TURSO_URL;
  const token = process.env.TURSO_AUTH_TOKEN;

  if (!url || !token) {
    return new Response(JSON.stringify({
      error: 'Missing env vars',
      hasUrl: !!url,
      hasToken: !!token,
    }), {
      status: 500,
      headers: { 'Content-Type': 'application/json' },
    });
  }

  let db;
  try {
    db = createClient({ url, authToken: token });
  } catch (error) {
    return new Response(JSON.stringify({
      error: 'Failed to create client',
      message: error instanceof Error ? error.message : String(error),
    }), {
      status: 500,
      headers: { 'Content-Type': 'application/json' },
    });
  }

  // Only allow POST
  if (req.method !== 'POST') {
    return new Response(JSON.stringify({ error: 'Method not allowed' }), {
      status: 405,
      headers: { 'Content-Type': 'application/json' },
    });
  }

  let body: BatchRequest;
  try {
    body = await req.json();
  } catch (error) {
    return new Response(JSON.stringify({
      error: 'Invalid JSON body',
      message: error instanceof Error ? error.message : String(error),
    }), {
      status: 400,
      headers: { 'Content-Type': 'application/json' },
    });
  }

  const { positions } = body;

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

  // Test with just the first position
  try {
    const result = await db.execute({
      sql: 'SELECT outcome FROM positions WHERE canonical = ?',
      args: [positions[0]],
    });

    // If we get here with one position, do the rest
    const evaluations: (number | null)[] = [result.rows[0]?.outcome as number ?? null];

    for (let i = 1; i < positions.length; i++) {
      try {
        const r = await db.execute({
          sql: 'SELECT outcome FROM positions WHERE canonical = ?',
          args: [positions[i]],
        });
        evaluations.push(r.rows[0]?.outcome as number ?? null);
      } catch {
        evaluations.push(null);
      }
    }

    return new Response(JSON.stringify({ evaluations }), {
      headers: { 'Content-Type': 'application/json' },
    });
  } catch (error) {
    return new Response(JSON.stringify({
      error: 'Database query failed',
      message: error instanceof Error ? error.message : String(error),
      stack: error instanceof Error ? error.stack : undefined,
    }), {
      status: 500,
      headers: { 'Content-Type': 'application/json' },
    });
  }
}
