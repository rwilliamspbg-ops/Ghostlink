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
  //
  // vite-plugin-monaco-editor-esm's built-in worker list hardcodes each
  // entry with a "monaco-editor/esm/vs/..." path (languageWork.js) and feeds
  // it straight to esbuild. monaco-editor >=0.53 ships a package.json
  // "exports" map — "./*": "./esm/vs/*.js" — so esbuild now resolves
  // subpaths strictly through that map instead of probing the filesystem;
  // requesting the already-prefixed "esm/vs/editor/editor.worker" subpath
  // re-applies the map's own "esm/vs/" prefix, doubling it to
  // "esm/vs/esm/vs/editor/editor.worker.js", which doesn't exist ("Could
  // not resolve" at both build and dev time). The plugin has had no release
  // since (2.0.2 is latest) and there's no option to fix individual built-in
  // entries, only to replace the whole worker list — so disable the broken
  // defaults (`languageWorkers: []`) and resupply the same 5 workers via
  // `customWorkers` with the "esm/vs/" prefix stripped, which the exports
  // map re-adds itself and resolves to the real on-disk file.
  plugins: [
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
      // vite-plugin-monaco-editor-esm hardcodes worker entry points as
      // e.g. "monaco-editor/esm/vs/editor/editor.worker" (last updated
      // against monaco-editor ^0.52, which had no package.json "exports"
      // map, so that full path resolved directly off disk). monaco-editor
      // 0.56 added an exports map — "./*": "./esm/vs/*.js" — meant for
      // *shorter* subpaths like "monaco-editor/editor/editor.worker"; fed
      // the plugin's already-full path, it double-prefixes to
      // esm/vs/esm/vs/... and esbuild fails with "Could not resolve".
      // Strip the redundant esm/vs/ before it hits package resolution so
      // the exports map re-adds exactly one copy.
      { find: /^monaco-editor\/esm\/vs\/(.*)$/, replacement: 'monaco-editor/$1' },
    ],
  },
  server: {
    port: 5173,
    proxy: {
      '/api': {
        target: proxyTarget,
        changeOrigin: true,
        rewrite: (path) => path,
        configure: (proxy) => {
          proxy.on('proxyReq', (proxyReq) => {
            // Matches api.ts's toolCallTimeout (see the comment there) -
            // a slow/large local model plus multi-round tool-calling can
            // legitimately run past 120s, and this proxy leg was cutting
            // the connection out from under a client that was still
            // willing to wait.
            proxyReq.setTimeout(300000);
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
            proxyReq.setTimeout(300000);
          });
        },
      }
    }
  },
})