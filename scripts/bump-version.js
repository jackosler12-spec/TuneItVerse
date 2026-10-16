import { readFileSync, writeFileSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const projectRoot = join(__dirname, '..');

const newVersion = process.argv[2];

if (!newVersion) {
  console.error('Usage: node scripts/bump-version.js <new-version>');
  console.error('Example: node scripts/bump-version.js 0.3.0');
  process.exit(1);
}

const files = [
  { path: 'package.json', key: 'version' },
  { path: 'src-tauri/Cargo.toml', key: 'version', toml: true },
  { path: 'src-tauri/tauri.conf.json', key: 'version' },
];

for (const file of files) {
  const fullPath = join(projectRoot, file.path);
  let content = readFileSync(fullPath, 'utf8');

  if (file.toml) {
    content = content.replace(/^version = ".*?"$/m, `version = "${newVersion}"`);
  } else {
    const json = JSON.parse(content);
    json[file.key] = newVersion;
    content = JSON.stringify(json, null, 2) + '\n';
  }

  writeFileSync(fullPath, content);
  console.log(`Updated ${file.path} → ${newVersion}`);
}

// Cargo.lock must match Cargo.toml or CI's `cargo check --locked` fails
// (missed on the 3.43.0 bump).
const lockPath = join(projectRoot, 'src-tauri/Cargo.lock');
let lock = readFileSync(lockPath, 'utf8');
lock = lock.replace(/(name = "TuneItVerse"\r?\nversion = ")[^"]*(")/, `$1${newVersion}$2`);
writeFileSync(lockPath, lock);
console.log(`Updated src-tauri/Cargo.lock → ${newVersion}`);

// Keep installer + UI banners in lockstep (missed on the 3.43.0 bump).
const issPath = join(projectRoot, 'installer/TuneItVerse.iss');
let iss = readFileSync(issPath, 'utf8');
iss = iss.replace(/^AppVersion=.*$/m, `AppVersion=${newVersion}`);
writeFileSync(issPath, iss);
console.log(`Updated installer/TuneItVerse.iss → ${newVersion}`);

const htmlPath = join(projectRoot, 'src/index.html');
let html = readFileSync(htmlPath, 'utf8');
html = html.replace(/v\d+\.\d+\.\d+/g, `v${newVersion}`);
html = html.replace(/(TuneItVerse )\d+\.\d+\.\d+/g, `$1${newVersion}`);
writeFileSync(htmlPath, html);
console.log(`Updated src/index.html → ${newVersion}`);

const jsPath = join(projectRoot, 'src/main.js');
let js = readFileSync(jsPath, 'utf8');
js = js.replace(/TuneItVerse UI v\d+\.\d+\.\d+/g, `TuneItVerse UI v${newVersion}`);
writeFileSync(jsPath, js);
console.log(`Updated src/main.js → ${newVersion}`);

// Window title in tauri.conf
const tauriPath = join(projectRoot, 'src-tauri/tauri.conf.json');
let tauri = readFileSync(tauriPath, 'utf8');
tauri = tauri.replace(/TuneItVerse ECU Tuning Platform v[\d.]+/g, `TuneItVerse ECU Tuning Platform v${newVersion}`);
writeFileSync(tauriPath, tauri);
console.log(`Updated src-tauri/tauri.conf.json title → ${newVersion}`);

console.log(`\nVersion successfully bumped to ${newVersion} across all files.`);