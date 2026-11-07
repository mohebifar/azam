// Exports golden data for crates/azam-mesh from the reference JavaScript engine in src.js:
// AT.meshPiece, AT.topology and AT.locate for every template piece.
//
//   (cd tools/golden && npm install)
//   node tools/golden/export-mesh.mjs [path/to/src.js] > crates/azam-mesh/tests/golden/mesh.json
//
// src.js loads Delaunator 5.0.1 from a CDN at runtime; here the same version comes from npm.
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import Delaunator from 'delaunator';
import { loadEngine, rng, templatePieces } from './load-engine.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const AT = loadEngine(process.argv[2] || path.join(here, '../../src.js'), { Delaunator });

const pts = (list) => list.map((p) => [p.x, p.y, p.c ? 1 : 0, p.id]);

// Templates have no internal lines, so add open lines with curve points. They are spread over the
// bounding box, so some run outside the piece. The last two are degenerate: a single point
// (skipped) and a zero-length line.
function syntheticLines(box, rand) {
  const p = (id, c) => ({ id, x: box.x0 + rand() * box.w, y: box.y0 + rand() * box.h, ...(c ? { c: 1 } : {}) });
  const q = p('d0');
  return [
    { id: 'line', pts: [p('a0'), p('a1')], fold: null },
    { id: 'fold', pts: [p('b0'), p('b1', 1), p('b2')], fold: 90 },
    { id: 'wave', pts: [p('c0'), p('c1', 1), p('c2', 1), p('c3'), p('c4', 1), p('c5')], fold: 180 },
    { id: 'dot', pts: [p('e0')], fold: null },
    { id: 'zero', pts: [q, { ...q, id: 'd1' }], fold: 30 },
  ];
}

function locateCase(mesh, box, rand) {
  const qs = [];
  for (let i = 0; i < 6; i++) qs.push([box.x0 - 30 + rand() * (box.w + 60), box.y0 - 30 + rand() * (box.h + 60)]);
  // Points well inside the mesh: a little off the centroid of random triangles.
  const nt = mesh.tris.length / 3;
  for (let i = 0; i < 4 && nt > 0; i++) {
    const t = Math.floor(rand() * nt);
    const [a, b, c] = [0, 1, 2].map((k) => mesh.tris[3 * t + k]);
    const w = [0.2 + rand() * 0.3, 0.2 + rand() * 0.3];
    const ws = [w[0], w[1], 1 - w[0] - w[1]];
    qs.push([0, 1].map((d) => ws[0] * mesh.xy[2 * a + d] + ws[1] * mesh.xy[2 * b + d] + ws[2] * mesh.xy[2 * c + d]));
  }
  return qs.map((q) => {
    const r = AT.locate(mesh, q[0], q[1]);
    return { q, hit: r && { tri: r.tri, abc: [r.a, r.b, r.c], l: [r.l1, r.l2, r.l3] } };
  });
}

function meshCase(name, pc, variant, { pd, grain, internals }, rand, extra) {
  const mesh = AT.meshPiece(pc.pts, internals, pd, grain);
  const out = {
    name: `${name} [${variant}]`,
    pts: pts(pc.pts),
    internalLines: internals.map((il) => ({ id: il.id, pts: pts(il.pts), fold: il.fold })),
    pd: pd ?? null,
    grain: grain ?? null,
    mesh: {
      n: mesh.n,
      xy: Array.from(mesh.xy),
      tris: Array.from(mesh.tris),
      nB: mesh.nB,
      bnd: Array.from(mesh.bnd),
      bk: Array.from(mesh.bk),
      bt: Array.from(mesh.bt),
      internals: mesh.internals.map((s) => ({ id: s.id, verts: s.verts, fold: s.fold })),
      pd: mesh.pd,
    },
  };
  if (extra) {
    const topo = AT.topology(mesh);
    out.edges = topo.edges.flatMap((e) => [e.a, e.b, e.o1, e.o2]);
    out.locate = locateCase(mesh, AT.bbox(mesh.ol.poly), rand);
  }
  return out;
}

const rand = rng(20261108);
const cases = [];
const pieces = templatePieces(AT);
for (const { name, pc } of pieces) {
  const box = AT.bbox(AT.outline(pc.pts).poly);
  const lines = syntheticLines(box, rand);
  cases.push(meshCase(name, pc, 'template', { pd: pc.pd, grain: pc.grain, internals: pc.internals || [] }, rand, false));
  cases.push(meshCase(name, pc, 'coarse, 37.5° grain, internal lines', { pd: (pc.pd || 20) * 1.6, grain: 37.5, internals: lines }, rand, true));
  cases.push(meshCase(name, pc, 'medium, 90° grain', { pd: (pc.pd || 20) * 1.3, grain: 90, internals: lines.slice(0, 2) }, rand, false));
}
// pd is clamped to 3..80 mm and defaults to 20 mm.
const rib = pieces.find((p) => p.name.startsWith('tee/4'));
for (const [variant, pd] of [['default pd', undefined], ['pd below minimum', 1], ['pd above maximum', 500]]) {
  cases.push(meshCase(rib.name, rib.pc, variant, { pd, grain: -12, internals: [] }, rand, true));
}

process.stdout.write(`${JSON.stringify({ generator: 'tools/golden/export-mesh.mjs', cases })}\n`);
