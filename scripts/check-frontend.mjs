// Frontend sanity checks (run by `npm test` and CI). No browser needed.
// 1. every src/*.js parses           4. every bound button id exists in index.html
// 2. every <script src> exists       5. no duplicate ids; every nav view has a panel + title
// 3. every invoked command is registered in generate_handler!
// 6. version strings agree (package.json, Cargo.toml, Cargo.lock, tauri.conf.json, Inno, UI)
import { readFileSync, readdirSync, existsSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';
import { spawnSync } from 'child_process';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const rd = (p) => readFileSync(join(root, p), 'utf8');
const errors = [];
const fail = (m) => errors.push(m);

const jsFiles = readdirSync(join(root, 'src')).filter((f) => f.endsWith('.js'));
for (const f of jsFiles) {
  const r = spawnSync(process.execPath, ['--check', join(root, 'src', f)], { encoding: 'utf8' });
  if (r.status !== 0) fail(`syntax: src/${f}\n${r.stderr}`);
}

const html = rd('src/index.html');
const scripts = [...html.matchAll(/<script\s+src="([^"]+)"/g)].map((m) => m[1]);
for (const s of scripts) if (!existsSync(join(root, 'src', s))) fail(`index.html loads missing script ${s}`);
for (const f of jsFiles) {
  for (const m of rd(`src/${f}`).matchAll(/\.src\s*=\s*'([\w.-]+\.js)'/g)) {
    if (!existsSync(join(root, 'src', m[1]))) fail(`src/${f} injects missing script ${m[1]}`);
  }
}

const p3 = rd('src-tauri/src/commands_p3.rs');
const handler = p3.slice(p3.indexOf('generate_handler!['), p3.indexOf('])', p3.indexOf('generate_handler![')));
const registered = new Set(handler.split(/[\s,\[]+/).map((t) => t.replace(/^.*::/, '')).filter(Boolean));
const invoked = new Set();
for (const f of jsFiles) {
  for (const m of rd(`src/${f}`).matchAll(/(?:invokeCmd|\bcmd|\binvoke)\(\s*'([a-z0-9_]+)'/g)) invoked.add(`${m[1]}|${f}`);
}
for (const k of invoked) {
  const [name, f] = k.split('|');
  if (!registered.has(name)) fail(`src/${f} invokes '${name}', which is not in generate_handler!`);
}

const ids = [...html.matchAll(/\sid="([^"]+)"/g)].map((m) => m[1]);
const seen = new Set();
for (const id of ids) { if (seen.has(id)) fail(`duplicate id="${id}" in index.html`); seen.add(id); }
const main = rd('src/main.js');
const actions = main.slice(main.indexOf('const actions = {'), main.indexOf('};', main.indexOf('const actions = {')));
for (const m of actions.matchAll(/'(btn-[a-z0-9-]+)'\s*:/g)) {
  if (!seen.has(m[1])) fail(`main.js binds #${m[1]} but index.html has no such element`);
}
for (const f of ['v336.js', 'v337.js', 'v347.js']) {
  const text = rd(`src/${f}`);
  for (const m of text.matchAll(/'(btn-[a-z0-9-]+)'/g)) {
    if (!seen.has(m[1]) && !text.includes(`id="${m[1]}"`)) fail(`src/${f} binds #${m[1]} but index.html has no such element`);
  }
}
const titles = main.slice(main.indexOf('const PAGE_TITLES'), main.indexOf('};', main.indexOf('const PAGE_TITLES')));
for (const m of html.matchAll(/class="nav-item[^"]*" data-view="([a-z0-9-]+)"/g)) {
  if (!seen.has(`view-${m[1]}`)) fail(`nav item '${m[1]}' has no #view-${m[1]} panel`);
  if (!new RegExp(`\\b${m[1]}:\\s*\\[`).test(titles)) fail(`nav item '${m[1]}' has no PAGE_TITLES entry`);
}

const version = JSON.parse(rd('package.json')).version;
const want = {
  'src-tauri/Cargo.toml': rd('src-tauri/Cargo.toml').match(/^version = "([^"]+)"/m)?.[1],
  'src-tauri/Cargo.lock': rd('src-tauri/Cargo.lock').match(/name = "TuneItVerse"\r?\nversion = "([^"]+)"/)?.[1],
  'src-tauri/tauri.conf.json': JSON.parse(rd('src-tauri/tauri.conf.json')).version,
  'installer/TuneItVerse.iss': rd('installer/TuneItVerse.iss').match(/^AppVersion=(.*)$/m)?.[1]?.trim(),
};
for (const [f, v] of Object.entries(want)) if (v !== version) fail(`${f} version ${v} != package.json ${version}`);
for (const m of html.matchAll(/v(\d+\.\d+\.\d+)|TuneItVerse (\d+\.\d+\.\d+)/g)) {
  const v = m[1] || m[2];
  if (v !== version) fail(`index.html shows ${v}, package.json is ${version}`);
}

if (errors.length) {
  console.error(errors.map((e) => '✗ ' + e).join('\n'));
  console.error(`\n${errors.length} frontend check(s) failed`);
  process.exit(1);
}
console.log(`ok frontend: ${jsFiles.length} scripts parse, ${invoked.size} invokes registered, ${seen.size} ids unique, version ${version}`);
