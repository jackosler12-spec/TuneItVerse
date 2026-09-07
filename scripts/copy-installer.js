import { existsSync, copyFileSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const projectRoot = join(__dirname, '..');
const destExe = join(projectRoot, 'TuneItVerse.exe');

const appCandidates = [
  join(projectRoot, 'src-tauri', 'target', 'release', 'TuneItVerse.exe'),
  join(projectRoot, 'src-tauri', 'target', 'x86_64-pc-windows-msvc', 'release', 'TuneItVerse.exe'),
];

const appSrc = appCandidates.find((p) => existsSync(p));
if (!appSrc) {
  console.error('TuneItVerse.exe not found under src-tauri/target/release. Run cargo build --release or npm run build first.');
  process.exit(1);
}

copyFileSync(appSrc, destExe);
console.log('Copied app: ' + appSrc);
console.log('        to: ' + destExe);
