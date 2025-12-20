export const config = { runtime: 'edge' };

export default async function handler() {
  try {
    const { createClient } = await import('@libsql/client/web');

    const url = process.env.TURSO_URL || '';
    const token = process.env.TURSO_AUTH_TOKEN || '';

    // Show full URL (it's not secret, just the database endpoint)
    const urlInfo = {
      full: url,
      length: url.length,
      startsWithLibsql: url.startsWith('libsql://'),
      hasWhitespace: /\s/.test(url),
    };

    // Try to create client
    let clientStatus = 'not attempted';
    let clientError = null;
    try {
      const db = createClient({ url, authToken: token });
      clientStatus = 'created successfully';

      // Try a simple query
      const result = await db.execute('SELECT 1 as test');
      clientStatus = 'query succeeded';
    } catch (e) {
      clientStatus = 'failed';
      clientError = e instanceof Error ? e.message : String(e);
    }

    return new Response(JSON.stringify({
      urlInfo,
      hasToken: !!token,
      tokenLength: token.length,
      clientStatus,
      clientError,
    }), {
      headers: { 'Content-Type': 'application/json' },
    });
  } catch (error) {
    return new Response(JSON.stringify({
      status: 'import failed',
      error: error instanceof Error ? error.message : String(error),
    }), {
      status: 500,
      headers: { 'Content-Type': 'application/json' },
    });
  }
}
