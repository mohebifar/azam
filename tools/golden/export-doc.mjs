// Exports golden data for crates/azam-doc from the reference JavaScript engine in src.js.
//
//   node tools/golden/export-doc.mjs [path/to/src.js] > crates/azam-doc/tests/golden/doc.json
//
// Ids: src.js makes ids from Math.random and the clock. Here `AT.uid` is replaced by the scheme the
// Rust `IdGen` uses (`<prefix>_<n>`, counting from 1 in every new project), so generated ids match
// exactly. A loaded project continues after the largest `<lowercase>_<digits>` id it contains, as
// the Rust generator does when it seeds itself. Project timestamps are set to 0.
//
// Edit steps are written as Rust `Command` values (serde's externally tagged enum shape); the
// `run` function below performs the same edit the way the src.js UI does it.
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { loadEngine, templates } from './load-engine.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const AT = loadEngine(process.argv[2] || path.join(here, '../../src.js'));

let seq = 0;
AT.uid = (p) => `${p || ''}_${++seq}`;
const baseNewProject = AT.newProject;
AT.newProject = (...args) => {
  const d = baseNewProject(...args);
  d.meta.createdAt = 0;
  d.meta.updatedAt = 0;
  return d;
};

const snap = (v) => JSON.parse(JSON.stringify(v));
const maxSuffix = (doc) => {
  let max = 0;
  const visit = (v) => {
    if (typeof v === 'string') {
      const m = /^[a-z]+_(\d+)$/.exec(v);
      if (m) max = Math.max(max, Number(m[1]));
    } else if (Array.isArray(v)) v.forEach(visit);
    else if (v && typeof v === 'object') Object.entries(v).forEach(([k, x]) => { visit(k); visit(x); });
  };
  visit(snap(doc));
  return max;
};
const newDoc = (tid, sex) => { seq = 0; return AT.normalize(AT.fromTemplate(tid, sex)); };
const loaded = (doc) => { seq = maxSuffix(doc); return doc; };
const xy = (p) => [p.x, p.y];

/* ---------- fabrics ---------- */
function fabricCases() {
  const ks = [-10, 0, 3.5, 16, 50, 72, 99.9, 100, 130];
  const functions = ks.map((k) => ({
    k, e2d: AT.E2D(k), rStretch: AT.rStretch(k), rBend: AT.rBend(k), limitStrain: AT.limitStrain(k),
    stress: [AT.stressKPa(k, 0.05, 0.3), AT.stressKPa(k, 0.1, undefined), AT.stressKPa(k, 0.02, 0), AT.stressKPa(k, 0.2, 0.05)],
  }));
  const presets = Object.keys(AT.FABRIC_PRESETS).map((key) => {
    seq = 0;
    const f = AT.makeFabric(key);
    const ph = f.phys, kAvg = (ph.stretchWeft + ph.stretchWarp) / 2;
    return {
      key, fabric: snap(f),
      derived: {
        e2d: [AT.E2D(ph.stretchWeft), AT.E2D(ph.stretchWarp), AT.E2D(ph.shear)],
        rStretch: [AT.rStretch(ph.stretchWeft), AT.rStretch(ph.stretchWarp), AT.rStretch(ph.shear)],
        rBend: [AT.rBend(ph.bendWeft), AT.rBend(ph.bendWarp)],
        limitStrain: AT.limitStrain(kAvg),
        stress: AT.stressKPa(kAvg, 0.1, ph.thickness),
      },
    };
  });
  return { physKeys: AT.PHYS_KEYS, functions, presets };
}

/* ---------- queries on a project ---------- */
function queries(doc) {
  const pieces = Object.values(doc.pieces);
  const nums = AT.seamNumbers(doc);
  const pomValues = Object.values(doc.poms).map((pm) => ({ id: pm.id, values: Object.fromEntries(doc.sizes.map((s) => [s, AT.pomValue(doc, pm, s)])) }));
  const areas = pieces.map((pc) => ({
    id: pc.id,
    net: Object.fromEntries(doc.sizes.map((s) => [s, AT.pieceArea(doc, pc, s, false)])),
    cut: Object.fromEntries(doc.sizes.map((s) => [s, AT.pieceArea(doc, pc, s, true)])),
  }));
  const coverage = pieces.map((pc) => {
    const ol = AT.outline(pc.pts), c = AT.edgeCoverage(doc, pc, ol, nums);
    return {
      id: pc.id,
      sides: c.sides.map((s) => ({ start: s.P0, end: s.P1, seam: s.sw.id, number: s.n, partners: s.partners })),
      sewn: c.sewn, free: c.free,
      freeParts: c.freeParts.map((f) => ({ start: f.P0, end: f.P1, segment: f.k, segmentId: f.sid })),
      total: c.T,
    };
  });
  const sideSegs = Object.values(doc.sewing).map((sw) => ({
    id: sw.id,
    sa: AT.seamSA(doc, sw),
    a: sw.a.map((sd) => (doc.pieces[sd.p] ? AT.sideSegIds(AT.outline(doc.pieces[sd.p].pts), sd) : [])),
    b: sw.b.map((sd) => (doc.pieces[sd.p] ? AT.sideSegIds(AT.outline(doc.pieces[sd.p].pts), sd) : [])),
  }));
  const fabrics = [];
  const keep = doc.activeColorway;
  Object.keys(doc.colorways).forEach((cw) => {
    doc.activeColorway = cw;
    Object.keys(doc.fabrics).forEach((fid) => fabrics.push({ colorway: cw, fabric: fid, value: snap(AT.effFabric(doc, fid)) }));
  });
  doc.activeColorway = keep;
  const mirrors = pieces.filter((pc) => pc.mirror && doc.pieces[pc.mirror.src]).map((pc) => {
    const src = doc.pieces[pc.mirror.src];
    return {
      id: pc.id,
      toMirror: src.pts.filter((p) => !p.c).map((p) => [p.id, AT.mirrorSegId(src, p.id)]),
      toSource: pc.pts.filter((p) => !p.c).map((p) => [p.id, AT.srcSegId(src, p.id)]),
    };
  });
  return {
    sewLengths: AT.sewLengths(doc), seamNumbers: nums, pomValues, areas, coverage, sideSegs, fabrics, mirrors,
    defaultHems: Object.fromEntries(pieces.map((pc) => [pc.id, AT.defaultHem(doc, pc)])),
    detectHems: AT.detectHems(doc),
  };
}

function templateCases() {
  const out = [];
  for (const t of templates(AT)) {
    for (const sex of ['female', 'male']) {
      const doc = newDoc(t.id, sex);
      out.push({ template: t.id, sex, lastId: seq, doc: snap(doc), queries: queries(doc) });
    }
  }
  return out;
}

/* ---------- curves ---------- */
function curveCases() {
  const doc = newDoc('tee', 'female');
  const pcs = Object.values(doc.pieces);
  const segParams = [];
  pcs.slice(0, 3).forEach((pc) => {
    const ol = AT.outline(pc.pts);
    ol.segs.forEach((sg) => {
      segParams.push({ piece: pc.id, segment: sg.id, values: [0, 0.07, 0.3, 0.5, 0.77, 1].map((t) => [t, AT.segBezParam(sg, t)]) });
    });
  });
  const ctrls = [
    [[0, 0], [100, 0]],
    [[0, 0], [50, -40], [100, 0]],
    [[0, 0], [30, 80], [70, -60], [100, 10]],
    [[5, 5], [10, 60], [80, 90], [120, 30], [150, 0]],
  ];
  const splits = [];
  ctrls.forEach((c) => [0, 0.25, 0.5, 0.9, 1].forEach((t) => {
    const r = AT.deCasteljauSplit(c.map(([x, y]) => ({ x, y })), t);
    splits.push({ ctrl: c, t, left: r.left.map(xy), right: r.right.map(xy) });
  }));
  const old = pcs[0].pts;
  const moved = old.slice(2).map((p, i) => {
    const a = 0.2, x = p.x * Math.cos(a) - p.y * Math.sin(a) + 40, y = p.x * Math.sin(a) + p.y * Math.cos(a) - 15;
    return { id: i === 3 ? 'renamed' : p.id, x: x + (i % 3) * 0.7, y, c: p.c };
  });
  const queriesAt = [[0, 0], [100, 50], [-30, 400], [250, 250]];
  const rm = AT.rigidMap(old, moved);
  const rigid = {
    old: old.map((p) => [p.id, p.x, p.y]), new: moved.map((p) => [p.id, p.x, p.y]),
    mapped: queriesAt.map(([x, y]) => [x, y, ...rm(x, y)]),
    tooFew: AT.rigidMap(old, moved.slice(0, 1)) === null,
  };
  return { segParams, splits, rigid };
}

/* ---------- normalize ---------- */
function normalizeCases() {
  const side = (p, s, dir) => ({ p, a: { s, t: 0 }, b: { s, t: 1 }, dir });
  const inputs = [
    {},
    {
      meta: { name: 'Loose', note: 'kept' },
      avatar: { sex: 'male', height: 1850, kind: 'parametric' },
      pieces: {
        pcA: {
          name: 'A', fabric: 'fab_main', grain: 0, pd: 20, layer: 0, state: 'active', order: 0,
          pts: { 2: { id: 'c', x: 100, y: 100 }, 0: { id: 'a', x: 0, y: 0 }, 1: { id: 'b', x: 100, y: 0 } },
          internals: { b: { id: 'il_9', pts: { 1: { x: 5, y: 5 }, 0: { x: 1, y: 1, c: 1 } } } },
          notches: [null, { id: 'n_1', s: 'a', t: 0.5 }],
          pins: null,
          sa: null,
        },
        pcNull: null,
      },
      sewing: { s1: { a: { 0: side('pcA', 'a', 1) }, b: [side('pcA', 'b', -1), null], color: '#fff' }, s2: null },
      poms: { pm1: { name: 'x', refs: { 0: { p: 'pcA', s: 'a' } } } },
      sizes: { 1: 'L', 0: 'M' },
      colorways: {},
      activeColorway: 'nope',
      fabrics: 'bad',
      units: '',
      xray: { deep: [1, { a: null }] },
    },
    {
      meta: null,
      sizes: [],
      baseSize: 'S',
      colorways: { cw_b: { id: 'cw_b', name: 'B', order: 1, over: {} }, cw_a: { id: 'cw_a', name: 'A', order: 0, over: {} } },
      activeColorway: 'cw_zzz',
      sim: { preset: 'fine', over: { substeps: 12 } },
      units: 'mm',
    },
  ];
  return inputs.map((input) => ({ input: snap(input), output: snap(AT.normalize(snap(input))) }));
}

/* ---------- edits ---------- */
const sideOf = (sd) => ({ p: sd.p, a: sd.a, b: sd.b, dir: sd.dir });
// Performs a command the way the src.js UI does, then syncs mirrors as its commit does.
function run(d, command) {
  const [kind, c] = Object.entries(command)[0];
  const pc = c && c.piece ? d.pieces[c.piece] : null;
  let out = [];
  const outline = (fn) => { const old = AT.clone(pc.pts); const id = fn(); AT.remapRefs(d, pc.id, old); return id; };
  switch (kind) {
    case 'CreatePiece': {
      const pts = c.points.map(([q, curve]) => ({ x: q[0], y: q[1], c: curve ? 1 : undefined }));
      if (pts[0].c) delete pts[0].c;
      const npc = AT.newPiece('Pattern ' + (Object.keys(d.pieces).length + 1), pts.map((p) => ({ id: AT.uid('p'), x: p.x, y: p.y, c: p.c })), { fabric: Object.keys(d.fabrics)[0] });
      AT.addPiece(d, npc); out = [{ Piece: npc.id }]; break;
    }
    case 'AddInternalLine': {
      const id = AT.uid('il');
      pc.internals = (pc.internals || []).concat([{ id, pts: c.points.map((p) => ({ x: p.x, y: p.y, c: p.c })), closed: !!c.closed, fold: null }]);
      out = [{ InternalLine: id }]; break;
    }
    case 'SplitSegment': out = [{ Point: outline(() => AT.splitSegment(pc, c.segment, c.t)) }]; break;
    case 'AddCurvePoint': out = [{ Point: outline(() => AT.addCurvePoint(pc, c.segment, c.t, c.at[0], c.at[1])) }]; break;
    case 'SetCurvature': AT.setCurvature(pc, c.segment, c.through[0], c.through[1]); break;
    case 'MovePoints': pc.pts.forEach((p) => { if (c.points.indexOf(p.id) >= 0) { p.x += c.delta[0]; p.y += c.delta[1]; } }); break;
    case 'DeletePoints': {
      const left = pc.pts.filter((p) => c.points.indexOf(p.id) < 0);
      if (left.filter((p) => !p.c).length < 3) throw new Error('too few corners');
      outline(() => { pc.pts = left; }); break;
    }
    case 'TransformPieces': {
      const [tk, tv] = Object.entries(c.transform)[0];
      let fn;
      if (tk === 'Translate') fn = (x, y) => [x + tv[0], y + tv[1]];
      else if (tk === 'Rotate') { const co = Math.cos(tv.angle), s = Math.sin(tv.angle), cx = tv.center; fn = (x, y) => [cx[0] + (x - cx[0]) * co - (y - cx[1]) * s, cx[1] + (x - cx[0]) * s + (y - cx[1]) * co]; }
      else throw new Error('transform ' + tk);
      c.pieces.forEach((id) => AT.transformPiece(d, id, fn)); break;
    }
    case 'FlipPieces': flip(d, c.pieces, c.axis === 'Horizontal' ? 'h' : 'v'); break;
    case 'RotatePieces90': c.pieces.forEach((id) => { const p = d.pieces[id]; if (!p || p.mirror) return; const bb = AT.bbox(p.pts); AT.transformPiece(d, id, (x, y) => [bb.cx - (y - bb.cy), bb.cy + (x - bb.cx)]); }); break;
    case 'DuplicatePiece': out = [{ Piece: AT.duplicatePiece(d, c.piece, false).id }]; break;
    case 'MirrorPiece': out = [{ Piece: AT.duplicatePiece(d, c.piece, true, c.linked).id }]; break;
    case 'DeletePieces': AT.deletePieces(d, c.pieces); break;
    case 'AddSeam': out = [{ Seam: AT.addSew(d, c.a.map(sideOf), c.b.map(sideOf)) }]; break;
    case 'DeleteSeam': delete d.sewing[c.seam]; break;
    case 'SetSeamAllowance': AT.setSeamSA(d, c.seam, c.mm); break;
    case 'SetSegmentAllowance': setSegSA(d, c.piece, c.segment, c.mm); break;
    case 'SetAllAllowances': Object.values(d.pieces).forEach((p) => { p.sa = c.mm; delete p.saSeg; }); break;
    case 'SetHem': setHem(d, c.piece, c.segment, c.mm); break;
    default:
      if (command === 'MarkHems') { markHems(d); break; }
      throw new Error('unknown command ' + kind);
  }
  AT.syncMirrors(d);
  return out;
}
// The src.js UI's handlers for these edits (App actions).
function flip(D0, pieces, axis) {
  pieces.filter((i) => D0.pieces[i] && !D0.pieces[i].mirror).forEach((id) => {
    const pc = D0.pieces[id], bb = AT.bbox(pc.pts), mx = (x, y) => (axis === 'h' ? [2 * bb.cx - x, y] : [x, 2 * bb.cy - y]);
    const segs = AT.segInfo(pc.pts), next = {}; segs.forEach((sg) => { next[sg.id] = pc.pts[sg.i1].id; });
    AT.transformPiece(D0, id, mx); pc.pts.reverse();
    const ms = (s) => next[s] || s;
    Object.values(D0.sewing).forEach((sw) => ['a', 'b'].forEach((k) => sw[k].forEach((sd) => { if (sd.p !== id) return; sd.a = { s: ms(sd.a.s), t: 1 - sd.a.t }; sd.b = { s: ms(sd.b.s), t: 1 - sd.b.t }; sd.dir = -sd.dir; })));
    (pc.notches || []).forEach((n) => { n.s = ms(n.s); n.t = 1 - (n.t || 0); });
    Object.values(D0.stitches).forEach((st) => { if (st.p === id && st.seg !== '*') st.seg = ms(st.seg); });
    Object.values(D0.poms).forEach((pm) => pm.refs.forEach((r) => { if (r.p === id) r.s = ms(r.s); }));
    ['saSeg', 'elastic'].forEach((k) => { if (pc[k]) { const o = {}; Object.keys(pc[k]).forEach((s) => { o[ms(s)] = pc[k][s]; }); pc[k] = o; } });
    if (axis === 'h') pc.grain = -(pc.grain || 0);
    if (pc.grade) Object.values(pc.grade).forEach((r) => Object.values(r).forEach((g) => { if (axis === 'h') g.dx = -g.dx; else g.dy = -g.dy; }));
  });
}
function setSegSA(d0, pid, sid, v) {
  const pc = d0.pieces[pid];
  const ol = AT.outline(pc.pts), cov = AT.edgeCoverage(d0, pc, ol, null), k = ol.byId[sid];
  const s0 = ol.start[k], s1 = s0 + ol.segs[k].len, T = ol.total, seams = {};
  cov.sides.forEach((x) => { if (Math.min(s1, x.P1) - Math.max(s0, x.P0) > 0.5 || Math.min(s1 + T, x.P1) - Math.max(s0 + T, x.P0) > 0.5) seams[x.sw.id] = 1; });
  const ids = Object.keys(seams);
  if (ids.length) ids.forEach((id) => AT.setSeamSA(d0, id, v));
  else {
    let tp = pc, ts = sid;
    if (pc.mirror && d0.pieces[pc.mirror.src]) { const s2 = AT.srcSegId(d0.pieces[pc.mirror.src], sid); if (s2) { tp = d0.pieces[pc.mirror.src]; ts = s2; } }
    tp.saSeg = Object.assign({}, tp.saSeg || {}, { [ts]: v }); AT.syncMirrors(d0);
  }
}
function setHem(d0, pid, sid, w) {
  let pc = d0.pieces[pid];
  if (pc.mirror && d0.pieces[pc.mirror.src]) { const srcPc = d0.pieces[pc.mirror.src], s2 = AT.srcSegId(srcPc, sid); if (s2) { pc = srcPc; sid = s2; } }
  const h = Object.assign({}, pc.hem || {}); if (w > 0) h[sid] = w; else delete h[sid];
  if (Object.keys(h).length) pc.hem = h; else delete pc.hem;
  AT.syncMirrors(d0);
}
function markHems(d0) {
  const r = AT.detectHems(d0);
  Object.keys(r).forEach((id) => { const pc = d0.pieces[id], h = Object.assign({}, pc.hem || {}); Object.keys(r[id]).forEach((k) => { if (h[k] == null) h[k] = r[id][k]; }); pc.hem = h; });
  AT.syncMirrors(d0);
}

function sequence(name, start, script) {
  const d = loaded(start);
  const first = snap(d);
  const steps = [];
  script(d, (command) => {
    const created = run(d, command);
    // Recorded as the app reloads it (normalized): new pieces then list their empty pins and
    // graphics, as every piece of a loaded project does.
    steps.push({ command, created, doc: snap(AT.normalize(snap(d))) });
    return created.length ? Object.values(created[0])[0] : null;
  });
  return { name, start: first, steps, queries: queries(d) };
}

const ordered = (d) => Object.values(d.pieces).sort((a, b) => a.order - b.order).map((pc) => pc.id);
const segSide = (p, s, rev) => AT.side({ id: p }, s, rev);
const strip = (sd) => ({ p: sd.p, a: sd.a, b: sd.b, dir: sd.dir });

// Every kind of edit, on the tee drafted for the male avatar.
function fullEdits() {
  return sequence('tee full', newDoc('tee', 'male'), (d, step) => {
    const [FB, BK, SL, SR, CO] = ordered(d);
    const FA = SL;
    step({ SplitSegment: { piece: FB, segment: 'hemL', t: 0.37 } });
    step({ AddCurvePoint: { piece: BK, segment: 'shL', t: 0.4, at: [d.pieces[BK].pts.find((p) => p.id === 'shL').x - 30, 60] } });
    const ua = d.pieces[FB].pts.find((p) => p.id === 'uaL'), hem = d.pieces[FB].pts.find((p) => p.id === 'hemL');
    step({ SetCurvature: { piece: FB, segment: 'uaL', through: [(ua.x + hem.x) / 2 - 25, (ua.y + hem.y) / 2] } });
    const bb = AT.bbox(d.pieces[BK].pts);
    step({ TransformPieces: { pieces: [BK], transform: { Rotate: { center: [bb.cx, bb.cy], angle: 7 * Math.PI / 180 } } } });
    const dup = step({ DuplicatePiece: { piece: SL } });
    const mir = step({ MirrorPiece: { piece: dup, linked: true } });
    const capSeam = Object.values(d.sewing).find((sw) => sw.a[0].p === SL && sw.b.length === 2).id;
    step({ SetSeamAllowance: { seam: capSeam, mm: 15 } });
    step({ SetSegmentAllowance: { piece: SR, segment: 'hemL', mm: 6 } });
    step({ SetSegmentAllowance: { piece: BK, segment: 'cbk', mm: 7 } });
    step('MarkHems');
    step({ SetHem: { piece: FA, segment: 'hemL', mm: 12 } });
    step({ SetHem: { piece: FB, segment: 'hemL', mm: 0 } });
    step({ SplitSegment: { piece: dup, segment: d.pieces[dup].pts[0].id, t: 0.5 } });
    step({ SetHem: { piece: mir, segment: d.pieces[mir].pts.filter((p) => !p.c)[1].id, mm: 18 } });
    step({ DeletePieces: { pieces: [dup] } });
    const bkPts = d.pieces[BK].pts.filter((p) => !p.c);
    step({ DeletePoints: { piece: BK, points: [bkPts[bkPts.length - 3].id] } });
    step({ FlipPieces: { pieces: [SR, FA], axis: 'Horizontal' } });
    step({ FlipPieces: { pieces: [CO], axis: 'Vertical' } });
    step({ RotatePieces90: { pieces: [CO] } });
    step({ TransformPieces: { pieces: [FB], transform: { Translate: [25, -10] } } });
    step({ MovePoints: { piece: FB, points: ['hpsR', 'shR'], delta: [3, -2] } });
    step({ AddInternalLine: { piece: FB, points: [{ x: 0, y: 200 }, { x: 20, y: 300, c: 1 }, { x: 0, y: 400 }], closed: false } });
    step({ TransformPieces: { pieces: [FB], transform: { Translate: [0, 5] } } });
    step({ MirrorPiece: { piece: CO, linked: false } });
    const sw = step({ AddSeam: { a: [strip(segSide(SL, 'hemL', false))], b: [strip(segSide(SR, 'hemL', true))] } });
    step({ DeleteSeam: { seam: sw } });
    step({ SetAllAllowances: { mm: 11 } });
    step({ CreatePiece: { points: [[[0, -500], true], [[200, -500], false], [[260, -350], true], [[200, -200], false], [[0, -200], false]] } });
  });
}

function teeEdits() {
  return sequence('tee', newDoc('tee', 'female'), (d, step) => {
    const [F, Bk, SL, , RIB] = ordered(d);
    step({ SplitSegment: { piece: F, segment: 'uaL', t: 0.5 } });
    step({ SetCurvature: { piece: Bk, segment: 'hemL', through: [d.pieces[Bk].pts.find((p) => p.id === 'hemL').x + 200, 700] } });
    step({ AddCurvePoint: { piece: SL, segment: 'hemL', t: 0.6, at: [d.pieces[SL].pts.find((p) => p.id === 'hemL').x + 50, 300] } });
    step({ MirrorPiece: { piece: F, linked: true } });
    step('MarkHems');
    step({ FlipPieces: { pieces: [Bk], axis: 'Vertical' } });
    step({ DeletePoints: { piece: F, points: [d.pieces[F].pts.find((p) => p.id === 'hemR').id] } });
    step({ DeletePieces: { pieces: [RIB] } });
  });
}

// A project with every kind of stored object and fields this version does not interpret.
function richProject() {
  const d = newDoc('tee', 'male');
  const [FB, BK, FA] = ordered(d);
  d.meta.owner = 'u1';
  d.avatar.kind = 'makehuman';
  d.xray = { opacity: 0.4, layers: [1, 2] };
  d.sim = { preset: 'fine', over: { substeps: 12, gravity: 0.9 }, quality: 'normal', selfCollision: true, air: 0.4 };
  d.techpack = { season: 'All season', ...d.techpack, designer: 'A. Designer', currency: 'EUR' };
  d.fabrics.fab_main.price = 32.5; d.fabrics.fab_main.width = 1500; d.fabrics.fab_main.texture = { path: 'tex/wool.png', w: 100 };
  d.fabrics.fab_main.phys.custom = 3;
  d.colorways.cw_navy = { id: 'cw_navy', name: 'Navy', order: 1, over: { fab_main: { color: '#2b3242' } }, thumb: 'navy.png' };
  const sw = Object.values(d.sewing)[0]; sw.kind = 'fold'; sw.a[0].note = 'side extra';
  Object.values(d.poms)[0].tol = 5;
  Object.values(d.stitches)[0].hidden = true;
  const pc = d.pieces[FB];
  pc.qty = 2; pc.elastic = { hemL: 0.8 }; pc.hide3d = true; pc.shrinkWeft = 2; pc.arr.flip = true;
  pc.pins = [{ x: 400, y: 500 }, { x: 410, y: 900, tag: 'p' }];
  pc.graphics = [{ id: 'g_900', path: 'logo.png', x: 380, y: 300, w: 60, h: 30, ang: 15 }];
  pc.internals = [{ id: 'il_901', pts: [{ x: 0, y: 200 }, { x: 10, y: 400, c: 1 }, { x: 0, y: 600 }], closed: false, fold: 180, style: 'dash' }];
  pc.notches.push({ id: 'n_902', s: 'hemL', t: 0.25, kind: 'v' });
  d.buttons.bt_903 = { id: 'bt_903', p: FB, x: 40, y: 320, size: 15, color: '#2b2b2b', shape: 'round', holes: 4, finish: 'plastic', fasten: 'bh_904' };
  d.holes.bh_904 = { id: 'bh_904', p: FA, x: 200, y: 320, len: 18, ang: 0, color: '#2a2a2a' };
  d.comments.c_905 = { id: 'c_905', n: 1, p: BK, x: 1300, y: 500, text: 'Check', by: 'u1', byName: 'Ann', at: 1, resolved: false, replies: [{ text: 'ok', by: 'u2', at: 2 }] };
  d.comments.c_906 = { id: 'c_906', n: 2, text: 'Global note', resolved: true };
  d.graphics.logo = { path: 'logo.png' };
  return d;
}

function richEdits() {
  return sequence('rich', richProject(), (d, step) => {
    const [FB, BK] = ordered(d);
    step({ TransformPieces: { pieces: [FB, BK], transform: { Rotate: { center: [500, 500], angle: -0.3 } } } });
    // Flip before copying: a src.js copy shares its grade objects with the source, so flipping the
    // source afterwards would also flip the copy's grading there.
    step({ FlipPieces: { pieces: [FB], axis: 'Horizontal' } });
    step({ DuplicatePiece: { piece: FB } });
    step({ MirrorPiece: { piece: FB, linked: false } });
    step({ DeletePieces: { pieces: [BK] } });
  });
}

const golden = {
  generator: 'tools/golden/export-doc.mjs',
  fabrics: fabricCases(),
  curves: curveCases(),
  normalize: normalizeCases(),
  templates: templateCases(),
  edits: [fullEdits(), teeEdits(), richEdits()],
};
process.stdout.write(`${JSON.stringify(golden)}\n`);
