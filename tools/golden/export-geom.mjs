// Exports golden data for crates/azam-geom from the reference JavaScript engine in src.js.
//
//   node tools/golden/export-geom.mjs [path/to/src.js] > crates/azam-geom/tests/golden/geom.json
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { loadEngine, rng, templatePieces } from './load-engine.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const AT = loadEngine(process.argv[2] || path.join(here, '../../src.js'));

const xy = (p) => [p.x, p.y];
const pts = (list) => list.map((p) => [p.x, p.y, p.c ? 1 : 0, p.id]);

function cutCase(pc, ol) {
  const hem = { ...(pc.hem || {}) };
  if (!Object.values(hem).some((v) => v > 0)) {
    // No hem in the template: hem the lowest segment so folded corners are covered.
    const lowest = ol.segs.reduce((best, sg) => {
      const y = sg.poly.reduce((s, p) => s + p.y, 0) / sg.poly.length;
      return y > best.y ? { y, id: sg.id } : best;
    }, { y: -Infinity, id: null });
    hem[lowest.id] = 30;
  }
  const piece = { sa: pc.sa || 0, saSeg: pc.saSeg || {}, hem };
  return { ...piece, pts: AT.cutOutline(piece, ol).map(xy) };
}

function pieceCase({ name, pc }, rand) {
  const ol = AT.outline(pc.pts);
  const T = ol.total;
  const box = AT.bbox(ol.poly);
  const randomPoint = () => [box.x0 - 40 + rand() * (box.w + 80), box.y0 - 40 + rand() * (box.h + 80)];

  const perimeter = [-0.3, 0, 0.013, 0.25, 0.5, 0.731, 0.999, 1, 1.42].map((f) => {
    const p = AT.perimPoint(ol, T * f);
    return { p: T * f, x: p.x, y: p.y, tx: p.tx, ty: p.ty, k: p.k, t: p.t };
  });
  const nearest = Array.from({ length: 8 }, () => {
    const q = randomPoint();
    const r = AT.nearestOnOutline(ol, q[0], q[1]);
    return { q, k: r.k, t: r.t, x: r.x, y: r.y, d: r.d };
  });
  const inside = Array.from({ length: 12 }, () => {
    const q = randomPoint();
    return { q, inside: AT.inside(ol.poly, q[0], q[1]) };
  });
  const widths = Object.fromEntries(ol.segs.map((sg) => [sg.id, Math.round(5 + rand() * 20)]));

  const sides = [];
  ol.segs.slice(0, 3).forEach((sg) => {
    for (const reverse of [false, true]) sides.push(AT.segSide(sg.id, reverse));
  });
  if (ol.segs.length >= 3) {
    for (const dir of [1, -1]) {
      sides.push({ a: { s: ol.segs[0].id, t: 0.25 }, b: { s: ol.segs[2].id, t: 0.6 }, dir });
    }
  }

  return {
    name,
    pts: pts(pc.pts),
    outline: {
      total: T,
      starts: ol.start,
      segs: ol.segs.map((sg) => ({ id: sg.id, i0: sg.i0, i1: sg.i1, len: sg.len, poly: sg.poly.map(xy) })),
      poly: ol.poly.map(xy),
    },
    area: AT.area(ol.poly),
    perimeter,
    nearest,
    inside,
    offset: { widths, pts: AT.offsetOutline(ol, (id) => widths[id]).map(xy) },
    cut: cutCase(pc, ol),
    sides: sides.map((s) => ({
      a: s.a, b: s.b, dir: s.dir,
      length: (AT.sideRange(ol, s) || {}).L ?? null,
      pts: AT.sidePolyline(ol, s, 5).map(xy),
    })),
    path: { p0: T * 0.2, p1: T * 1.3, pts: AT.olPath(ol, T * 0.2, T * 1.3).map(xy) },
    internals: [...(pc.internals || []), ...syntheticLines(box, rand)]
      .filter((il) => il.pts && il.pts.length >= 2)
      .map((il) => ({ pts: pts(il.pts), flat: AT.evalOpen(il.pts).map(xy) })),
  };
}

// Templates have no internal lines, so add a few open lines with curve points inside the piece.
function syntheticLines(box, rand) {
  const p = (id, c) => ({ id, x: box.x0 + rand() * box.w, y: box.y0 + rand() * box.h, ...(c ? { c: 1 } : {}) });
  return [
    { pts: [p('i0'), p('i1')] },
    { pts: [p('i0'), p('i1', 1), p('i2')] },
    { pts: [p('i0'), p('i1', 1), p('i2', 1), p('i3'), p('i4', 1), p('i5')] },
    { pts: [p('i0'), p('i1'), p('i2', 1)] },
  ];
}

const rand = rng(20261107);
const cases = templatePieces(AT).map((piece) => pieceCase(piece, rand));
process.stdout.write(`${JSON.stringify({ generator: 'tools/golden/export-geom.mjs', pieces: cases })}\n`);
