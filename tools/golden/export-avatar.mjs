// Exports golden data for crates/azam-avatar from the reference JavaScript engine in src.js.
//
//   node tools/golden/export-avatar.mjs [path/to/src.js] > crates/azam-avatar/tests/golden/avatar.json
//
// The MakeHuman avatar pack is not part of the repository, so this script also builds a small
// synthetic pack (a ring-and-tube mannequin with morph targets, measures, landmarks, joints and
// skin weights) in the same binary format, runs the engine's MakeHuman code on it, and writes the
// pack next to the JSON as crates/azam-avatar/tests/golden/synthetic-body.atmh.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { loadEngine, rng, templatePieces } from './load-engine.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const AT = loadEngine(process.argv[2] || path.join(here, '../../src.js'));
const PACK_PATH = path.join(here, '../../crates/azam-avatar/tests/golden/synthetic-body.atmh');

// The engine sandbox has no TextDecoder (mhParse needs it): reach the sandbox global through a
// function created by the sandbox's own Function constructor.
const sandboxGlobal = AT.clamp.constructor('return this')();
sandboxGlobal.TextDecoder = TextDecoder;

const rand = rng(20261107);
const arr = (a) => Array.from(a);
const xyz = (p) => [p[0], p[1], p[2]];

/* ---------- math ---------- */
function mathCases() {
  const ellAxes = [];
  for (let i = 0; i < 12; i++) { const C = 200 + rand() * 1200, k = 0.4 + rand() * 0.6; const ab = AT.ellAxes(C, k); ellAxes.push([C, k, ab[0], ab[1]]); }
  const ellPerim = [];
  for (let i = 0; i < 12; i++) { const a = 20 + rand() * 300, b = 20 + rand() * 300; ellPerim.push([a, b, AT.ellPerim(a, b)]); }
  const pchip = [];
  for (let c = 0; c < 6; c++) {
    const n = 2 + Math.floor(rand() * 8), xs = [], ys = [];
    let x = rand() * 100;
    for (let i = 0; i < n; i++) { xs.push(x); ys.push(rand() * 200 - 100); x += 1 + rand() * 60; }
    const f = AT.pchip(xs, ys), at = [];
    for (let i = 0; i < 25; i++) { const q = xs[0] - 20 + rand() * (xs[n - 1] - xs[0] + 40); at.push([q, f(q)]); }
    xs.forEach((q) => at.push([q, f(q)]));
    pchip.push({ xs, ys, at });
  }
  const pt = (s) => [(rand() - 0.5) * s, (rand() - 0.5) * s, (rand() - 0.5) * s];
  const roundCone = [];
  for (let c = 0; c < 6; c++) {
    const a = pt(200), b = pt(200), r1 = 5 + rand() * 60, r2 = c === 5 ? r1 : 5 + rand() * 60;
    const P = { ax: a[0], ay: a[1], az: a[2], bx: b[0], by: b[1], bz: b[2], r1, r2 };
    const pts = [];
    for (let i = 0; i < 30; i++) { const q = pt(400); pts.push([...q, AT.sdRoundCone(q[0], q[1], q[2], P)]); }
    roundCone.push({ a, b, r1, r2, pts });
  }
  const ellipsoid = [];
  for (let c = 0; c < 6; c++) {
    const center = pt(100), radii = [10 + rand() * 120, 10 + rand() * 120, 10 + rand() * 120];
    const P = { cx: center[0], cy: center[1], cz: center[2], rx: radii[0], ry: radii[1], rz: radii[2] };
    const pts = [[...center, AT.sdEllipsoid(center[0], center[1], center[2], P)]];
    for (let i = 0; i < 30; i++) { const q = pt(400); pts.push([...q, AT.sdEllipsoid(q[0], q[1], q[2], P)]); }
    ellipsoid.push({ center, radii, pts });
  }
  const smin = [];
  for (let i = 0; i < 20; i++) { const a = rand() * 60 - 30, b = rand() * 60 - 30, k = 1 + rand() * 40; smin.push([a, b, k, AT.smin(a, b, k)]); }
  return { ellAxes, ellPerim, pchip, roundCone, ellipsoid, smin };
}

function solveLinCases() {
  const out = [];
  for (let c = 0; c < 8; c++) {
    const n = 1 + Math.floor(rand() * 8), A = [], b = [];
    for (let i = 0; i < n; i++) { const row = []; for (let j = 0; j < n; j++) row.push(rand() < 0.2 ? 0 : rand() * 20 - 10); A.push(row); b.push(rand() * 10 - 5); }
    out.push({ A, b, x: AT.solveLin(A, b) });
  }
  out.push({ A: [[1, 2], [2, 4]], b: [1, 2], x: AT.solveLin([[1, 2], [2, 4]], [1, 2]) });
  return out;
}

/* ---------- shared avatar exports ---------- */
const frameOut = (F) => ({
  O: xyz(F.O), a: xyz(F.a), n: xyz(F.n), r: xyz(F.r), R: F.R, t0: F.t0 || 0, wrapAxis: !!F.wrapAxis,
  chain: (F.chain || []).map((c) => ({ A: xyz(c.A), a: xyz(c.a), n: xyz(c.n), r: xyz(c.r), s: c.s, L: c.L })),
});

function gridSummary(G, stride) {
  let sum = 0, neg = 0;
  for (let i = 0; i < G.data.length; i++) { sum += G.data[i]; if (G.data[i] < 0) neg++; }
  const values = [];
  for (let i = 0; i < G.data.length; i += stride) values.push(G.data[i]);
  return { x0: G.x0, y0: G.y0, z0: G.z0, g: G.g, nx: G.nx, ny: G.ny, nz: G.nz, sum, neg, stride, values };
}

function gridSamples(G, count) {
  const out = [], g = [0, 0, 0];
  const X = G.x0 + G.g * (G.nx - 1), Y = G.y0 + G.g * (G.ny - 1), Z = G.z0 + G.g * (G.nz - 1);
  for (let i = 0; i < count; i++) {
    const p = [G.x0 - 20 + rand() * (X - G.x0 + 40), G.y0 - 20 + rand() * (Y - G.y0 + 40), G.z0 - 20 + rand() * (Z - G.z0 + 40)];
    g[0] = g[1] = g[2] = NaN;
    const d = AT.gridSample(G, p[0], p[1], p[2], g);
    out.push(Number.isNaN(g[0]) ? [...p, d] : [...p, d, g[0], g[1], g[2]]);
  }
  return out;
}

function meshSummary(mesh, vStride, tStride) {
  const sum = (a) => { let s = 0; for (let i = 0; i < a.length; i++) s += a[i]; return s; };
  const nV = mesh.pos.length / 3, nT = mesh.idx.length / 3, verts = [], tris = [];
  for (let v = 0; v < nV; v += vStride) verts.push([v, ...arr(mesh.pos.subarray(v * 3, v * 3 + 3)), ...arr(mesh.nor.subarray(v * 3, v * 3 + 3))]);
  for (let t = 0; t < nT; t += tStride) tris.push([t, ...arr(mesh.idx.subarray(t * 3, t * 3 + 3))]);
  return { vertices: nV, triangles: nT, posSum: sum(mesh.pos), norSum: sum(mesh.nor), idxSum: sum(mesh.idx), vStride, tStride, verts, tris };
}

function profileOut(av, shoulderXs) {
  const t = av.torso;
  return {
    H: av.H, ys: av.ys, neckR: av.neckR,
    torso: { y0: t.y0, y1: t.y1, step: t.step, A: arr(t.A), B: arr(t.B), C: arr(t.C) },
    evals: Array.from({ length: 25 }, () => { const y = t.y0 - 30 + rand() * (t.y1 - t.y0 + 60); return [y, ...t.eval(y)]; }),
    shoulderY: shoulderXs.map((x) => [x, av.shoulderY(x)]),
  };
}

function jointsOut(J) {
  const o = {};
  for (const s of ['L', 'R']) { const j = J[s]; o[s] = { S: j.S, E: j.E, W: j.W, a: j.a, uaR: j.uaR, Hj: j.Hj, K: j.K, A: j.A, la: j.la, thR: j.thR }; }
  return o;
}

function slotFrames(av) {
  const out = [];
  for (const sl of AT.SLOTS) {
    const variants = [{ slot: sl.id }, { slot: sl.id, off: 25, ang: sl.ang + 33, h: av.ys.bust, t: 120 }, { slot: sl.id, off: 0, ang: -70, t: 410 }];
    for (const a of variants) out.push({ arr: a, vRange: [-180, 140], frame: frameOut(AT.slotFrame(av, a, [-180, 140])) });
  }
  out.push({ arr: { slot: 'nonexistent' }, vRange: [-10, 10], frame: frameOut(AT.slotFrame(av, { slot: 'nonexistent' }, [-10, 10])) });
  return out;
}

const pieces = templatePieces(AT);
function pieceArrangements(av) {
  const out = [];
  const place = (name, pc, a) => {
    const ol = AT.outline(pc.pts), bb = AT.bbox(ol.poly);
    const xy = [];
    ol.poly.forEach((p, i) => { if (i % 3 === 0) xy.push(p.x, p.y); });
    for (let i = 0; i < 40; i++) xy.push(bb.x0 + rand() * bb.w, bb.y0 + rand() * bb.h);
    const n = xy.length / 2, outPos = new Float32Array(n * 3 + 6);
    const F = AT.arrangePositions(av, { arr: a }, { n, xy: Float64Array.from(xy), ol }, outPos, 1);
    out.push({ name, arr: a || null, bbox: [bb.x0, bb.y0, bb.x1, bb.y1], xy, pos: arr(outPos.subarray(3, 3 + n * 3)), frame: frameOut(F) });
  };
  pieces.forEach(({ name, pc }) => place(name, pc, pc.arr));
  const front = pieces.find((p) => p.name.startsWith('tee/0')), sleeve = pieces.find((p) => p.name.startsWith('tee/2'));
  place('variant/flip', front.pc, { ...front.pc.arr, flip: true });
  place('variant/flat-shift', front.pc, { ...front.pc.arr, flat: true, shift: 120 });
  place('variant/shift-wrap', front.pc, { ...front.pc.arr, shift: -90, ang: 20 });
  place('variant/sleeve-flat', sleeve.pc, { ...sleeve.pc.arr, flat: true, flip: true });
  place('variant/leg', sleeve.pc, { slot: 'legRB', off: 40, t: 300 });
  place('variant/neck-default', front.pc, { slot: 'neck' });
  place('variant/default-arr', front.pc, undefined);
  place('variant/chador', front.pc, { drape: 'chador', originX: 300, originY: 40, crownLift: 20, headZ: 15 });
  place('variant/chador-defaults', front.pc, { drape: 'chador' });
  return out;
}

function arrangementPointsOut(av) {
  return AT.arrangementPoints(av).map((p) => ({ slot: p.slot, h: p.h ?? null, t: p.t ?? null, label: p.label, pos: xyz(p.pos) }));
}

/* ---------- parametric avatars ---------- */
function parametricCase(name, params, withPieces) {
  const av = AT.buildAvatar(params), mesh = AT.avatarMesh(av), G = av.grid;
  const prims = av.prims.map((P) => (P.t === 'rc'
    ? { t: 'rc', a: [P.ax, P.ay, P.az], b: [P.bx, P.by, P.bz], r1: P.r1, r2: P.r2, k: P.k, bb: P.bb }
    : { t: 'el', c: [P.cx, P.cy, P.cz], r: [P.rx, P.ry, P.rz], k: P.k, bb: P.bb }));
  const X = G.x0 + G.g * (G.nx - 1), Y = G.y0 + G.g * (G.ny - 1), Z = G.z0 + G.g * (G.nz - 1);
  const sdf = Array.from({ length: 80 }, () => { const p = [G.x0 + rand() * (X - G.x0), G.y0 + rand() * (Y - G.y0), G.z0 + rand() * (Z - G.z0)]; return [...p, av.sdf(p[0], p[1], p[2])]; });
  const TP = AT.torsoProfile(params);
  return {
    name, params,
    profile: { ...profileOut(av, [0, 30, 60, 90, 120, 150, 180, 210, 260]), s: TP.s, fem: TP.fem, hip: TP.hip, shH: TP.shH },
    prims, joints: jointsOut(av.joints), sdf,
    grid: gridSummary(G, 997), samples: gridSamples(G, 400),
    mesh: meshSummary(mesh, 211, 401),
    arrangementPoints: arrangementPointsOut(av),
    slotFrames: slotFrames(av),
    pieces: withPieces ? pieceArrangements(av) : [],
  };
}

/* ---------- synthetic MakeHuman pack ---------- */
function syntheticPack() {
  const P = [], meta = [], bodyTris = [], eyeTris = [];
  const add = (p, m) => { P.push(p[0], p[1], p[2]); meta.push(m); return meta.length - 1; };
  const parts = [];
  const N = 24;
  // closed tube through a list of ring centres; ring(i) -> {c, u, w, ru, rw}
  function tube(part, rings, capA, capB) {
    const ids = [], start = bodyTris.length;
    rings.forEach((R, ri) => {
      const ring = [];
      for (let j = 0; j < N; j++) {
        const th = 2 * Math.PI * j / N, cu = Math.cos(th) * R.ru, sw = Math.sin(th) * R.rw;
        const p = [R.c[0] + R.u[0] * cu + R.w[0] * sw, R.c[1] + R.u[1] * cu + R.w[1] * sw, R.c[2] + R.u[2] * cu + R.w[2] * sw];
        const rad = [p[0] - R.c[0], p[1] - R.c[1], p[2] - R.c[2]], L = Math.hypot(...rad) || 1;
        ring.push(add(p, { part, ring: ri, j, th, rad: rad.map((v) => v / L), axisDir: R.d }));
      }
      ids.push(ring);
    });
    for (let ri = 0; ri + 1 < ids.length; ri++) for (let j = 0; j < N; j++) {
      const a = ids[ri][j], b = ids[ri][(j + 1) % N], c = ids[ri + 1][(j + 1) % N], d = ids[ri + 1][j];
      bodyTris.push(a, b, c, a, c, d);
    }
    const ca = add(capA, { part, ring: -1, rad: [0, 0, 0] }), cb = add(capB, { part, ring: ids.length, rad: [0, 0, 0] });
    for (let j = 0; j < N; j++) { bodyTris.push(ca, ids[0][(j + 1) % N], ids[0][j]); bodyTris.push(cb, ids[ids.length - 1][j], ids[ids.length - 1][(j + 1) % N]); }
    // orient outward: positive signed volume
    let vol = 0;
    for (let t = start; t < bodyTris.length; t += 3) {
      const a = bodyTris[t] * 3, b = bodyTris[t + 1] * 3, c = bodyTris[t + 2] * 3;
      vol += P[a] * (P[b + 1] * P[c + 2] - P[b + 2] * P[c + 1]) - P[a + 1] * (P[b] * P[c + 2] - P[b + 2] * P[c]) + P[a + 2] * (P[b] * P[c + 1] - P[b + 1] * P[c]);
    }
    if (vol < 0) for (let t = start; t < bodyTris.length; t += 3) { const x = bodyTris[t + 1]; bodyTris[t + 1] = bodyTris[t + 2]; bodyTris[t + 2] = x; }
    parts.push({ part, ids, capA: ca, capB: cb });
    return ids;
  }
  const X = [1, 0, 0], Z = [0, 0, 1];
  // torso: crotch 800 -> neck 1480
  const torsoY = [], torsoRings = [];
  for (let y = 800; y <= 1480; y += 20) torsoY.push(y);
  const ax = AT.pchip([800, 900, 1060, 1160, 1230, 1380, 1440, 1480], [120, 168, 128, 140, 160, 150, 60, 52]);
  const bz = AT.pchip([800, 900, 1060, 1160, 1230, 1380, 1440, 1480], [90, 128, 95, 100, 112, 90, 55, 50]);
  const cz = AT.pchip([800, 900, 1230, 1480], [-5, -15, 12, -12]);
  torsoY.forEach((y) => torsoRings.push({ c: [0, y, cz(y)], u: X, w: Z, ru: ax(y), rw: bz(y), d: [0, 1, 0] }));
  const torso = tube('torso', torsoRings, [0, 795, -5], [0, 1486, -12]);
  // head sphere
  const headRings = [];
  for (let i = 1; i < 9; i++) { const a = Math.PI * i / 9, y = 1590 - 110 * Math.cos(a), r = 100 * Math.sin(a); headRings.push({ c: [0, y, 10], u: X, w: Z, ru: r, rw: r * 1.05, d: [0, 1, 0] }); }
  const head = tube('head', headRings, [0, 1480, 10], [0, 1700, 10]);
  // limbs
  const limb = (part, root, dir, len, r0, r1, nR, extraEnd) => {
    const rings = [];
    const d = AT.norm(dir), u = AT.norm(AT.cross(d, Z)), w = AT.cross(u, d);
    for (let i = 0; i < nR; i++) { const t = len * i / (nR - 1), r = r0 + (r1 - r0) * i / (nR - 1); rings.push({ c: [root[0] + d[0] * t, root[1] + d[1] * t, root[2] + d[2] * t], u, w, ru: r, rw: r * 0.92, d }); }
    const endC = rings[nR - 1].c;
    return tube(part, rings, [root[0] - d[0] * r0 * 0.5, root[1] - d[1] * r0 * 0.5, root[2] - d[2] * r0 * 0.5], extraEnd || [endC[0] + d[0] * r1 * 0.5, endC[1] + d[1] * r1 * 0.5, endC[2] + d[2] * r1 * 0.5]);
  };
  const s8 = Math.sin(8 * AT.deg), c8 = Math.cos(8 * AT.deg), s3 = Math.sin(3 * AT.deg), c3 = Math.cos(3 * AT.deg);
  const armL = limb('armL', [178, 1392, -5], [s8, -c8, 0], 560, 46, 27, 13);
  const armR = limb('armR', [-178, 1392, -5], [-s8, -c8, 0], 560, 46, 27, 13);
  const legL = limb('legL', [92, 845, -5], [s3, -c3, 0], 770, 86, 38, 15, [92 + s3 * 770, 0, 30]);
  const legR = limb('legR', [-92, 845, -5], [-s3, -c3, 0], 770, 86, 38, 15, [-92 - s3 * 770, 0, 30]);
  const bodyV = meta.length;
  // eyes: two small tetrahedra on the face
  for (const sx of [1, -1]) {
    const c = [sx * 35, 1610, 112], e = [];
    for (const o of [[6, 0, 0], [-3, 5, 0], [-3, -5, 0], [0, 0, 6]]) e.push(add([c[0] + o[0], c[1] + o[1], c[2] + o[2]], { part: 'eye', rad: [0, 0, 0] }));
    eyeTris.push(e[0], e[1], e[2], e[0], e[3], e[1], e[1], e[3], e[2], e[2], e[3], e[0]);
  }
  const nV = meta.length;
  // skin weights
  const wt = { armL: new Uint8Array(nV), armR: new Uint8Array(nV), legL: new Uint8Array(nV), legR: new Uint8Array(nV) };
  meta.forEach((m, v) => { if (wt[m.part]) wt[m.part][v] = m.ring <= 0 ? 128 : 255; });
  // measures, landmarks, joints
  const ringNear = (rings, ys, y) => { let best = 0; ys.forEach((yy, i) => { if (Math.abs(yy - y) < Math.abs(ys[best] - y)) best = i; }); return rings[best]; };
  const tr = (y) => ringNear(torso, torsoY, y);
  const at = (ring, deg) => ring[Math.round(deg / 360 * N) % N];
  const measures = {
    'measure-neck-circ': tr(1460), 'measure-bust-circ': tr(1230), 'measure-underbust-circ': tr(1160), 'measure-waist-circ': tr(1060), 'measure-hips-circ': tr(900),
    'measure-upperarm-circ': armL[3], 'measure-wrist-circ': armL[11], 'measure-thigh-circ': legL[2], 'measure-knee-circ': legL[7], 'measure-calf-circ': legL[9], 'measure-ankle-circ': legL[13],
    'measure-frontchest-dist': [at(tr(1260), 60), at(tr(1260), 120)], 'measure-napetowaist-dist': [at(tr(1460), 270), at(tr(1060), 270)],
    'measure-waisttohip-dist': [at(tr(1060), 0), at(tr(900), 0)],
    'measure-upperarm-length': [armL[0][6], armL[6][6]], 'measure-lowerarm-length': [armL[6][6], armL[12][6]],
    'measure-upperleg-height': [legL[0][0], legL[7][0]], 'measure-lowerleg-height': [legL[7][0], legL[14][0]],
    'measure-shoulder-dist': [armL[0][6], armR[0][6]],
  };
  const topOf = (ring) => ring.reduce((b, v) => (P[v * 3 + 1] > P[b * 3 + 1] ? v : b), ring[0]);
  const landmarks = { shoulderL: topOf(armL[0]), shoulderR: topOf(armR[0]), crotch: parts[0].capA, navel: at(tr(1000), 90) };
  const joints = { 'joint-head': head[3] };
  for (const [s, A, Lg] of [['l', armL, legL], ['r', armR, legR]]) {
    Object.assign(joints, { [`joint-${s}-shoulder`]: A[0], [`joint-${s}-elbow`]: A[6], [`joint-${s}-hand`]: A[12], [`joint-${s}-upper-leg`]: Lg[0], [`joint-${s}-knee`]: Lg[7], [`joint-${s}-ankle`]: Lg[14] });
  }
  // morph targets
  const q = 0.02;
  const targets = {};
  const gauss = (x, s) => Math.exp(-(x * x) / (2 * s * s));
  const define = (name, fn) => {
    const idx = [], d = [];
    meta.forEach((m, v) => {
      const dd = fn(m, [P[v * 3], P[v * 3 + 1], P[v * 3 + 2]], v);
      if (!dd) return;
      const qd = dd.map((x) => Math.max(-32767, Math.min(32767, Math.round(x / q))));
      if (qd[0] || qd[1] || qd[2]) { idx.push(v); d.push(...qd); }
    });
    targets[name] = { idx, d };
  };
  const radial = (m, amt) => [m.rad[0] * amt, m.rad[1] * amt, m.rad[2] * amt];
  const isLimb = (m) => /^(arm|leg)/.test(m.part);
  define('g_female', (m, p) => (m.part === 'torso' ? [0, 0, 15 * gauss(p[1] - 1230, 60) * Math.max(0, Math.sin(m.th || 0))] : null));
  define('g_male', (m, p) => (m.part === 'torso' ? [Math.sign(p[0]) * 20 * gauss(p[1] - 1380, 80), 0, 0] : null));
  ['female', 'male'].forEach((gn, gi) => ['minmuscle', 'averagemuscle', 'maxmuscle'].forEach((mu, mi) => ['minweight', 'averageweight', 'maxweight'].forEach((we, wi) => {
    const amt = (wi - 1) * 12 + (mi - 1) * 5 + gi * 4;
    define(`universal-${gn}-young-${mu}-${we}`, (m) => (m.part === 'torso' || isLimb(m) ? radial(m, amt * (isLimb(m) ? 0.5 : 1)) : null));
  })));
  const circTargets = { 'measure-neck-circ': ['torso', 1460], 'measure-bust-circ': ['torso', 1230], 'measure-underbust-circ': ['torso', 1160], 'measure-waist-circ': ['torso', 1060], 'measure-hips-circ': ['torso', 900],
    'measure-upperarm-circ': ['arm', 3], 'measure-wrist-circ': ['arm', 11], 'measure-thigh-circ': ['leg', 2], 'measure-knee-circ': ['leg', 7], 'measure-calf-circ': ['leg', 9], 'measure-ankle-circ': ['leg', 13] };
  const both = (name, fn) => { define(name + '-incr', (m, p, v) => fn(m, p, v, 1)); define(name + '-decr', (m, p, v) => fn(m, p, v, -0.8)); };
  Object.entries(circTargets).forEach(([name, [part, where]]) => both(name, (m, p, v, sg) => {
    if (part === 'torso') return m.part === 'torso' && m.ring >= 0 && m.ring < torso.length ? radial(m, sg * 20 * gauss(p[1] - where, 25)) : null;
    if (!m.part.startsWith(part) || m.ring < 0 || m.ring >= 15) return null;
    return radial(m, sg * 12 * gauss(m.ring - where, 1));
  }));
  both('measure-frontchest-dist', (m, p, v, sg) => (m.part === 'torso' ? [sg * Math.sign(p[0]) * 15 * Math.max(0, Math.sin(m.th || 0)) * gauss(p[1] - 1260, 40), 0, 0] : null));
  const ramp = (x, a, b) => AT.clamp((x - a) / (b - a), 0, 1);
  both('measure-napetowaist-dist', (m, p, v, sg) => (m.part === 'torso' || m.part === 'head' || /^arm/.test(m.part) ? [0, sg * 30 * ramp(p[1], 1060, 1440), 0] : null));
  both('measure-waisttohip-dist', (m, p, v, sg) => (m.part === 'torso' ? [0, -sg * 25 * (1 - ramp(p[1], 880, 1060)), 0] : null));
  both('measure-shoulder-dist', (m, p, v, sg) => (/^arm/.test(m.part) ? [sg * Math.sign(p[0]) * 18, 0, 0] : m.part === 'torso' ? [sg * Math.sign(p[0]) * 18 * ramp(p[1], 1250, 1400), 0, 0] : null));
  const limbDir = (m) => (m.part === 'armL' ? [s8, -c8, 0] : m.part === 'armR' ? [-s8, -c8, 0] : m.part === 'legL' ? [s3, -c3, 0] : [-s3, -c3, 0]);
  const shiftAlong = (m, amt) => { const d = limbDir(m); return [d[0] * amt, d[1] * amt, d[2] * amt]; };
  both('measure-upperarm-length', (m, p, v, sg) => (/^arm/.test(m.part) ? shiftAlong(m, sg * 30 * ramp(m.ring, 1, 6)) : null));
  both('measure-lowerarm-length', (m, p, v, sg) => (/^arm/.test(m.part) ? shiftAlong(m, sg * 25 * ramp(m.ring, 7, 12)) : null));
  both('measure-upperleg-height', (m, p, v, sg) => (/^leg/.test(m.part) ? shiftAlong(m, sg * 30 * ramp(m.ring, 1, 7)) : null));
  both('measure-lowerleg-height', (m, p, v, sg) => (/^leg/.test(m.part) ? shiftAlong(m, sg * 25 * ramp(m.ring, 8, 14)) : null));
  define('nipple-point-decr', (m, p) => (m.part === 'torso' ? [0, 0, -3 * gauss(p[1] - 1230, 20) * Math.max(0, Math.sin(m.th || 0)) ** 4] : null));
  define('nipple-size-decr', (m, p) => (m.part === 'torso' ? [0, 0, -1.5 * gauss(p[1] - 1230, 30) * Math.max(0, Math.sin(m.th || 0)) ** 2] : null));
  // pack
  const arrays = [];
  const push = (name, dtype, data) => arrays.push({ name, dtype, data });
  push('base', 'float32', Float32Array.from(P));
  push('bodyTris', 'uint32', Uint32Array.from(bodyTris));
  push('eyeTris', 'uint16', Uint16Array.from(eyeTris));
  push('w_armL', 'uint8', wt.armL); push('w_armR', 'uint8', wt.armR); push('w_legL', 'uint8', wt.legL); push('w_legR', 'uint8', wt.legR);
  const names = Object.keys(targets);
  names.forEach((nm, i) => {
    const t = targets[nm];
    if (i % 2) push('ti_' + nm, 'uint16', Uint16Array.from(t.idx)); else push('ti_' + nm, 'int32', Int32Array.from(t.idx));
    push('td_' + nm, 'int16', Int16Array.from(t.d));
  });
  let off = 0;
  const specs = {};
  arrays.forEach((a) => { off = Math.ceil(off / 4) * 4; specs[a.name] = { dtype: a.dtype, offset: off, length: a.data.length }; off += a.data.byteLength; });
  const header = { nV, bodyV, q, arrays: specs, targets: names, measures, landmarks, joints };
  let hjson = JSON.stringify(header);
  while ((8 + Buffer.byteLength(hjson)) % 4) hjson += ' ';
  const hbuf = Buffer.from(hjson), total = 8 + hbuf.length + off;
  const out = new Uint8Array(total);
  out.set(Buffer.from('ATMH'), 0);
  new DataView(out.buffer).setUint32(4, hbuf.length, true);
  out.set(hbuf, 8);
  arrays.forEach((a) => out.set(new Uint8Array(a.data.buffer, a.data.byteOffset, a.data.byteLength), 8 + hbuf.length + specs[a.name].offset));
  return out;
}

function makehumanCase() {
  const bytes = syntheticPack();
  fs.writeFileSync(PACK_PATH, bytes);
  const buf = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
  const M = AT.mhParse(buf);
  const fitOut = (f) => ({ gender: f.gender, muscle: f.muscle, weight: f.weight, meas: AT.MH_FIT_TARGETS.map((k) => f.meas[k]), scale: f.scale });
  const shape = { gender: 0.3, muscle: 0.8, weight: 0.25, meas: { 'measure-upperarm-length': 1.1, 'measure-bust-circ': 0.7, 'measure-waist-circ': -0.4, 'measure-lowerleg-height': -1.3 } }; // keys in fit-target order
  const composed = AT.mhCompose(M, shape, false);
  const tapes = ['measure-bust-circ', 'measure-hips-circ', 'measure-wrist-circ'].map((k) => {
    const t = AT.mhTape(M.measures[k].map((g) => AT.mhP(composed, null, g)));
    return { name: k, len: t.len, c: t.c, hull: t.hull3 };
  });
  const specA = { ...AT.AVATAR_PRESETS.female, acrossFront: 330, backLength: 405, waistToHip: 190, wrist: 165, kneeHeight: 470, knee: 360, calf: 350, ankle: 230, armAngle: 38, legSpread: 4 };
  const specB = { ...AT.AVATAR_PRESETS.male, height: 1760, fat: 0.7, muscle: 0.8 };
  const fits = [specA, specB].map((spec) => ({ spec, fit: fitOut(AT.mhFit(M, { ...AT.AVATAR_PRESETS[spec.sex], ...spec })) }));
  const av = AT.buildAvatarMH(M, specA, { g: 10 });
  const posed = AT.mhCompose(M, { gender: 0, muscle: 0.5, weight: 0.5, meas: {} }, false);
  const poseInfo = AT.mhPose(M, posed, 20, 9);
  const prof = AT.profileFromAvatar(av);
  return {
    probe: M.probe.length,
    composed: { shape: { gender: 0.3, muscle: 0.8, weight: 0.25, meas: AT.MH_FIT_TARGETS.map((k) => shape.meas[k] || 0) }, pos: arr(composed) },
    measureBase: AT.MEAS.map((d) => AT.mhMeasureAll(M, M.base, null)[d.k]),
    measureComposed: AT.MEAS.map((d) => AT.mhMeasureAll(M, composed, null)[d.k]),
    tapes,
    pose: { arm: 20, leg: 9, info: poseInfo, pos: arr(posed) },
    fits,
    avatar: {
      spec: specA,
      measured: AT.MEAS.map((d) => av.measured[d.k]),
      fit: fitOut(av.fit),
      pose: av.pose,
      profile: profileOut(av, [0, 40, 80, 120, 160, 200, 300]),
      shH: prof.shH, hip: prof.hip,
      joints: jointsOut(av.joints),
      tapes: av.tapes.map((t) => ({ k: t.k, closed: !!t.closed, pts: t.pts })),
      mesh: { pos: arr(av.mesh.pos), nor: arr(av.mesh.nor), idx: arr(av.mesh.idx), bodyV: av.bodyV },
      grid: gridSummary(av.grid, 97),
      samples: gridSamples(av.grid, 300),
      arrangementPoints: arrangementPointsOut(av),
    },
  };
}

const female = AT.AVATAR_PRESETS.female, male = AT.AVATAR_PRESETS.male;
const custom = { sex: 'female', height: 1580, bust: 930, underbust: 0, waist: 770, hip: 1015, shoulder: 350, arm: 520, crotch: 700, neck: 320, thigh: 590, upperArm: 295, armAngle: 28, legSpread: 11 };
const out = {
  generator: 'tools/golden/export-avatar.mjs',
  presets: AT.AVATAR_PRESETS,
  sizeCharts: AT.SIZE_CHARTS,
  math: mathCases(),
  solveLin: solveLinCases(),
  avatars: [parametricCase('female', female, true), parametricCase('male', male, false), parametricCase('custom', custom, true)],
  makehuman: makehumanCase(),
};
process.stdout.write(`${JSON.stringify(out)}\n`);
