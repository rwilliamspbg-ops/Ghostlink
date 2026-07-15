import { existsSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const scriptPath = fileURLToPath(import.meta.url);
const frontendRoot = path.resolve(path.dirname(scriptPath), '..');
const viteBin = path.join(frontendRoot, 'node_modules', '.bin', process.platform === 'win32' ? 'vite.cmd' : 'vite');

if (existsSync(viteBin)) {
  process.exit(0);
}

console.log('[ghostlink-frontend] Missing local Vite binary. Installing dependencies with npm ci...');
const result = spawnSync('npm', ['ci'], {
  cwd: frontendRoot,
  stdio: 'inherit',
  shell: process.platform === 'win32',
});

if (result.status !== 0) {
  process.exit(result.status ?? 1);
}
