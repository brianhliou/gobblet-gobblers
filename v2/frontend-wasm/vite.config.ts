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
      allow: ['.', '../gobblet-core/pkg'],
    },
  },
})
