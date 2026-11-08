// Exports golden data for crates/azam-sim from the reference JavaScript engine in src.js:
// simulation settings (AT.simSettings / AT.simParams / AT.fricK / AT.grabAnchor / AT.settleTracker /
// AT.estimateParticles), AT.buildSim on template projects with the parametric avatar, CPU
// AT.step / AT.simStep frames, rebuilds that reuse the previous drape, AT.fitValues / AT.fitSummary,
// the untangler core and the skin-vertex vs cloth-triangle collision on synthetic data.
//
//   (cd tools/golden && npm install)
//   node tools/golden/export-sim.mjs [path/to/src.js] > crates/azam-sim/tests/golden/sim.json
//
// Ids: `AT.uid` is replaced by the `<prefix>_<n>` counter used by export-doc.mjs, so template
// projects match the Rust templates. Projects are written whole, so the Rust test reads them
// directly. Positions are Float32Array values written as doubles (exact).
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import Delaunator from 'delaunator';
import { loadEngine, templates } from './load-engine.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const AT = loadEngine(process.argv[2] || path.join(here, '../../src.js'), { Delaunator });

let seq = 0;
AT.uid = (p) => `${p || ''}_${++seq}`;
const snap = (v) => JSON.parse(JSON.stringify(v));
const arr = (a) => Array.from(a);
const newDoc = (tid, sex) => {
  seq = 0;
  const d = AT.normalize(AT.fromTemplate(tid, sex));
  d.meta.createdAt = 0;
  d.meta.updatedAt = 0;
  return d;
};

/* ---------- settings ---------- */
function settingsCases() {
  const stored = [
    {},
    { preset: 'draft' }, { preset: 'normal' }, { preset: 'fine' }, { preset: 'layered' }, { preset: 'final' },
    { quality: 'fast' }, { quality: 'complete' }, { quality: 'fine' }, { quality: 'bogus' }, { quality: 'normal', selfCollision: true },
    { preset: 'bogus', quality: 'layered' },
    { preset: 'fine', over: { substeps: 99, air: 0.5, pdScale: 0.1, framesPerStep: 2.6, selfCollision: false, fricModel: 'slide', autoStop: true, bogus: 3, gravity: null } },
    { quality: 'normal', selfCollision: false, air: 1.2, g: 0.5 },
    { preset: 'final', air: 0.8, over: { air: 2 } },
    { preset: 'layered', over: { fricModel: 'grip', ccFric: 7, settle: 1, maxTime: 500, dt: 0.001, ccEveryGPU: 3, ccIters: 0, clothThickness: 4, sewSpeed: 50, grabSpeed: 5000, ccRepair: false, untangle: false, iterations: 2.5 } },
    { preset: 'draft', over: { substeps: 8, gravity: 2.5, air: -1 } },
  ];
  return stored.map((sim) => {
    const st = AT.simSettings({ sim });
    return { sim, resolved: { preset: st.preset, v: st.v, base: st.base, over: st.over, modified: st.modified }, params: AT.simParams(st) };
  });
}

function fricKCases() {
  const out = [];
  const ps = [{ ccFricMode: 1, ccMuCap: 0.8, ccFric: 2 }, { ccFricMode: 0, ccMuCap: 1.5, ccFric: 2 }, { ccFricMode: 0, ccMuCap: 1.5, ccFric: 0.5 }];
  for (const P of ps) for (const [mu0, dn, tl] of [[0.3, 1, 0.1], [0.3, 1, 5], [0.45, 0.2, 0.05], [0.1, 0.5, 2], [0.9, 3, 1e-3]]) {
    out.push({ params: P, mu0, dn, tl, out: AT.fricK(P, mu0, dn, tl) });
  }
  return out;
}

function grabAnchorCases() {
  const sim = { grab: { idx: [], wt: [], rel: [], target: [10, 1000, 50] }, params: { grabSpeed: 700 } };
  const steps = [];
  const targets = [[10, 1000, 50], [40, 1000, 50], [40, 1100, 90], [40, 1100, 90], [-200, 900, 300], [-200, 900, 300], [-201, 900, 300]];
  targets.forEach((t, i) => { sim.grab.target = t; const dt = i === 3 ? 1 / 30 : 1 / 60; const an = AT.grabAnchor(sim, dt).slice(); steps.push({ target: t, dt, anchor: an }); });
  return { speed: 700, steps };
}

function settleCases() {
  const out = [];
  const run = (name, fn, n, thr) => {
    const T = AT.settleTracker(), pushes = [];
    for (let i = 0; i < n; i++) { const [t, E, gap] = fn(i); pushes.push([t, E, gap, thr, T.push(t, E, gap, thr) || null]); }
    out.push({ name, pushes });
  };
  run('decay', (i) => [i / 60, 400 * Math.exp(-i / 50), 0.5], 400, 14);
  run('plateau', (i) => [i / 60, 40 + 5 * Math.sin(i / 7), 1], 420, 14);
  run('open seams', (i) => [i / 60, 2, i < 200 ? 4 : 1], 300, 14);
  run('noisy', (i) => [i / 30, 10 + 30 * ((i * 7919) % 13) / 13, 0], 200, 20);
  return out;
}

function estimateCases() {
  const out = [];
  for (const t of templates(AT)) {
    if (t.id === 'blank') continue;
    const doc = newDoc(t.id, t.sex || 'female');
    out.push({ template: t.id, sex: t.sex || 'female', values: [1, 0.65, 1.3, 2.5].map((s) => [s, AT.estimateParticles(doc, s)]) });
  }
  return out;
}

/* ---------- simulations ---------- */
const every = (n, k) => { const o = []; for (let i = 0; i < n; i += k) o.push(i); return o; };

function buildOut(sim, full = true, orders = false) {
  const N = sim.N;
  const colors = (n, vertsOf) => { const cz = AT.GPU.colorize(n, N, vertsOf); return orders ? { ranges: cz.ranges, order: arr(cz.order) } : { ranges: cz.ranges }; };
  const S = sim.S, B = sim.B, SW = sim.SW, FA = sim.FA;
  const sum = (a) => { let s = 0; for (const v of a) s += v; return s; };
  return {
    N,
    recs: sim.recs.map((r) => ({ id: r.id, off: r.off, n: r.n, nB: r.mesh.nB, tris: r.mesh.tris.length / 3, pd: r.mesh.pd })),
    counts: { S: S.n, B: B.n, SW: SW.n, FA: FA.n, pins: sim.pins.length },
    sums: { SL: sum(S.L), Sr: sum(S.r), Scm: sum(S.cm), BL: sum(B.L), Br: sum(B.r), SWu: sum(SW.u) },
    stretch: every(S.n, 7).map((c) => [c, S.i[c], S.j[c], S.L[c], S.r[c], S.cm[c]]),
    bend: every(B.n, 7).map((c) => [c, B.i[c], B.j[c], B.L[c], B.r[c]]),
    folds: every(B.n, 1).filter((c) => B.r[c] < 0).map((c) => [c, B.i[c], B.j[c], B.L[c], B.r[c]]),
    sewing: every(SW.n, 1).map((c) => [SW.a[c], SW.b0[c], SW.b1[c], SW.u[c], SW.s[c]]),
    fasteners: every(FA.n, 1).map((c) => [FA.a[c], FA.b[c]]),
    pins: arr(sim.pins), pinPos: arr(sim.pinPos),
    particles: every(N, 5).map((q) => [q, sim.w[q], sim.w0[q], sim.damp[q], sim.margin[q], sim.rad[q], sim.fric[q], sim.layerOf[q], sim.pdOf[q], sim.thickOf[q], sim.pieceOf[q], sim.active[q], sim.pressure[q], sim.sewn[q]]),
    x: full ? arr(sim.x) : every(N, 5).map((q) => [q, sim.x[q * 3], sim.x[q * 3 + 1], sim.x[q * 3 + 2]]),
    coloring: {
      stretch: colors(S.n, (c, vs) => { vs[0] = S.i[c]; vs[1] = S.j[c]; return 2; }),
      bend: colors(B.n, (c, vs) => { vs[0] = B.i[c]; vs[1] = B.j[c]; return 2; }),
      fasteners: colors(FA.n, (c, vs) => { vs[0] = FA.a[c]; vs[1] = FA.b[c]; return 2; }),
      sewing: colors(SW.n, (c, vs) => { vs[0] = SW.a[c]; vs[1] = SW.b0[c]; vs[2] = SW.b1[c]; return 3; }),
    },
  };
}

function frameOut(sim, f, full) {
  const N = sim.N, x = sim.x;
  const lo = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity], mean = [0, 0, 0];
  for (let q = 0; q < N; q++) for (let c = 0; c < 3; c++) { const v = x[q * 3 + c]; if (v < lo[c]) lo[c] = v; if (v > hi[c]) hi[c] = v; mean[c] += v; }
  const o = {
    frame: f, energy: sim.energy, sewGap: sim.sewGap, gScale: sim.gScale, gP: arr(sim.gP), time: sim.time, frames: sim.frames,
    lo, hi, mean: mean.map((m) => m / N),
    sample: every(N, 11).map((q) => [q, x[q * 3], x[q * 3 + 1], x[q * 3 + 2], sim.v[q * 3], sim.v[q * 3 + 1], sim.v[q * 3 + 2]]),
  };
  if (full) { o.x = arr(sim.x); o.v = arr(sim.v); }
  return o;
}

function fitOut(sim, doc) {
  const out = {};
  for (const mode of ['strain', 'stress', 'fit', 'pressure']) {
    const v = AT.fitValues(sim, doc, mode);
    out[mode] = every(sim.N, 3).map((q) => [q, v[q]]);
    if (mode === 'fit') out.summary = AT.fitSummary(v, sim);
  }
  return out;
}

// the grab the 3D view starts on pointer-down (src.js Scene.onDown)
function grabAt(sim, q) {
  const r = sim.recs[sim.pieceOf[q]], R = 45, idx = [], wt = [], rel = [];
  const c = [sim.x[q * 3], sim.x[q * 3 + 1], sim.x[q * 3 + 2]];
  for (let i = 0; i < r.n; i++) { const k = (r.off + i) * 3, d = Math.hypot(sim.x[k] - c[0], sim.x[k + 1] - c[1], sim.x[k + 2] - c[2]); if (d < R) { idx.push(r.off + i); const f = 1 - d / R; wt.push(0.5 * f * f); rel.push(sim.x[k] - c[0], sim.x[k + 1] - c[1], sim.x[k + 2] - c[2]); } }
  sim.grab = { idx, wt, rel, target: c.slice() };
}

// runs `frames` frames; `script(sim, f)` runs before frame f (grab, release…)
function runFrames(sim, frames, { full = [], every: ev = 5, simStep = false, script = null } = {}) {
  const out = [];
  for (let f = 1; f <= frames; f++) {
    if (script) script(sim, f);
    if (simStep) AT.simStep(sim, 1 / 60); else AT.step(sim, 1 / 60);
    out.push(f % ev === 0 || full.includes(f) || f === frames ? frameOut(sim, f, full.includes(f)) : { frame: f, energy: sim.energy, sewGap: sim.sewGap, gScale: sim.gScale, time: sim.time });
  }
  return out;
}

const gridSum = (G) => { let s = 0; for (const v of G.data) s += v; return { nx: G.nx, ny: G.ny, nz: G.nz, g: G.g, x0: G.x0, y0: G.y0, z0: G.z0, sum: s }; };

function projectCase(name, doc, { frames = 0, full = [], params: extra = {}, simStep = false, script = null, rebuild = null, fit = false } = {}) {
  const av = AT.buildAvatar(doc.avatar);
  const params = Object.assign(AT.simParams(AT.simSettings(doc)), extra);
  const sim = AT.buildSim(doc, av, null, { params });
  const docSnap = snap(doc);
  const out = { name, doc: docSnap, extraParams: extra, grid: gridSum(av.grid), build: buildOut(sim, frames > 0, name === 'tee') };
  out.frames = runFrames(sim, frames, { full, simStep, script });
  if (sim.grab) sim.grab = null;
  if (fit) out.fit = fitOut(sim, doc);
  if (rebuild) out.rebuilds = rebuild(doc, av, sim, params);
  return out;
}

function teeRebuilds(doc, av, sim, params) {
  const out = [];
  const rec = (name, d, opts, s2) => ({
    name, doc: d, rearrange: !!opts.rearrange, rearrangeIds: opts.rearrangeIds || [], gScale: s2.gScale ?? null,
    N: s2.N, recs: s2.recs.map((r) => ({ id: r.id, off: r.off, n: r.n })), counts: { S: s2.S.n, B: s2.B.n, SW: s2.SW.n }, x: arr(s2.x),
  });
  // edited sleeve: remapped through the previous mesh; the other pieces are copied (the app keeps the gravity hold-back)
  const d2 = snap(doc);
  d2.pieces.pc_4.pts.forEach((p) => { if (p.id === 'uaR') { p.x += 6; p.y -= 3; } });
  const s2 = AT.buildSim(d2, av, sim, { params });
  if (sim.gScale != null) s2.gScale = sim.gScale;
  out.push(rec('edited sleeve', d2, {}, s2));
  // re-arranged sleeve: placed at its arrangement again
  out.push(rec('rearranged sleeve', snap(doc), { rearrangeIds: ['pc_5'] }, AT.buildSim(doc, av, sim, { params, rearrangeIds: ['pc_5'] })));
  // edited sleeve with rearrange: the edited piece goes back to its arrangement
  out.push(rec('edited sleeve, rearrange', d2, { rearrange: true }, AT.buildSim(d2, av, sim, { params, rearrange: true })));
  return out;
}

function varietyDoc() {
  const doc = newDoc('tee', 'female');
  const P = doc.pieces;
  // front: fold line across the chest, interfaced, graded in size L
  const bb = AT.bbox(AT.outline(P.pc_2.pts).poly);
  const yF = bb.y0 + (bb.y1 - bb.y0) * 0.45;
  P.pc_2.internals = [{ id: 'il_fold', pts: [{ x: bb.x0 + 40, y: yF }, { x: (bb.x0 + bb.x1) / 2, y: yF - 25, c: 1 }, { x: bb.x1 - 40, y: yF }], fold: 120 }];
  P.pc_2.strengthen = true;
  P.pc_2.grade = { hemL: { L: { dx: -12, dy: 8 } }, hemR: { L: { dx: 12, dy: 8 } }, uaL: { L: { dx: -10, dy: 0 } } };
  doc.activeSize = 'L';
  // back: denim, shrinkage, layer 1, a pin
  const den = AT.makeFabric('denim'); den.id = 'fab_denim'; doc.fabrics.fab_denim = den;
  P.pc_3.fabric = 'fab_denim'; P.pc_3.shrinkWeft = 95; P.pc_3.shrinkWarp = 98; P.pc_3.layer = 1;
  const bk = AT.bbox(AT.outline(P.pc_3.pts).poly);
  P.pc_3.pins = [{ x: (bk.x0 + bk.x1) / 2, y: bk.y0 + 60 }];
  // sleeves: elastic hem, pressure, a missing fabric
  P.pc_4.elastic = { hemL: 80 };
  P.pc_5.pressure = 2; P.pc_5.fabric = 'fab_missing';
  // neck rib frozen
  P.pc_6.state = 'frozen';
  // a switched-off swatch with a pin
  P.pc_off = { id: 'pc_off', name: 'Swatch', order: 9, fabric: 'fab_main', pd: 25, grain: 15, layer: 0, state: 'off', arr: { slot: 'back', off: 120, h: 600 }, pts: [{ id: 'o1', x: 0, y: 0 }, { id: 'o2', x: 120, y: 0 }, { id: 'o3', x: 120, y: 90 }, { id: 'o4', x: 0, y: 90 }], internals: [], pins: [{ x: 60, y: 45 }], sa: 10 };
  // a button on the front fastened to a hole on the back
  doc.buttons = { bt_1: { id: 'bt_1', p: 'pc_2', x: (bb.x0 + bb.x1) / 2, y: bb.y0 + 150, fasten: 'bh_1' } };
  doc.holes = { bh_1: { id: 'bh_1', p: 'pc_3', x: (bk.x0 + bk.x1) / 2, y: bk.y0 + 150 } };
  return doc;
}

function varietyScript(sim, f) {
  if (f === 4) grabAt(sim, sim.recs[0].off + 100);
  if (f >= 4 && f < 9 && sim.grab) { const t = sim.grab.target; sim.grab.target = [t[0] + 8, t[1] + 15, t[2] + 25]; }
  if (f === 9) sim.grab = null;
}

/* ---------- untangle ---------- */
function untangleCase() {
  seq = 0;
  const doc = AT.newProject('untangle', 'female');
  doc.meta.createdAt = 0; doc.meta.updatedAt = 0;
  const sq = (id, order) => ({ id, name: id, order, fabric: 'fab_main', pd: 20, grain: 0, layer: 0, state: 'active', arr: { slot: 'front', off: 60 }, pts: [{ id: 'a', x: 0, y: 0 }, { id: 'b', x: 300, y: 0 }, { id: 'c', x: 300, y: 300 }, { id: 'd', x: 0, y: 300 }], internals: [], pins: [], sa: 10 });
  doc.pieces = { pc_a: sq('pc_a', 0), pc_b: sq('pc_b', 1) };
  const av = AT.buildAvatar(doc.avatar);
  const sim = AT.buildSim(doc, av, null, { params: AT.simParams(AT.simSettings(doc)), projectOut: false });
  // A flat in z = 0; B tilted so its last 50 mm poke through A
  const [ra, rb] = sim.recs;
  for (let i = 0; i < ra.n; i++) { const k = (ra.off + i) * 3, u = ra.mesh.xy[2 * i], v = ra.mesh.xy[2 * i + 1]; sim.x[k] = u - 150 + 0.013 * v; sim.x[k + 1] = 1300 - v; sim.x[k + 2] = 0.002 * u * 0.5; }
  for (let i = 0; i < rb.n; i++) { const k = (rb.off + i) * 3, u = rb.mesh.xy[2 * i], v = rb.mesh.xy[2 * i + 1]; sim.x[k] = 0.3 * v - 40 + 0.01 * u; sim.x[k + 1] = 1290 - v * 0.9; sim.x[k + 2] = 0.4 * u - 100; }
  const S = AT.untangleSetup(sim);
  const r = AT.untangleCore(S, new Float32Array(sim.x));
  const marked = []; for (let q = 0; q < sim.N; q++) if (r.tang[q]) marked.push([q, r.tang[q]]);
  return {
    doc: snap(doc), x: arr(sim.x),
    setup: { N: S.N, nT: S.T.length / 4, nE: S.E.length / 2, cell: S.cell, adj: S.adjStart[S.N], cap: S.cap },
    result: { crossings: r.crossings, groups: r.groups, resolved: r.resolved, ambiguous: r.ambiguous, marked: r.marked, tangles: marked },
  };
}

/* ---------- skin vertices vs cloth triangles (synthetic) ---------- */
function bodyTriCase() {
  // a 7 x 7 cloth grid near y = 1000 over a bumpy skin patch below it
  const n = 7, N = n * n, x = new Float32Array(N * 3), w = new Float32Array(N);
  for (let j = 0; j < n; j++) for (let i = 0; i < n; i++) { const q = j * n + i; x[q * 3] = i * 18 - 54 + 0.3 * j; x[q * 3 + 1] = 1000 + 2 * Math.sin(i + 2 * j); x[q * 3 + 2] = j * 18 - 54; w[q] = (i + j) % 5 === 0 ? 0 : 0.5 + 0.01 * q; }
  const T = [], tm = [];
  for (let j = 0; j + 1 < n; j++) for (let i = 0; i + 1 < n; i++) { const a = j * n + i, b = a + 1, c = a + n, d = c + 1; T.push(a, c, b, b, c, d); tm.push(3.2 + 0.1 * (i % 3), 3.2 + 0.1 * (j % 3)); }
  const P = [], Nn = [];
  for (let k = 0; k < 40; k++) { const px = -60 + (k * 37) % 120, pz = -60 + (k * 53) % 120, py = 995 + 4 * Math.sin(k); P.push(px, py, pz); const nx = 0.15 * Math.sin(k * 3), nz = 0.1 * Math.cos(k * 5), L = Math.hypot(nx, 1, nz); Nn.push(nx / L, 1 / L, nz / L); }
  const prev = new Float32Array(x);
  for (let q = 0; q < N; q++) prev[q * 3 + 1] -= 0.25;
  const sim = { x: new Float32Array(x), p: new Float32Array(prev), w, bt: { P: new Float32Array(P), Nn: new Float32Array(Nn), nb: P.length / 3, T: Int32Array.from(T), tm: Float32Array.from(tm), C: 24, HS: 1 << 16, head: new Int32Array(1 << 16), next: new Int32Array(T.length / 3 * 8 + 16), item: new Int32Array(T.length / 3 * 8 + 16) } };
  const fixes = AT.bodyTriCollide(sim);
  return { x: arr(x), prev: arr(prev), w: arr(w), tris: T, margins: arr(sim.bt.tm), skin: arr(sim.bt.P), normals: arr(sim.bt.Nn), fixes, outX: arr(sim.x), outPrev: arr(sim.p) };
}

const projects = [];
projects.push(projectCase('tee', newDoc('tee', 'female'), { frames: 30, full: [1, 2, 30], rebuild: teeRebuilds, fit: true }));
projects.push(projectCase('tee variety', varietyDoc(), { frames: 12, full: [1, 2, 12], script: varietyScript, fit: true }));
projects.push(projectCase('tee triangle contacts', newDoc('tee', 'female'), { frames: 8, full: [4], params: { cpuTriCollision: true, ccEvery: 4 } }));
{
  const d = newDoc('dress', 'female');
  d.sim = { preset: 'draft' };
  projects.push(projectCase('dress draft', d, { frames: 6, full: [1, 6], simStep: true, fit: true }));
}
projects.push(projectCase('skirt', newDoc('skirt', 'female'), { frames: 0 }));
projects.push(projectCase('dress male', newDoc('dress', 'male'), { frames: 0 }));

process.stdout.write(`${JSON.stringify({
  generator: 'tools/golden/export-sim.mjs',
  settings: settingsCases(), fricK: fricKCases(), grabAnchor: grabAnchorCases(), settle: settleCases(), estimate: estimateCases(),
  projects, untangle: untangleCase(), bodyTri: bodyTriCase(),
})}\n`);
