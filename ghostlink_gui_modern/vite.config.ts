import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

const proxyTarget = process.env.VITE_PROXY_TARGET || 'http://localhost:8003'

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      '/api': {
        target: proxyTarget,
        changeOrigin: true,
        rewrite: (path) => path,
      },
      '/health': {
        target: proxyTarget,
        changeOrigin: true,
      },
      '/v1': {
        target: proxyTarget,
        changeOrigin: true,
      }
    }
  },
})
