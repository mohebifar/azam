// Loads the engine half of src.js (everything before the UI) into a sandbox and returns its `AT`
// namespace. The engine only touches `window`/`navigator` when GPU or script loading is used.
// Libraries that src.js loads from a CDN at runtime (e.g. Delaunator) can be passed in `globals`.
//
// `{ until: 'app' }` loads further, up to the React app: the UI base (file IO), the 3D and 2D
// windows, the tech pack and the pattern PDF. Those sections only touch the DOM, three.js or jsPDF
// when their functions run, so they load in the same sandbox.
import fs from 'node:fs';
import vm from 'node:vm';

const RULE = '/* ============================================================\n   ';
const MARKERS = {
  engine: `${RULE}ATELIER UI BASE`,
  app: `${RULE}ATELIER APP`,
};

export function loadEngine(srcPath, globals = {}, { until = 'engine' } = {}) {
  const src = fs.readFileSync(srcPath, 'utf8');
  const marker = MARKERS[until];
  if (!marker) throw new Error(`unknown section: ${until}`);
  const end = src.indexOf(marker);
  if (end < 0) throw new Error(`${until} marker not found in ${srcPath}`);
  const sandbox = { console, window: {}, navigator: {}, ...globals };
  vm.createContext(sandbox);
  vm.runInContext(`${src.slice(0, end)}\n;globalThis.AT = AT;`, sandbox, { filename: srcPath });
  return sandbox.AT;
}

// Small seeded PRNG (mulberry32) so golden files are reproducible.
export function rng(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

// Templates src.js drafts that azam no longer offers: left out of every recording.
export const REMOVED_TEMPLATES = new Set(['labadeh', 'aba']);

// The garment templates azam offers, in src.js's order.
export function templates(AT) {
  return AT.TEMPLATES.filter((t) => !REMOVED_TEMPLATES.has(t.id));
}

// Every piece of every garment template, with a stable name.
export function templatePieces(AT) {
  const out = [];
  for (const t of templates(AT)) {
    const doc = AT.fromTemplate(t.id, 'female');
    Object.values(doc.pieces)
      .sort((a, b) => (a.order || 0) - (b.order || 0))
      .forEach((pc, i) => out.push({ name: `${t.id}/${i}-${pc.name}`, pc }));
  }
  return out;
}
