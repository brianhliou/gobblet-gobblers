export const config = { runtime: 'edge' };

export default async function handler() {
  try {
    // Try dynamic import to catch any import errors
    const { createClient } = await import('@libsql/client/web');

    // Check env vars
    const hasUrl = !!process.env.TURSO_URL;
    const hasToken = !!process.env.TURSO_AUTH_TOKEN;
    const urlPrefix = process.env.TURSO_URL?.substring(0, 20) || 'not set';

    return new Response(JSON.stringify({
      status: 'import succeeded',
      hasUrl,
      hasToken,
      urlPrefix,
      createClientType: typeof createClient,
    }), {
      headers: { 'Content-Type': 'application/json' },
    });
  } catch (error) {
    return new Response(JSON.stringify({
      status: 'import failed',
      error: error instanceof Error ? error.message : String(error),
      stack: error instanceof Error ? error.stack : undefined,
    }), {
      status: 500,
      headers: { 'Content-Type': 'application/json' },
    });
  }
}
