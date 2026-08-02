import * as monaco from 'monaco-editor';
import { loader } from '@monaco-editor/react';

// Ghostlink is a local-first app (see the speech-to-text tradeoff note in
// ChatTab.tsx) — @monaco-editor/react's default loader pulls Monaco from a
// jsdelivr CDN at runtime, which would silently fail offline. Pointing the
// loader at the `monaco-editor` package already in node_modules keeps the
// editor itself fully local; `vite-plugin-monaco-editor-esm` (see
// vite.config.ts) handles bundling its language workers the same way.
loader.config({ monaco });
