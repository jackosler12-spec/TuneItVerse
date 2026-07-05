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
  { path: 'src-tauri/tauri.conf.json', key: 'version' }
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

console.log(`\nVersion successfully bumped to ${newVersion} across all files.`);