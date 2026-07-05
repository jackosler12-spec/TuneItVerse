import { existsSync, mkdirSync, copyFileSync, readdirSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const projectRoot = join(__dirname, '..');
const nsisDir = join(projectRoot, 'src-tauri', 'target', 'release', 'bundle', 'nsis');

const destDir = projectRoot;

if (!existsSync(nsisDir)) {
  console.error('NSIS bundle directory not found. Did you run "tauri build"?');
  process.exit(1);
}

const files = readdirSync(nsisDir).filter(f => f.endsWith('-setup.exe') || f.endsWith('.exe'));

if (files.length === 0) {
  console.error('No installer .exe found in NSIS bundle folder.');
  process.exit(1);
}

for (const file of files) {
  const src = join(nsisDir, file);
  const dest = join(destDir, file);
  copyFileSync(src, dest);
  console.log(`Copied: ${file} → ${destDir}`);
}

console.log('Installer(s) successfully copied to project root.');