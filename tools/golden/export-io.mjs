// Exports golden data for crates/azam-io from the reference JavaScript engine in src.js: DXF, SVG
// and OBJ export, DXF import, the tech-pack tables and CSV, pattern dimensions, the pattern PDF
// planner, and the drawing operations of the pattern and tech-pack PDFs.
//
//   (cd tools/golden && npm install)
//   node tools/golden/export-io.mjs [path/to/src.js] > crates/azam-io/tests/golden/io.json
//
// These functions live after the engine, so the sandbox loads src.js up to the React app. Ids use
// the `<prefix>_<n>` scheme of the Rust `IdGen` (as in export-doc.mjs) and timestamps are 0.
//
// PDFs: src.js draws with jsPDF 2.5.1 (loaded from a CDN in the browser, from npm here). The
// jsPDF instance handed to src.js is wrapped in a recorder that logs every drawing call (text,
// lines, shapes, clipping) with the colour, font and line state it is drawn with; the real jsPDF
// only measures and wraps text. Text outside Latin-1 is measured with the core-font metrics (in the
// browser src.js rasterises it with canvas, which cannot be reproduced headless). The date in page
// headers is fixed.
import path from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { loadEngine } from './load-engine.mjs';

const require = createRequire(import.meta.url);
const { jsPDF } = require('jspdf');

const DATE = '07/11/2026';

const here = path.dirname(fileURLToPath(import.meta.url));
const AT = loadEngine(process.argv[2] || path.join(here, '../../src.js'), { window: { jspdf: { jsPDF: recorder() } } }, { until: 'app' });

let seq = 0;
AT.uid = (p) => `${p || ''}_${++seq}`;
const baseNewProject = AT.newProject;
AT.newProject = (...args) => {
  const d = baseNewProject(...args);
  d.meta.createdAt = 0;
  d.meta.updatedAt = 0;
  return d;
};
// `Function` of the sandbox realm, to patch the sandbox's own `Date`.
AT.clamp.constructor(`Date.prototype.toLocaleDateString = function () { return ${JSON.stringify(DATE)}; };`)();
AT.pdfInstallUnicode = (pdf) => { pdf.uniWidth = (s) => pdf.getTextWidth(String(s)); return pdf; };

const snap = (v) => JSON.parse(JSON.stringify(v));
// The Rust port names the product "azam" where src.js writes "Atelier" (CSV title, OBJ header,
// PDF page headers); the golden data holds the azam-branded text.
const brand = (s) => s.replace(/Atelier/g, 'azam').replace(/ATELIER/g, 'AZAM');
const brandPages = (pages) => pages.map((pg) => ({ ...pg, ops: pg.ops.map((op) => (op[0] === 'text' ? [op[0], brand(op[1]), ...op.slice(2)] : op)) }));
const r6 = (v) => Math.round(v * 1e6) / 1e6 + 0;
const xy = (p) => [p.x, p.y];

/* ---------- recording jsPDF ---------- */
// Pattern outlines are long polylines; a path is recorded as its vertex count, end points, vertex
// sums and bounding box, which any change to a vertex shows up in.
function pathDigest(pts) {
  const n = pts.length, first = pts[0], last = pts[n - 1];
  let sx = 0, sy = 0, x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const [x, y] of pts) { sx += x; sy += y; x0 = Math.min(x0, x); y0 = Math.min(y0, y); x1 = Math.max(x1, x); y1 = Math.max(y1, y); }
  return [n, ...[first[0], first[1], last[0], last[1], sx, sy, x0, y0, x1, y1].map(r6)];
}

function recorder() {
  const color = (args) => {
    const [a, b, c] = args;
    if (typeof a === 'string' && a[0] === '#') { const n = parseInt(a.slice(1), 16); return [(n >> 16) & 255, (n >> 8) & 255, n & 255]; }
    if (b == null) return [a, a, a];
    return [a, b, c];
  };
  const pageSize = (format, orientation) => {
    const f = Array.isArray(format) ? format : { a4: [210, 297] }[format];
    const [w, h] = [Math.min(f[0], f[1]), Math.max(f[0], f[1])];
    return orientation === 'landscape' ? [h, w] : [w, h];
  };
  return function RecordingPDF(opts) {
    const real = new jsPDF({ unit: 'mm', format: 'a4' });
    const st = { text: [0, 0, 0], draw: [0, 0, 0], fill: [0, 0, 0], lw: 0.200025, dash: [[], 0], style: 'normal', size: 16 };
    const pages = [];
    let ops = null;
    const addPage = (format, orientation) => { const [w, h] = pageSize(format, orientation); ops = []; pages.push({ w: r6(w), h: r6(h), ops }); };
    addPage(opts.format, opts.orientation);
    const stroke = () => [r6(st.lw), st.draw.slice(), [st.dash[0].map(r6), r6(st.dash[1])]];
    const paint = (style) => {
      if (style === 'F') return [st.fill.slice()];
      if (style === 'FD' || style === 'DF') return [...stroke(), st.fill.slice()];
      return stroke();
    };
    let pendingClip = null;
    const pdf = {
      __recording: pages,
      internal: real.internal,
      // without arguments jsPDF repeats the document's own format and orientation
      addPage(format, orientation) { addPage(format || opts.format, format ? orientation : opts.orientation); return pdf; },
      setTextColor(...a) { st.text = color(a); return pdf; },
      setDrawColor(...a) { st.draw = color(a); return pdf; },
      setFillColor(...a) { st.fill = color(a); return pdf; },
      setLineWidth(w) { st.lw = w; return pdf; },
      setLineDashPattern(d, p) { st.dash = [d.slice(), p]; return pdf; },
      setFont(name, style) { st.style = style || 'normal'; real.setFont(name, st.style); return pdf; },
      setFontSize(s) { st.size = s; real.setFontSize(s); return pdf; },
      getFontSize() { return real.getFontSize(); },
      getFont() { return real.getFont(); },
      getTextWidth(s) { return real.getTextWidth(s); },
      getLineHeightFactor() { return real.getLineHeightFactor(); },
      splitTextToSize(s, w) { return real.splitTextToSize(s, w); },
      text(str, x, y, o) {
        if (str.length === 0) return pdf; // jsPDF draws nothing for empty text
        o = o || {};
        let lines = Array.isArray(str) ? str.slice() : String(str).match(/[\r?\n]/) ? String(str).split(/\r\n|\r|\n/g) : [String(str)];
        if (o.maxWidth > 0) lines = lines.reduce((acc, l) => acc.concat(real.splitTextToSize(l, o.maxWidth)), []);
        const leading = st.size * 1.15 / real.internal.scaleFactor;
        lines.forEach((line, i) => ops.push(['text', line, r6(x), r6(y + i * leading), o.align || 'left', o.baseline || 'alphabetic', o.angle ? r6(o.angle) : 0, st.style, st.size, st.text.slice()]));
        return pdf;
      },
      line(x1, y1, x2, y2) { ops.push(['line', r6(x1), r6(y1), r6(x2), r6(y2), ...stroke()]); return pdf; },
      lines(rel, x, y, scale, style, closed) {
        const pts = [[x, y]];
        let cx = x, cy = y;
        rel.forEach((d) => { cx += d[0]; cy += d[1]; pts.push([cx, cy]); });
        ops.push(['path', pathDigest(pts), !!closed, style, ...paint(style)]);
        return pdf;
      },
      rect(x, y, w, h, style) {
        if (style === null) { pendingClip = [r6(x), r6(y), r6(w), r6(h)]; return pdf; }
        ops.push(['rect', r6(x), r6(y), r6(w), r6(h), style || 'S', ...paint(style || 'S')]);
        return pdf;
      },
      circle(x, y, r, style) { ops.push(['circle', r6(x), r6(y), r6(r), style || 'S', ...paint(style || 'S')]); return pdf; },
      triangle(x1, y1, x2, y2, x3, y3, style) { ops.push(['triangle', [x1, y1, x2, y2, x3, y3].map(r6), style || 'S', ...paint(style || 'S')]); return pdf; },
      saveGraphicsState() { ops.push(['save']); return pdf; },
      restoreGraphicsState() { ops.push(['restore']); return pdf; },
      clip() { ops.push(['clip', ...pendingClip]); pendingClip = null; return pdf; },
      discardPath() { return pdf; },
      addImage() { throw new Error('images are not recorded'); },
    };
    return pdf;
  };
}

/* ---------- projects ---------- */
const newDoc = (tid) => { seq = 0; return AT.normalize(AT.fromTemplate(tid, 'female')); };

// The tee with every feature the exporters draw: hems, per-edge allowances, notches, open, curved
// and closed internal lines, a fold line, buttons, a buttonhole, topstitches, comments, a priced
// fabric, a second colorway, quantities, grain, a mirror link and grading in the active size.
function sampler() {
  const doc = newDoc('tee');
  doc.meta.name = 'Sampler "tee", <test> & co';
  const pcs = AT.asArray(doc.pieces);
  const [front, back, sleeve] = pcs;
  const olF = AT.outline(front.pts), olB = AT.outline(back.pts), olS = AT.outline(sleeve.pts);
  const lowest = (ol) => ol.segs.reduce((b, sg) => { const y = sg.poly.reduce((s, p) => s + p.y, 0) / sg.poly.length; return y > b.y ? { y, id: sg.id } : b; }, { y: -Infinity, id: null }).id;
  front.hem = { [lowest(olF)]: 30 };
  back.hem = { [lowest(olB)]: 25 };
  sleeve.hem = { [lowest(olS)]: 20 };
  front.saSeg = { [olF.segs[1].id]: 15 };
  front.notches = [{ id: 'n_a', s: olF.segs[1].id, t: 0.35 }, { id: 'n_b', s: olF.segs[2].id, t: 0.6 }, { id: 'n_c', s: olF.segs[1].id, t: 0.8 }];
  back.notches = [{ id: 'n_d', s: olB.segs[0].id, t: 0.5 }];
  const bb = AT.bbox(olF.poly);
  const at = (fx, fy) => ({ x: bb.x0 + bb.w * fx, y: bb.y0 + bb.h * fy });
  front.internals = [
    { id: 'il_a', pts: [at(0.3, 0.3), at(0.3, 0.7)] },
    { id: 'il_b', pts: [at(0.4, 0.5), { ...at(0.5, 0.35), c: 1 }, at(0.6, 0.5)], fold: 90 },
    { id: 'il_c', pts: [at(0.45, 0.75), at(0.55, 0.75), at(0.5, 0.85)], closed: true },
  ];
  front.qty = 2;
  back.grain = 15;
  sleeve.qty = 2;
  sleeve.state = 'off';
  doc.buttons = {
    b_1: { id: 'b_1', p: front.id, x: at(0.5, 0.3).x, y: at(0.5, 0.3).y, size: 15, color: '#222222' },
    b_2: { id: 'b_2', p: front.id, x: at(0.5, 0.4).x, y: at(0.5, 0.4).y, size: 15, color: '#222222' },
    b_3: { id: 'b_3', p: front.id, x: at(0.5, 0.5).x, y: at(0.5, 0.5).y, shape: 'square', size: 20, color: '#aa3300', finish: 'metal' },
  };
  doc.holes = { h_1: { id: 'h_1', p: front.id, x: 10, y: 10 } };
  doc.stitches.st_x = { id: 'st_x', p: back.id, seg: '*', rows: 2 };
  doc.comments = {
    c_1: { id: 'c_1', p: front.id, x: 0, y: 0, n: 1, text: 'Check the hem depth', byName: 'Azam' },
    c_2: { id: 'c_2', p: back.id, x: 0, y: 0, n: 2, text: 'Resolved note', resolved: true },
    c_3: { id: 'c_3', x: 0, y: 0, n: 3, text: 'A long review note about the neckline that needs to be truncated in the table because it is far too long', byName: 'Reviewer' },
  };
  const fab = Object.values(doc.fabrics)[0];
  Object.assign(fab, { price: 12.5, width: 1500, waste: 0.1, content: '95% cotton, 5% elastane' });
  doc.colorways.cw_2 = { id: 'cw_2', name: 'Night', order: 1, over: { [fab.id]: { color: '#202840' } } };
  doc.activeColorway = 'cw_2';
  doc.poms[Object.keys(doc.poms)[0]].tol = 10;
  // grading: move one corner of the front in size L, and show size L
  front.grade = { [front.pts[0].id]: { L: { dx: 12, dy: -6 }, XL: { dx: 20, dy: -9 } } };
  doc.activeSize = 'L';
  return doc;
}

const projects = [...AT.TEMPLATES.map((t) => ({ name: t.id, doc: newDoc(t.id) })), { name: 'sampler', doc: sampler() }];

/* ---------- dimensions ---------- */
function dimensions(doc) {
  const nums = AT.seamNumbers(doc);
  const items = AT.ppdfItems(doc, { ids: Object.keys(doc.pieces) });
  return items.map((it) => {
    const pc = it.pc, t = AT.dimTableRows(doc, it, nums), pole = AT.labelPole(it.g.ol);
    return {
      id: pc.id,
      dims: AT.pieceDims(doc, pc),
      allowance: AT.allowanceSummary(pc),
      saMax: AT.saMax(pc),
      box: [it.box.x0, it.box.y0, it.box.x1, it.box.y1],
      sym: t.dm.sym,
      cx: t.dm.cx,
      corners: t.dm.corners.map((c) => ({ L: c.L, x: c.x, y: c.y, X: c.X, Y: c.Y, ang: c.ang, smooth: c.smooth })),
      edges: t.dm.edges.map((e) => ({ from: e.from, to: e.to, curve: e.curve, len: e.len, chord: e.chord, allow: e.allow, hem: e.hem })),
      seams: t.rows.map((r) => r.seams),
      notches: t.notches,
      pole: { x: pole.x, y: pole.y, s: pole.s, d: pole.d, cands: pole.cands.slice(0, 10).map((c) => [c.x, c.y, c.s]) },
    };
  });
}

/* ---------- pattern PDF ---------- */
const PDF_CASES = [
  { layout: 'tiled', paper: 'a4', units: 'cm', sa: true, lengths: false, dims: false },
  { layout: 'tiled', paper: 'letter', units: 'in', sa: true, lengths: true, dims: true, sizes: 'all' },
  { layout: 'pieces', paper: 'a4', units: 'cm', sa: true, lengths: false, dims: true },
  { layout: 'pieces', paper: 'a3', units: 'mm', sa: false, lengths: true, dims: false },
  { layout: 'overview', paper: 'a4', units: 'cm', sa: true, lengths: true, dims: true, sizes: 'all' },
  { layout: 'plotter', paper: 'a4', units: 'cm', sa: true, lengths: false, dims: true, plotter: 914 },
];

function planSummary(doc, o) {
  const p = AT.ppdfPlan(doc, o);
  return {
    pages: p.pages, note: p.note, sewPages: p.sewPages ?? 0, dimPages: p.dimPages ?? 0,
    items: p.items.map((it) => ({ id: it.pc.id, k: it.k ?? null, land: it.land ?? null })),
    pack: p.pk ? { w: p.pk.w, h: p.pk.h, pos: p.pk.pos.map((q) => [q.x, q.y, q.w, q.h]), rows: p.pk.rows.map((r) => ({ y: r.y, h: r.h, idx: r.idx })) } : null,
    cols: p.cols ?? null, rows: p.rows ?? null, tw: p.tw ?? null, th: p.th ?? null, used: p.used ?? null,
    roll: p.roll ?? null, sheets: p.sheets ? p.sheets.map((s) => ({ y0: s.y0, h: s.h, idx: s.idx })) : null,
  };
}

function patternPdf(doc, o) {
  if (!AT.ppdfItems(doc, o).length) return null;
  return brandPages(AT.patternPDF(doc, o, {}).__recording);
}

/* ---------- OBJ ---------- */
// A small synthetic drape: each piece is a fan around its centroid, lifted into 3D.
function objCase(doc) {
  const recs = [];
  let off = 0;
  const pos = [];
  AT.asArray(doc.pieces).forEach((pc, i) => {
    const ol = AT.outline(AT.gradedPts(pc, doc.activeSize, doc.baseSize));
    const step = Math.max(1, Math.floor(ol.poly.length / 24));
    const ring = ol.poly.filter((_, j) => j % step === 0);
    const c = AT.bbox(ring);
    const pts2 = [{ x: c.cx, y: c.cy }, ...ring];
    const xy2 = new Float64Array(pts2.length * 2);
    pts2.forEach((p, j) => { xy2[2 * j] = p.x; xy2[2 * j + 1] = p.y; });
    const tris = [];
    for (let j = 1; j <= ring.length; j++) tris.push(0, j, j === ring.length ? 1 : j + 1);
    pts2.forEach((p) => pos.push(p.x * 0.4 - 300 + i * 25, 1500 - p.y * 0.4, 120 * Math.sin(p.x / 400) + i * 3));
    recs.push({ pc, n: pts2.length, off, mesh: { xy: xy2, tris: new Uint32Array(tris) } });
    off += pts2.length;
  });
  if (doc.name === 'sampler' && recs.length) recs[recs.length - 1].pc = { ...recs[recs.length - 1].pc, hide3d: true };
  const sim = { recs, N: off, x: Float32Array.from(pos), nrm: new Float32Array(off * 3) };
  const av = { pos: Float32Array.from([0, 0, 0, 100, 0, 0, 0, 100, 0, 0, 0, 100]), nor: Float32Array.from([0, 0, 1, 0, 0, 1, 0, 0, 1, 0.6, 0.8, 0]), idx: Uint32Array.from([0, 1, 2, 0, 2, 3]) };
  const out = AT.exportOBJ(doc, sim, av, true);
  AT.computeNormals(sim);
  return {
    pieces: recs.map((r) => ({
      id: r.pc.id, hidden: !!r.pc.hide3d,
      positions: Array.from(sim.x.slice(r.off * 3, (r.off + r.n) * 3)),
      normals: Array.from(sim.nrm.slice(r.off * 3, (r.off + r.n) * 3)),
      uvs: Array.from(r.mesh.xy), triangles: Array.from(r.mesh.tris),
    })),
    avatar: { positions: Array.from(av.pos), normals: Array.from(av.nor), triangles: Array.from(av.idx) },
    obj: brand(out.obj), mtl: out.mtl,
  };
}

/* ---------- DXF import of foreign files ---------- */
const FOREIGN_DXF = {
  lwpolyline: ['0', 'SECTION', '2', 'ENTITIES', '0', 'LWPOLYLINE', '8', 'CUT', '90', '4', '70', '1', '10', '0', '20', '0', '10', '100', '20', '0', '10', '100.2', '20', '50', '10', '0', '20', '50', '0', 'LINE', '8', '7', '10', '0', '20', '0', '11', '5', '21', '5', '0', 'ENDSEC', '0', 'EOF'].join('\r\n'),
  closedByEnds: ['0', 'SECTION', '2', 'ENTITIES', '0', 'POLYLINE', '8', '1', '66', '1', '70', '0',
    ...[[0, 0], [40, 0], [40.0001, 10], [40, 20], [0, 20], [0, 0.2]].flatMap(([x, y]) => ['0', 'VERTEX', '8', '1', '10', String(x), '20', String(y), '30', '0']),
    '0', 'SEQEND', '0', 'POLYLINE', '8', '1', '66', '1', '70', '0', '0', 'VERTEX', '10', '1', '20', '1', '0', 'VERTEX', '10', '2', '20', '2', '0', 'SEQEND', '0', 'ENDSEC', '0', 'EOF'].join('\n'),
  blockByArea: ['0', 'SECTION', '2', 'BLOCKS', '0', 'BLOCK', '8', '0', '2', 'PANEL', '70', '0',
    '0', 'POLYLINE', '8', '3', '70', '1', ...[[0, 0], [10, 0], [10, 10], [0, 10]].flatMap(([x, y]) => ['0', 'VERTEX', '10', String(x), '20', String(y)]), '0', 'SEQEND',
    '0', 'POLYLINE', '8', '5', '70', '1', ...[[0, 0], [30, 0], [30, 30], [15, 30.2], [0, 30]].flatMap(([x, y]) => ['0', 'VERTEX', '10', String(x), '20', String(y)]), '0', 'SEQEND',
    '0', 'POLYLINE', '8', '8', '70', '0', ...[[5, 5], [10, 5.1], [20, 5]].flatMap(([x, y]) => ['0', 'VERTEX', '10', String(x), '20', String(y)]), '0', 'SEQEND',
    '0', 'TEXT', '8', '1', '1', 'PIECE NAME: Yoke', '0', 'ENDBLK', '0', 'ENDSEC', '0', 'EOF'].join('\n'),
};
const imported = (text) => AT.importDXF(text).map((p) => ({
  name: p.name, pts: p.pts.map(xy), internals: p.internals.map((il) => ({ pts: il.pts.map(xy), closed: il.closed })),
}));

/* ---------- tech pack PDF ---------- */
const TECH_INFO = { season: 'SS27', designer: 'Studio azam', currency: 'EUR', notes: 'Pre-wash all fabrics. Topstitch hems at 2.5 mm stitch length; keep seam allowances pressed towards the back.\nSecond paragraph of the notes.' };

const cases = projects.map(({ name, doc }) => {
  const dxf = AT.exportDXF(doc);
  return {
    name,
    project: snap(doc),
    dxf,
    dxfImport: imported(dxf),
    svg: AT.exportSVG(doc),
    techData: snap(AT.techData(doc)),
    csv: brand(AT.techCSV(doc)),
    dimensions: dimensions(doc),
    plans: PDF_CASES.map((o) => ({ options: o, plan: planSummary(doc, o) })),
    obj: objCase({ ...doc, name }),
  };
});

const pdfs = [];
for (const { name, doc } of projects) {
  PDF_CASES.forEach((o, i) => {
    // every template in the default tiled layout; the other layouts on the tee, the labadeh and the
    // sampler (the fully dimensioned tiles only on the tee and the sampler, to keep the file small)
    if (i > 0 && !['tee', 'labadeh', 'sampler'].includes(name)) return;
    if (i === 1 && name === 'labadeh') return;
    const pages = patternPdf(doc, o);
    if (pages) pdfs.push({ project: name, options: o, pages });
  });
}
const techPdfs = projects.map(({ name, doc }) => {
  const info = name === 'sampler' ? TECH_INFO : {};
  return { project: name, info, pages: brandPages(AT.techPDF(doc, {}, info).__recording) };
});

process.stdout.write(`${JSON.stringify({
  generator: 'tools/golden/export-io.mjs',
  date: DATE,
  projects: cases,
  foreignDxf: Object.entries(FOREIGN_DXF).map(([name, text]) => ({ name, text, pieces: imported(text) })),
  patternPdfs: pdfs,
  techPdfs,
})}\n`);
