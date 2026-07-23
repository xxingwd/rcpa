import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

// https://vite.dev/config/
export default defineConfig({
  plugins: [
    react(),
    tailwindcss(),
  ],
  server: {
    port: 5173,
    proxy: {
      '/v1': {
        target: 'http://localhost:15000',
        changeOrigin: true,
      },
      '/health': {
        target: 'http://localhost:15000',
        changeOrigin: true,
      },
      '/stats': {
        target: 'http://localhost:15000',
        changeOrigin: true,
      },
    },
  },
})
