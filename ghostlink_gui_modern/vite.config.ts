import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import monacoEditorEsmPlugin from 'vite-plugin-monaco-editor-esm'
import fs from 'node:fs'
import path from 'node:path'
import type { Plugin } from 'vite'

// index.html loads /env-config.js before the app. That file is generated per
// deployment (Dockerfile entrypoint, launch-native.ps1) and gitignored, so on a
// fresh checkout it doesn't exist and the dev/preview server's SPA fallback
// would answer with index.html, which the browser then fails to parse as JS.
// Serve an empty stub in that case; a real generated file always wins.
function envConfigFallback(): Plugin {
  const stub = 'window._env_ = window._env_ || {};\n'
  const handler = (req: { url?: string }, res: any, next: () => void) => {
    if (req.url?.split('?')[0] !== '/env-config.js') return next()
    if (fs.existsSync(path.resolve(__dirname, 'public/env-config.js'))) return next()
    res.setHeader('Content-Type', 'application/javascript')
    res.setHeader('Cache-Control', 'no-store')
    res.end(stub)
  }
  return {
    name: 'ghostlink-env-config-fallback',
    configureServer(server) { server.middlewares.use(handler) },
    configurePreviewServer(server) { server.middlewares.use(handler) },
  }
}

// The control-plane gateway (Go), not ghost-link directly — see config.ts's
// resolveApiBase for the matching default on the axios-client side.
const proxyTarget = process.env.VITE_PROXY_TARGET || 'http://127.0.0.1:8000'

export default defineConfig({
  // Bundles Monaco's language workers locally (esbuild, served from
  // node_modules/.monaco) instead of the CDN @monaco-editor/react defaults
  // to — Ghostlink is local-first everywhere else, so the Editor tab's code
  // editor shouldn't be the one piece that silently needs internet access.
  plugins: [
    envConfigFallback(),
    react(),
    monacoEditorEsmPlugin({
      languageWorkers: [],
      customWorkers: [
        { label: 'editorWorkerService', entry: 'monaco-editor/editor/editor.worker' },
        { label: 'css', entry: 'monaco-editor/language/css/css.worker' },
        { label: 'html', entry: 'monaco-editor/language/html/html.worker' },
        { label: 'json', entry: 'monaco-editor/language/json/json.worker' },
        { label: 'typescript', entry: 'monaco-editor/language/typescript/ts.worker' },
      ],
    }),
  ],
  resolve: {
    alias: [
      { find: /^monaco-editor\/esm\/vs\/(.*)$/, replacement: 'monaco-editor/$1' },
    ],
  },
  build: {
    rollupOptions: {
      output: {
        manualChunks: (id) => {
          if (id.includes('monaco-editor')) {
            return 'monaco-editor';
          }
          if (id.includes('recharts')) {
            return 'recharts';
          }
        },
      },
    },
  },
  server: {
    port: 5173,
    proxy: {
      '/api': {
        target: proxyTarget,
        changeOrigin: true,
        secure: false,
        rewrite: (path) => path,
        configure: (proxy) => {
          proxy.on('proxyReq', (proxyReq) => {
            proxyReq.setTimeout(300000);
          });
        },
      },
      '/health': {
        target: proxyTarget,
        changeOrigin: true,
        secure: false,
        configure: (proxy) => {
          proxy.on('proxyReq', (proxyReq) => {
            proxyReq.setTimeout(10000);
          });
        },
      },
      '/v1': {
        target: proxyTarget,
        changeOrigin: true,
        secure: false,
        configure: (proxy) => {
          proxy.on('proxyReq', (proxyReq) => {
            proxyReq.setTimeout(300000);
          });
        },
      }
    }
  },
})
