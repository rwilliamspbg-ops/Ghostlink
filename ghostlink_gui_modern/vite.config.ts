import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// Backend port configuration
// Change this if your backend runs on a different port
const BACKEND_PORT = process.env.BACKEND_PORT || '8003';

export default defineConfig({
  plugins: [react()],
  server: {
    port: 3000,
    host: '0.0.0.0',
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:8003',
        changeOrigin: true,
      },
      '/health': {
        target: 'http://127.0.0.1:8003',
        changeOrigin: true,
      },
    },
  },
  build: {
    outDir: 'dist',
    sourcemap: false,
  },
});