import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  optimizeDeps: {
    exclude: ['gobblet-core'],
  },
  server: {
    fs: {
      // Allow serving files from the gobblet-core package
      allow: ['.', '../core/pkg'],
    },
    // Local dev convenience: proxy the tablebase probe so the `/api` fallback in
    // src/api.ts works against a locally-running gobblet-api (see docs/deployment.md).
    // Production uses VITE_API_URL (Vercel env) and never hits this proxy.
    proxy: {
      '/api': {
        target: 'http://localhost:8080',
        changeOrigin: true,
        rewrite: (p) => p.replace(/^\/api/, ''),
      },
    },
  },
})
