import { existsSync, copyFileSync, readdirSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const projectRoot = join(__dirname, '..');
const nsisDir = join(projectRoot, 'src-tauri', 'target', 'release', 'bundle', 'nsis');

const destDir = projectRoot; // Place installer in the front/root folder of the repo

if (!existsSync(nsisDir)) {
  console.error('❌ NSIS bundle directory not found. Run "npm run build:win-exe" or "tauri build" first.');
  process.exit(1);
}

const files = readdirSync(nsisDir).filter(f => 
  f.toLowerCase().endsWith('-setup.exe') || 
  f.toLowerCase().endsWith('.exe')
);

if (files.length === 0) {
  console.error('❌ No installer .exe found in NSIS output folder.');
  process.exit(1);
}

console.log('📦 Copying installer(s) to repository root...');

for (const file of files) {
  const src = join(nsisDir, file);
  const dest = join(destDir, file);
  copyFileSync(src, dest);
  console.log(`   ✅ Copied: ${file}`);
}

console.log(`\n🎉 Installer(s) successfully placed in: ${destDir}`);
console.log('   You can now easily find and run the installer from the repo root.');