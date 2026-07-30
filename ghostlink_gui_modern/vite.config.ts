import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import monacoEditorEsmPlugin from 'vite-plugin-monaco-editor-esm'

// The control-plane gateway (Go), not ghost-link directly — see config.ts's
// resolveApiBase for the matching default on the axios-client side.
const proxyTarget = process.env.VITE_PROXY_TARGET || 'http://127.0.0.1:8000'

export default defineConfig({
  // Bundles Monaco's language workers locally (esbuild, served from
  // node_modules/.monaco) instead of the CDN @monaco-editor/react defaults
  // to — Ghostlink is local-first everywhere else, so the Editor tab's code
  // editor shouldn't be the one piece that silently needs internet access.
  plugins: [react(), monacoEditorEsmPlugin()],
  server: {
    port: 5173,
    proxy: {
      '/api': {
        target: proxyTarget,
        changeOrigin: true,
        rewrite: (path) => path,
        configure: (proxy) => {
          proxy.on('proxyReq', (proxyReq) => {
            proxyReq.setTimeout(120000);
          });
        },
      },
      '/health': {
        target: proxyTarget,
        changeOrigin: true,
        configure: (proxy) => {
          proxy.on('proxyReq', (proxyReq) => {
            proxyReq.setTimeout(10000);
          });
        },
      },
      '/v1': {
        target: proxyTarget,
        changeOrigin: true,
        configure: (proxy) => {
          proxy.on('proxyReq', (proxyReq) => {
            proxyReq.setTimeout(120000);
          });
        },
      }
    }
  },
})