// Regenerate reference/ecu_database/gm_osid_map.json from PcmHammer's public
// Apps/PcmLibrary/Misc/PcmInfo.cs (OSID switch). Facts only: OS ID -> PcmType.
//   node scripts/gen-gm-osid-map.mjs <path/to/PcmInfo.cs> <commit-sha>
import { readFileSync, writeFileSync } from 'fs';

const [src, sha] = process.argv.slice(2);
if (!src || !sha) {
  console.error('Usage: node scripts/gen-gm-osid-map.mjs <PcmInfo.cs> <commit-sha>');
  process.exit(1);
}
const lines = readFileSync(src, 'utf8').split(/\r?\n/);
const start = lines.findIndex((l) => /^\s*switch \(osid\)\s*$/.test(l));
if (start < 0) throw new Error('switch (osid) not found');
const map = {};
let pending = [];
let depth = 0;
for (let i = start + 1; i < lines.length; i++) {
  const l = lines[i];
  depth += (l.match(/\{/g) || []).length - (l.match(/\}/g) || []).length;
  if (depth <= 0 && i > start + 1) break;
  const c = l.match(/^\s*case (\d{6,8}):/);
  if (c) { pending.push(c[1]); continue; }
  const t = l.match(/PCMInfo\(PcmType\.(\w+)\)/);
  if (t) {
    for (const id of pending) {
      if (map[id] && map[id] !== t[1]) throw new Error(`OSID ${id} maps to ${map[id]} and ${t[1]}`);
      map[id] = t[1];
    }
    pending = [];
    continue;
  }
  if (/^\s*break;/.test(l)) pending = [];
}
const sorted = Object.fromEntries(Object.keys(map).sort((a, b) => +a - +b).map((k) => [k, map[k]]));
const counts = {};
Object.values(sorted).forEach((t) => { counts[t] = (counts[t] || 0) + 1; });
const out = {
  description: 'GM OS ID -> PCM hardware type, extracted from PcmHammer PcmInfo.cs (switch (osid)). Public source, not checked against a dump per OS ID except P01 12225074 (reference/LS1 PCM P01 12225074.bin).',
  source: `https://github.com/PcmHammer/PcmHammer/blob/${sha}/Apps/PcmLibrary/Misc/PcmInfo.cs`,
  verification: 'public_source',
  generator: 'scripts/gen-gm-osid-map.mjs',
  counts,
  osids: sorted,
};
writeFileSync(new URL('../reference/ecu_database/gm_osid_map.json', import.meta.url), JSON.stringify(out, null, 1) + '\n');
console.log('wrote', Object.keys(sorted).length, 'OS IDs', counts);
