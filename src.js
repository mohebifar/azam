
/* ============================================================
   ATELIER ENGINE — pattern geometry, meshing, avatar, cloth solver
   Units: millimetres, grams, seconds. 2D: x right, y down.
   3D: y up, avatar faces +z, feet on y = 0.
   ============================================================ */
const AT = {};
AT.uid = function (p) { return (p || '') + Math.random().toString(36).slice(2, 8) + (Date.now() % 1e6).toString(36); };
AT.clamp = function (v, a, b) { return v < a ? a : v > b ? b : v; };
AT.lerp = function (a, b, t) { return a + (b - a) * t; };
AT.deg = Math.PI / 180;
AT.clone = function (o) { return o == null ? o : JSON.parse(JSON.stringify(o)); };

/* ---------- curves & outlines ---------- */
// A pattern outline is a closed list of points {id,x,y,c}. c=1 marks a curve point:
// curve points are Bezier control points between the surrounding corner points (CLO semantics).
AT.segInfo = function (pts) {
  const n = pts.length, corners = [];
  for (let i = 0; i < n; i++) if (!pts[i].c) corners.push(i);
  if (!corners.length && n) corners.push(0);
  const segs = [];
  for (let k = 0; k < corners.length; k++) {
    const i0 = corners[k], i1 = corners[(k + 1) % corners.length];
    const ctrl = [pts[i0]];
    let j = (i0 + 1) % n, guard = 0;
    while (j !== i1 && guard++ < n) { ctrl.push(pts[j]); j = (j + 1) % n; }
    ctrl.push(pts[i1]);
    segs.push({ k: k, i0: i0, i1: i1, id: pts[i0].id, ctrl: ctrl });
  }
  return segs;
};
AT.bez = function (ctrl, t) {
  const m = ctrl.length, xs = new Array(m), ys = new Array(m);
  for (let i = 0; i < m; i++) { xs[i] = ctrl[i].x; ys[i] = ctrl[i].y; }
  for (let r = 1; r < m; r++) for (let i = 0; i < m - r; i++) { xs[i] += (xs[i + 1] - xs[i]) * t; ys[i] += (ys[i + 1] - ys[i]) * t; }
  return { x: xs[0], y: ys[0] };
};
AT.flattenCtrl = function (c) {
  if (c.length === 2) return [{ x: c[0].x, y: c[0].y }, { x: c[1].x, y: c[1].y }];
  let L = 0;
  for (let i = 1; i < c.length; i++) L += Math.hypot(c[i].x - c[i - 1].x, c[i].y - c[i - 1].y);
  const n = Math.max(10, Math.min(240, Math.ceil(L / 3)));
  const out = [];
  for (let i = 0; i <= n; i++) out.push(AT.bez(c, i / n));
  return out;
};
AT.polyCum = function (poly) {
  const cum = [0];
  for (let i = 1; i < poly.length; i++) cum.push(cum[i - 1] + Math.hypot(poly[i].x - poly[i - 1].x, poly[i].y - poly[i - 1].y));
  return cum;
};
AT.outline = function (pts) {
  const segs = AT.segInfo(pts), res = { segs: [], total: 0, start: [], byId: {} };
  segs.forEach(function (s, k) {
    const poly = AT.flattenCtrl(s.ctrl), cum = AT.polyCum(poly);
    const sg = { k: k, i0: s.i0, i1: s.i1, id: s.id, ctrl: s.ctrl, poly: poly, cum: cum, len: cum[cum.length - 1] };
    res.start.push(res.total);
    res.total += sg.len;
    res.segs.push(sg);
    res.byId[s.id] = k;
  });
  res.poly = [];
  res.segs.forEach(function (sg) { for (let i = 0; i < sg.poly.length - 1; i++) res.poly.push(sg.poly[i]); });
  return res;
};
AT.polyPointAt = function (poly, cum, L) {
  let lo = 0, hi = cum.length - 1;
  if (L <= 0) lo = 0, hi = 1;
  else if (L >= cum[hi]) lo = hi - 1;
  else { while (hi - lo > 1) { const m = (lo + hi) >> 1; if (cum[m] <= L) lo = m; else hi = m; } }
  const d = cum[hi] - cum[lo], u = d > 1e-9 ? AT.clamp((L - cum[lo]) / d, 0, 1) : 0;
  const a = poly[lo], b = poly[hi];
  return { x: a.x + (b.x - a.x) * u, y: a.y + (b.y - a.y) * u, tx: (b.x - a.x) / (d || 1), ty: (b.y - a.y) / (d || 1) };
};
AT.segPoint = function (sg, t) { return AT.polyPointAt(sg.poly, sg.cum, sg.len * t); };
AT.perimParam = function (ol, k, t) { return ol.start[k] + ol.segs[k].len * t; };
AT.perimPoint = function (ol, P) {
  const T = ol.total;
  P = ((P % T) + T) % T;
  let k = ol.segs.length - 1;
  for (let i = 0; i < ol.segs.length; i++) { if (P < ol.start[i] + ol.segs[i].len) { k = i; break; } }
  const sg = ol.segs[k], t = sg.len > 0 ? (P - ol.start[k]) / sg.len : 0;
  const p = AT.segPoint(sg, AT.clamp(t, 0, 1));
  p.k = k; p.t = t;
  return p;
};
// Sewing side: {p, a:{s,t}, b:{s,t}, dir}. s = id of the segment's start corner point.
AT.sideRange = function (ol, side) {
  const ka = ol.byId[side.a.s], kb = ol.byId[side.b.s];
  if (ka == null || kb == null) return null;
  const T = ol.total, Pa = AT.perimParam(ol, ka, side.a.t), Pb = AT.perimParam(ol, kb, side.b.t);
  let L = side.dir > 0 ? Pb - Pa : Pa - Pb;
  L = ((L % T) + T) % T;
  if (L < 1e-6) L = T;
  return { Pa: Pa, Pb: Pb, L: L, dir: side.dir > 0 ? 1 : -1, T: T };
};
AT.sidePolyline = function (ol, side, step) {
  const r = AT.sideRange(ol, side);
  if (!r) return [];
  const n = Math.max(2, Math.ceil(r.L / (step || 5)));
  const out = [];
  for (let i = 0; i <= n; i++) out.push(AT.perimPoint(ol, r.Pa + r.dir * r.L * i / n));
  return out;
};
AT.segSide = function (sid, reverse) {
  return reverse ? { a: { s: sid, t: 1 }, b: { s: sid, t: 0 }, dir: -1 } : { a: { s: sid, t: 0 }, b: { s: sid, t: 1 }, dir: 1 };
};

/* ---------- polygon utilities ---------- */
AT.area = function (poly) { let a = 0; for (let i = 0, n = poly.length; i < n; i++) { const p = poly[i], q = poly[(i + 1) % n]; a += p.x * q.y - q.x * p.y; } return a / 2; };
AT.inside = function (poly, x, y) {
  let c = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const a = poly[i], b = poly[j];
    if ((a.y > y) !== (b.y > y) && x < (b.x - a.x) * (y - a.y) / (b.y - a.y) + a.x) c = !c;
  }
  return c;
};
AT.distSeg = function (px, py, ax, ay, bx, by) {
  const dx = bx - ax, dy = by - ay, L2 = dx * dx + dy * dy;
  let t = L2 > 0 ? ((px - ax) * dx + (py - ay) * dy) / L2 : 0;
  t = AT.clamp(t, 0, 1);
  const x = ax + dx * t, y = ay + dy * t;
  return { d: Math.hypot(px - x, py - y), t: t, x: x, y: y };
};
AT.bbox = function (pts) {
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const p of pts) { if (p.x < x0) x0 = p.x; if (p.y < y0) y0 = p.y; if (p.x > x1) x1 = p.x; if (p.y > y1) y1 = p.y; }
  return { x0: x0, y0: y0, x1: x1, y1: y1, w: x1 - x0, h: y1 - y0, cx: (x0 + x1) / 2, cy: (y0 + y1) / 2 };
};
// nearest location on an outline -> {k, t, x, y, d}
AT.nearestOnOutline = function (ol, x, y) {
  let best = { d: Infinity };
  ol.segs.forEach(function (sg, k) {
    for (let i = 0; i < sg.poly.length - 1; i++) {
      const a = sg.poly[i], b = sg.poly[i + 1], r = AT.distSeg(x, y, a.x, a.y, b.x, b.y);
      if (r.d < best.d) {
        const L = sg.cum[i] + (sg.cum[i + 1] - sg.cum[i]) * r.t;
        best = { d: r.d, k: k, t: sg.len > 0 ? L / sg.len : 0, x: r.x, y: r.y, id: sg.id };
      }
    }
  });
  return best;
};
// Seam allowance: offset the net outline outward. widths per segment id (mm).
AT.offsetOutline = function (ol, widthFor) {
  const ccw = AT.area(ol.poly) > 0; // y-down screen coords: positive area = clockwise visually
  const out = [];
  ol.segs.forEach(function (sg) {
    const w = widthFor(sg.id);
    const P = sg.poly;
    for (let i = 0; i < P.length - 1; i++) {
      const a = P[i], b = P[i + 1];
      let dx = b.x - a.x, dy = b.y - a.y; const L = Math.hypot(dx, dy) || 1; dx /= L; dy /= L;
      // outward normal
      const nx = ccw ? dy : -dy, ny = ccw ? -dx : dx;
      out.push({ ax: a.x + nx * w, ay: a.y + ny * w, bx: b.x + nx * w, by: b.y + ny * w, w: w });
    }
  });
  // join consecutive offset edges by intersection (miter, limited)
  const res = [];
  for (let i = 0; i < out.length; i++) {
    const e1 = out[(i - 1 + out.length) % out.length], e2 = out[i];
    const d1x = e1.bx - e1.ax, d1y = e1.by - e1.ay, d2x = e2.bx - e2.ax, d2y = e2.by - e2.ay;
    const den = d1x * d2y - d1y * d2x;
    if (Math.abs(den) < 1e-9) { res.push({ x: e2.ax, y: e2.ay }); continue; }
    const t = ((e2.ax - e1.ax) * d2y - (e2.ay - e1.ay) * d2x) / den;
    const ix = e1.ax + d1x * t, iy = e1.ay + d1y * t;
    const lim = Math.max(e1.w, e2.w) * 3 + 1;
    if (Math.hypot(ix - e2.ax, iy - e2.ay) > lim) { res.push({ x: e1.bx, y: e1.by }); res.push({ x: e2.ax, y: e2.ay }); }
    else res.push({ x: ix, y: iy });
  }
  return res;
};

// allowance outside the sewing line for one segment: hem allowance, else per-segment SA, else the piece SA
AT.edgeAllowance = function (pc, sid) {
  if (pc.hem && pc.hem[sid] > 0) return pc.hem[sid];
  return (pc.saSeg && pc.saSeg[sid] != null) ? pc.saSeg[sid] : (pc.sa || 0);
};
// cut line of a piece: offset by each segment's allowance, mitred; where a hem meets another edge the corner is
// folded (below the hem fold the side cut edge is the mirror image of the side cut edge above it), so the hem
// allowance matches the garment when it is turned up
AT.cutOutline = function (pc, ol) {
  const ccw = AT.area(ol.poly) > 0, hem = pc.hem || {}, out = [];
  ol.segs.forEach(function (sg) {
    const w = AT.edgeAllowance(pc, sg.id), isH = hem[sg.id] > 0, P = sg.poly;
    for (let i = 0; i < P.length - 1; i++) {
      const a = P[i], b = P[i + 1];
      let dx = b.x - a.x, dy = b.y - a.y; const L = Math.hypot(dx, dy) || 1; dx /= L; dy /= L;
      const nx = ccw ? dy : -dy, ny = ccw ? -dx : dx;
      out.push({ ax: a.x + nx * w, ay: a.y + ny * w, bx: b.x + nx * w, by: b.y + ny * w, w: w, sid: sg.id, hem: isH, na: a, dx: dx, dy: dy });
    }
  });
  const X = function (p, d, q, e) { const den = d.x * e.y - d.y * e.x; if (Math.abs(den) < 1e-9) return null; const t = ((q.x - p.x) * e.y - (q.y - p.y) * e.x) / den; return { x: p.x + d.x * t, y: p.y + d.y * t }; };
  const res = [];
  for (let i = 0; i < out.length; i++) {
    const e1 = out[(i - 1 + out.length) % out.length], e2 = out[i];
    if (e1.sid !== e2.sid && e1.hem !== e2.hem) {
      const H = e2.hem ? e2 : e1, S = e2.hem ? e1 : e2, C = e2.na, fu = { x: H.dx, y: H.dy };
      const sD = { x: S.bx - S.ax, y: S.by - S.ay }, F = X({ x: S.ax, y: S.ay }, sD, C, fu);
      if (F) {
        const k = 2 * (sD.x * fu.x + sD.y * fu.y), mD = { x: k * fu.x - sD.x, y: k * fu.y - sD.y };
        const G = X(F, mD, { x: H.ax, y: H.ay }, { x: H.bx - H.ax, y: H.by - H.ay }), lim = Math.max(H.w, S.w) * 4 + 5;
        if (G && Math.hypot(F.x - C.x, F.y - C.y) < lim && Math.hypot(G.x - C.x, G.y - C.y) < lim) {
          if (e2.hem) { res.push(F); res.push(G); } else { res.push(G); res.push(F); }
          continue;
        }
      }
    }
    const d1x = e1.bx - e1.ax, d1y = e1.by - e1.ay, d2x = e2.bx - e2.ax, d2y = e2.by - e2.ay;
    const den = d1x * d2y - d1y * d2x;
    if (Math.abs(den) < 1e-9) { res.push({ x: e2.ax, y: e2.ay }); continue; }
    const t = ((e2.ax - e1.ax) * d2y - (e2.ay - e1.ay) * d2x) / den;
    const ix = e1.ax + d1x * t, iy = e1.ay + d1y * t, lim = Math.max(e1.w, e2.w) * 3 + 1;
    if (Math.hypot(ix - e2.ax, iy - e2.ay) > lim) { res.push({ x: e1.bx, y: e1.by }); res.push({ x: e2.ax, y: e2.ay }); }
    else res.push({ x: ix, y: iy });
  }
  return res;
};
// part of an outline between two perimeter parameters (P1 > P0, may wrap past the total)
AT.olPath = function (ol, P0, P1) {
  if (!ol._vp) { const vp = []; ol.segs.forEach(function (sg, k) { for (let i = 0; i < sg.poly.length - 1; i++) vp.push({ P: ol.start[k] + sg.cum[i], x: sg.poly[i].x, y: sg.poly[i].y }); }); ol._vp = vp; }
  const T = ol.total, out = [AT.perimPoint(ol, P0)];
  for (let base = Math.floor(P0 / T) * T; base < P1; base += T) ol._vp.forEach(function (v) { const P = v.P + base; if (P > P0 + 1e-6 && P < P1 - 1e-6) out.push({ x: v.x, y: v.y }); });
  out.push(AT.perimPoint(ol, P1));
  return out;
};
AT.evalOpen = function (pts) { // internal line: open polyline with curve points
  if (!pts || pts.length < 2) return [];
  const out = [];
  let i0 = 0;
  for (let i = 1; i < pts.length; i++) {
    if (!pts[i].c || i === pts.length - 1) {
      const fl = AT.flattenCtrl(pts.slice(i0, i + 1));
      for (let j = out.length ? 1 : 0; j < fl.length; j++) out.push(fl[j]);
      i0 = i;
    }
  }
  return out;
};

/* ---------- grading ---------- */
AT.gradedPts = function (piece, size, base) {
  if (!size || size === base || !piece.grade) return piece.pts;
  return piece.pts.map(function (p) {
    const r = piece.grade[p.id] && piece.grade[p.id][size];
    return r ? { id: p.id, x: p.x + (r.dx || 0), y: p.y + (r.dy || 0), c: p.c } : p;
  });
};

/* ---------- meshing ---------- */
AT.meshPiece = function (pts, internals, pd, grainDeg) {
  pd = AT.clamp(pd || 20, 3, 80);
  const ol = AT.outline(pts);
  const dense = ol.poly;
  const B = [];
  ol.segs.forEach(function (sg, k) {
    const n = Math.max(1, Math.round(sg.len / pd));
    for (let i = 0; i < n; i++) { const p = AT.segPoint(sg, i / n); B.push({ x: p.x, y: p.y, k: k, t: i / n }); }
  });
  const nB = B.length;
  // spatial hash of boundary samples
  const cell = pd, hash = new Map();
  const hk = function (x, y) { return Math.floor(x / cell) + ',' + Math.floor(y / cell); };
  const addH = function (p, idx) { const k = hk(p.x, p.y); let a = hash.get(k); if (!a) { a = []; hash.set(k, a); } a.push(idx); };
  const pts2 = B.map(function (b) { return { x: b.x, y: b.y }; });
  B.forEach(function (b, i) { addH(b, i); });
  const nearDist = function (x, y, r) {
    const cx = Math.floor(x / cell), cy = Math.floor(y / cell), R = Math.ceil(r / cell);
    let best = Infinity;
    for (let i = -R; i <= R; i++) for (let j = -R; j <= R; j++) {
      const a = hash.get((cx + i) + ',' + (cy + j)); if (!a) continue;
      for (const idx of a) { const q = pts2[idx]; const d = Math.hypot(q.x - x, q.y - y); if (d < best) best = d; }
    }
    return best;
  };
  // internal lines
  const intSamples = [];
  (internals || []).forEach(function (il) {
    const pl = AT.evalOpen(il.pts);
    if (pl.length < 2) return;
    const cum = AT.polyCum(pl), L = cum[cum.length - 1];
    const n = Math.max(1, Math.round(L / pd));
    const verts = [];
    for (let i = 0; i <= n; i++) {
      const p = AT.polyPointAt(pl, cum, L * i / n);
      if (!AT.inside(dense, p.x, p.y)) continue;
      if (nearDist(p.x, p.y, pd * 0.35) < pd * 0.35) continue;
      verts.push(pts2.length);
      pts2.push({ x: p.x, y: p.y });
      addH(p, pts2.length - 1);
    }
    intSamples.push({ id: il.id, verts: verts, fold: il.fold });
  });
  // interior lattice aligned with grain
  const g = (grainDeg || 0) * AT.deg, cg = Math.cos(g), sg = Math.sin(g);
  const bb = AT.bbox(dense), cx0 = bb.cx, cy0 = bb.cy, R = Math.hypot(bb.w, bb.h) / 2 + pd;
  const hstep = pd * Math.sqrt(3) / 2;
  let row = 0;
  for (let v = -R; v <= R; v += hstep, row++) {
    for (let u = -R + (row % 2 ? pd / 2 : 0); u <= R; u += pd) {
      const x = cx0 + u * cg - v * sg, y = cy0 + u * sg + v * cg;
      if (x < bb.x0 || x > bb.x1 || y < bb.y0 || y > bb.y1) continue;
      if (!AT.inside(dense, x, y)) continue;
      if (nearDist(x, y, pd * 0.62) < pd * 0.62) continue;
      pts2.push({ x: x, y: y });
    }
  }
  const coords = new Float64Array(pts2.length * 2);
  pts2.forEach(function (p, i) { coords[2 * i] = p.x; coords[2 * i + 1] = p.y; });
  const del = new Delaunator(coords);
  const T = del.triangles, tris = [];
  const isB = function (i) { return i < nB; };
  const adjB = function (i, j) { if (!isB(i) || !isB(j)) return false; const d = Math.abs(i - j); return d === 1 || d === nB - 1; };
  for (let t = 0; t < T.length; t += 3) {
    const a = T[t], b = T[t + 1], c = T[t + 2];
    const ax = coords[2 * a], ay = coords[2 * a + 1], bx = coords[2 * b], by = coords[2 * b + 1], cx = coords[2 * c], cy = coords[2 * c + 1];
    const ar = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax);
    if (Math.abs(ar) < pd * pd * 0.004) continue;
    if (!AT.inside(dense, (ax + bx + cx) / 3, (ay + by + cy) / 3)) continue;
    let ok = true;
    const E = [[a, b], [b, c], [c, a]];
    for (const e of E) {
      if (adjB(e[0], e[1])) continue;
      const mx = (coords[2 * e[0]] + coords[2 * e[1]]) / 2, my = (coords[2 * e[0] + 1] + coords[2 * e[1] + 1]) / 2;
      if (!AT.inside(dense, mx, my)) { ok = false; break; }
    }
    if (!ok) continue;
    if (ar > 0) tris.push(a, b, c); else tris.push(a, c, b);
  }
  const bnd = new Int32Array(nB), bk = new Int32Array(nB), bt = new Float32Array(nB);
  for (let i = 0; i < nB; i++) { bnd[i] = i; bk[i] = B[i].k; bt[i] = B[i].t; }
  return { n: pts2.length, xy: coords, tris: new Uint32Array(tris), nB: nB, bnd: bnd, bk: bk, bt: bt, internals: intSamples, ol: ol, pd: pd };
};
// topology: unique edges with opposite vertices
AT.topology = function (mesh) {
  const n = mesh.n, T = mesh.tris, map = new Map(), edges = [];
  for (let t = 0; t < T.length; t += 3) {
    for (let e = 0; e < 3; e++) {
      const i = T[t + e], j = T[t + (e + 1) % 3], o = T[t + (e + 2) % 3];
      const a = Math.min(i, j), b = Math.max(i, j), key = a * n + b;
      let E = map.get(key);
      if (!E) { E = { a: a, b: b, o1: o, o2: -1 }; map.set(key, E); edges.push(E); } else E.o2 = o;
    }
  }
  return { edges: edges, map: map };
};
// barycentric locate of 2D point in mesh -> {tri, u, v, w} (nearest triangle if outside)
AT.locate = function (mesh, x, y) {
  const T = mesh.tris, P = mesh.xy;
  let best = null, bestD = Infinity;
  for (let t = 0; t < T.length; t += 3) {
    const a = T[t], b = T[t + 1], c = T[t + 2];
    const ax = P[2 * a], ay = P[2 * a + 1], bx = P[2 * b], by = P[2 * b + 1], cx = P[2 * c], cy = P[2 * c + 1];
    const d = (by - cy) * (ax - cx) + (cx - bx) * (ay - cy);
    if (Math.abs(d) < 1e-12) continue;
    const l1 = ((by - cy) * (x - cx) + (cx - bx) * (y - cy)) / d;
    const l2 = ((cy - ay) * (x - cx) + (ax - cx) * (y - cy)) / d;
    const l3 = 1 - l1 - l2;
    const out = Math.max(0, -l1) + Math.max(0, -l2) + Math.max(0, -l3);
    if (out < bestD) { bestD = out; best = { tri: t / 3, a: a, b: b, c: c, l1: l1, l2: l2, l3: l3 }; if (out === 0) break; }
  }
  return best;
};


/* ---------- parametric avatar ---------- */
AT.AVATAR_PRESETS = {
  female: { sex: 'female', height: 1700, bust: 880, underbust: 760, waist: 700, hip: 960, shoulder: 365, arm: 560, crotch: 780, neck: 330, thigh: 540, upperArm: 270, armAngle: 45, legSpread: 6 },
  male: { sex: 'male', height: 1800, bust: 980, underbust: 920, waist: 840, hip: 980, shoulder: 410, arm: 610, crotch: 840, neck: 390, thigh: 560, upperArm: 310, armAngle: 45, legSpread: 6 }
};
AT.SIZE_CHARTS = {
  female: [
    { name: 'XS', height: 1640, bust: 800, underbust: 690, waist: 620, hip: 880, neck: 310, shoulder: 350, arm: 545, crotch: 755, thigh: 500, upperArm: 245 },
    { name: 'S', height: 1670, bust: 840, underbust: 725, waist: 660, hip: 920, neck: 320, shoulder: 357, arm: 552, crotch: 768, thigh: 520, upperArm: 257 },
    { name: 'M', height: 1700, bust: 880, underbust: 760, waist: 700, hip: 960, neck: 330, shoulder: 365, arm: 560, crotch: 780, thigh: 540, upperArm: 270 },
    { name: 'L', height: 1720, bust: 940, underbust: 810, waist: 760, hip: 1020, neck: 342, shoulder: 375, arm: 566, crotch: 788, thigh: 575, upperArm: 290 },
    { name: 'XL', height: 1740, bust: 1000, underbust: 860, waist: 820, hip: 1080, neck: 355, shoulder: 385, arm: 572, crotch: 796, thigh: 610, upperArm: 310 }
  ],
  male: [
    { name: 'S', height: 1740, bust: 900, underbust: 850, waist: 760, hip: 920, neck: 370, shoulder: 395, arm: 590, crotch: 815, thigh: 520, upperArm: 285 },
    { name: 'M', height: 1800, bust: 980, underbust: 920, waist: 840, hip: 980, neck: 390, shoulder: 410, arm: 610, crotch: 840, thigh: 560, upperArm: 310 },
    { name: 'L', height: 1840, bust: 1060, underbust: 1000, waist: 920, hip: 1040, neck: 410, shoulder: 425, arm: 622, crotch: 855, thigh: 595, upperArm: 335 },
    { name: 'XL', height: 1870, bust: 1140, underbust: 1080, waist: 1000, hip: 1100, neck: 430, shoulder: 440, arm: 632, crotch: 865, thigh: 630, upperArm: 360 }
  ]
};
AT.ellAxes = function (C, k) { const p = Math.PI * (3 * (1 + k) - Math.sqrt((3 + k) * (1 + 3 * k))); const a = C / p; return [a, a * k]; };
AT.ellPerim = function (a, b) { return Math.PI * (3 * (a + b) - Math.sqrt((3 * a + b) * (a + 3 * b))); };
// monotone cubic (PCHIP) interpolation on knots
AT.pchip = function (xs, ys) {
  const n = xs.length, h = [], d = [], m = new Array(n).fill(0);
  for (let i = 0; i < n - 1; i++) { h.push(xs[i + 1] - xs[i]); d.push((ys[i + 1] - ys[i]) / h[i]); }
  m[0] = d[0]; m[n - 1] = d[n - 2];
  for (let i = 1; i < n - 1; i++) {
    if (d[i - 1] * d[i] <= 0) m[i] = 0;
    else { const w1 = 2 * h[i] + h[i - 1], w2 = h[i] + 2 * h[i - 1]; m[i] = (w1 + w2) / (w1 / d[i - 1] + w2 / d[i]); }
  }
  return function (x) {
    if (x <= xs[0]) return ys[0];
    if (x >= xs[n - 1]) return ys[n - 1];
    let i = 0; while (i < n - 2 && x > xs[i + 1]) i++;
    const t = (x - xs[i]) / h[i], t2 = t * t, t3 = t2 * t;
    return (2 * t3 - 3 * t2 + 1) * ys[i] + (t3 - 2 * t2 + t) * h[i] * m[i] + (-2 * t3 + 3 * t2) * ys[i + 1] + (t3 - t2) * h[i] * m[i + 1];
  };
};
AT.sdRoundCone = function (px, py, pz, P) {
  const bax = P.bx - P.ax, bay = P.by - P.ay, baz = P.bz - P.az;
  const l2 = bax * bax + bay * bay + baz * baz, rr = P.r1 - P.r2, a2 = l2 - rr * rr, il2 = 1 / l2;
  const pax = px - P.ax, pay = py - P.ay, paz = pz - P.az;
  const y = pax * bax + pay * bay + paz * baz, z = y - l2;
  const qx = pax * l2 - bax * y, qy = pay * l2 - bay * y, qz = paz * l2 - baz * y;
  const x2 = qx * qx + qy * qy + qz * qz, y2 = y * y * l2, z2 = z * z * l2;
  const k = Math.sign(rr) * rr * rr * x2;
  if (Math.sign(z) * a2 * z2 > k) return Math.sqrt(x2 + z2) * il2 - P.r2;
  if (Math.sign(y) * a2 * y2 < k) return Math.sqrt(x2 + y2) * il2 - P.r1;
  return (Math.sqrt(x2 * a2 * il2) + y * rr) * il2 - P.r1;
};
AT.sdEllipsoid = function (px, py, pz, P) {
  const x = px - P.cx, y = py - P.cy, z = pz - P.cz;
  const k0 = Math.sqrt((x / P.rx) * (x / P.rx) + (y / P.ry) * (y / P.ry) + (z / P.rz) * (z / P.rz));
  const k1 = Math.sqrt((x / (P.rx * P.rx)) * (x / (P.rx * P.rx)) + (y / (P.ry * P.ry)) * (y / (P.ry * P.ry)) + (z / (P.rz * P.rz)) * (z / (P.rz * P.rz)));
  return k1 > 1e-9 ? k0 * (k0 - 1) / k1 : -Math.min(P.rx, P.ry, P.rz);
};
AT.smin = function (a, b, k) { const h = Math.max(k - Math.abs(a - b), 0) / k; return Math.min(a, b) - h * h * k * 0.25; };

AT.torsoProfile = function (m0) {
  const base = AT.AVATAR_PRESETS[m0 && m0.sex === 'male' ? 'male' : 'female'];
  const m = Object.assign({}, base, m0 || {});
  const H = m.height, fem = m.sex !== 'male', s = H / 1700;
  const yS = H * 0.818, yNeck = H * 0.848, yChin = H * 0.872, yBust = H * 0.722, yUnder = H * 0.683, yWaist = H * 0.625;
  const yHigh = H * 0.578, yHip = H * 0.527, yCr = Math.min(m.crotch, H * 0.5);
  const hip = AT.ellAxes(m.hip, 0.76), waist = AT.ellAxes(m.waist, 0.74), bust = AT.ellAxes(m.bust, fem ? 0.70 : 0.66);
  const under = AT.ellAxes(m.underbust || m.bust - 110, 0.72), high = AT.ellAxes((m.hip + m.waist) / 2, 0.74);
  const neckR = m.neck / (2 * Math.PI), shH = m.shoulder / 2;
  const sl = [
    [yCr - 52 * s, hip[0] * 0.18, hip[1] * 0.2, -6],
    [yCr - 30 * s, hip[0] * 0.74, hip[1] * 0.62, -6],
    [yCr, hip[0] * 0.93, hip[1] * 0.86, -10],
    [yHip, hip[0], hip[1], fem ? -20 : -14],
    [yHigh, high[0], high[1], -10],
    [yWaist, waist[0], waist[1], -2],
    [yUnder, under[0], under[1], 2],
    [yBust, bust[0], bust[1], fem ? 14 : 6],
    [H * 0.768, bust[0] * 0.96, bust[1] * 0.92, 4],
    [yS - 32 * s, shH * 0.86, bust[1] * 0.78, -2],
    [yS, shH * 0.80, 74 * s, -10],
    [yS + 16 * s, neckR * 2.25, 58 * s, -14],
    [yNeck, neckR * 1.15, neckR * 1.05, -14]
  ];
  const ys = sl.map(function (r) { return r[0]; });
  const fa = AT.pchip(ys, sl.map(function (r) { return r[1]; }));
  const fb = AT.pchip(ys, sl.map(function (r) { return r[2]; }));
  const fc = AT.pchip(ys, sl.map(function (r) { return r[3]; }));
  const ty0 = ys[0], ty1 = ys[ys.length - 1], tStep = 2, tN = Math.ceil((ty1 - ty0) / tStep) + 1;
  const TA = new Float32Array(tN), TB = new Float32Array(tN), TC = new Float32Array(tN);
  for (let i = 0; i < tN; i++) { const y = ty0 + i * tStep; TA[i] = fa(y); TB[i] = fb(y); TC[i] = fc(y); }
  const tEval = function (y) {
    const f = AT.clamp((y - ty0) / tStep, 0, tN - 1.0001), i = Math.floor(f), u = f - i;
    return [TA[i] + (TA[i + 1] - TA[i]) * u, TB[i] + (TB[i + 1] - TB[i]) * u, TC[i] + (TC[i + 1] - TC[i]) * u];
  };
  // height of the shoulder surface at horizontal distance x from centre (searching down from the neck)
  const shoulderY = function (x) { for (let y = ty1; y > yS - 60; y -= 2) { if (tEval(y)[0] >= x) return y; } return yS; };
  return { m: m, H: H, s: s, fem: fem, torso: { y0: ty0, y1: ty1, step: tStep, n: tN, A: TA, B: TB, C: TC, eval: tEval }, shoulderY: shoulderY,
    hip: hip, neckR: neckR, shH: shH,
    ys: { shoulder: yS, neck: yNeck, chin: yChin, bust: yBust, under: yUnder, waist: yWaist, high: yHigh, hip: yHip, crotch: yCr, knee: H * 0.285 } };
};
AT.buildAvatar = function (m0) {
  const TP = AT.torsoProfile(m0);
  const m = TP.m, H = TP.H, s = TP.s, hip = TP.hip, neckR = TP.neckR, shH = TP.shH;
  const yS = TP.ys.shoulder, yNeck = TP.ys.neck, yChin = TP.ys.chin, yCr = TP.ys.crotch;
  const torso = TP.torso, tEval = torso.eval, ty0 = torso.y0, ty1 = torso.y1;
  const sdTorso = function (x, y, z) {
    if (y < ty0 - 40 || y > ty1 + 40) return 1e3;
    const yc = AT.clamp(y, ty0, ty1);
    const e = tEval(yc), X = x, Z = z - e[2];
    const sx = X / e[0], sz = Z / e[1], sN = Math.sqrt(sx * sx + sz * sz), r = Math.sqrt(X * X + Z * Z);
    let d = sN > 1e-6 ? r * (1 - 1 / sN) : -Math.min(e[0], e[1]);
    if (y !== yc) d = Math.max(d, Math.abs(y - yc)) + (d < 0 ? Math.abs(y - yc) : 0);
    return d;
  };
  // limbs
  const beta = (m.armAngle == null ? 45 : m.armAngle) * AT.deg, uaR = m.upperArm / (2 * Math.PI);
  const prims = [];
  const sides = [1, -1];
  const joints = {};
  sides.forEach(function (sd) {
    const S = [sd * (shH - uaR * 0.55), yS - uaR * 0.62, -6];
    const a = [sd * Math.sin(beta), -Math.cos(beta), 0];
    const Lu = m.arm * 0.54, Lf = m.arm * 0.46;
    const E = [S[0] + a[0] * Lu, S[1] + a[1] * Lu, S[2] + 6];
    const W = [E[0] + a[0] * Lf, E[1] + a[1] * Lf, E[2] + 4];
    prims.push({ t: 'rc', ax: S[0], ay: S[1], az: S[2], bx: E[0], by: E[1], bz: E[2], r1: uaR, r2: uaR * 0.74, k: 16 * s });
    prims.push({ t: 'rc', ax: E[0], ay: E[1], az: E[2], bx: W[0], by: W[1], bz: W[2], r1: uaR * 0.74, r2: uaR * 0.52, k: 12 * s });
    const hl = 92 * s;
    prims.push({ t: 'el', cx: W[0] + a[0] * hl * 0.55, cy: W[1] + a[1] * hl * 0.55, cz: W[2] + 6, rx: sd ? 26 * s + Math.abs(a[0]) * 22 * s : 30, ry: 38 * s + Math.abs(a[1]) * 20 * s, rz: 44 * s, k: 14 * s });
    const thR = m.thigh / (2 * Math.PI), gam = (m.legSpread || 0) * AT.deg;
    const Hj = [sd * hip[0] * 0.47, yCr + 44 * s, -8];
    const la = [sd * Math.sin(gam), -Math.cos(gam), 0];
    const kneeY = H * 0.285, ankY = H * 0.048;
    const Lt = (Hj[1] - kneeY) / Math.cos(gam), Ls = (kneeY - ankY) / Math.cos(gam);
    const K = [Hj[0] + la[0] * Lt, kneeY, Hj[2] + 6];
    const A = [K[0] + la[0] * Ls, ankY, K[2] - 6];
    prims.push({ t: 'rc', ax: Hj[0], ay: Hj[1], az: Hj[2], bx: K[0], by: K[1], bz: K[2], r1: thR, r2: thR * 0.62, k: 40 * s });
    prims.push({ t: 'rc', ax: K[0], ay: K[1], az: K[2], bx: A[0], by: A[1], bz: A[2], r1: thR * 0.62, r2: thR * 0.40, k: 16 * s });
    prims.push({ t: 'el', cx: A[0], cy: 34 * s, cz: A[2] + 48 * s, rx: 44 * s, ry: 34 * s, rz: 122 * s, k: 18 * s });
    joints[sd > 0 ? 'L' : 'R'] = { S: S, E: E, W: W, a: a, uaR: uaR, Hj: Hj, K: K, A: A, la: la, thR: thR };
  });
  prims.push({ t: 'rc', ax: 0, ay: yNeck - 34 * s, az: -14, bx: 0, by: yChin + 8 * s, bz: -4, r1: neckR * 1.02, r2: neckR * 0.95, k: 30 * s });
  prims.push({ t: 'el', cx: 0, cy: yChin + 0.064 * H, cz: 8 * s, rx: 0.058 * H, ry: 0.074 * H, rz: 0.068 * H, k: 22 * s });
  prims.forEach(function (P) {
    let r;
    if (P.t === 'rc') { r = Math.max(P.r1, P.r2) + P.k + 12; P.bb = [Math.min(P.ax, P.bx) - r, Math.min(P.ay, P.by) - r, Math.min(P.az, P.bz) - r, Math.max(P.ax, P.bx) + r, Math.max(P.ay, P.by) + r, Math.max(P.az, P.bz) + r]; }
    else { r = P.k + 12; P.bb = [P.cx - P.rx - r, P.cy - P.ry - r, P.cz - P.rz - r, P.cx + P.rx + r, P.cy + P.ry + r, P.cz + P.rz + r]; }
  });
  const sdf = function (x, y, z) {
    let d = sdTorso(x, y, z);
    for (let i = 0; i < prims.length; i++) {
      const P = prims[i], b = P.bb;
      if (x < b[0] || y < b[1] || z < b[2] || x > b[3] || y > b[4] || z > b[5]) continue;
      const di = P.t === 'rc' ? AT.sdRoundCone(x, y, z, P) : AT.sdEllipsoid(x, y, z, P);
      d = AT.smin(d, di, P.k);
    }
    return d;
  };
  // sampled grid
  const g = Math.max(6, Math.round(7.5 * s));
  let bx0 = -shH - m.arm - 60, bx1 = -bx0;
  const reachX = Math.max(Math.abs(joints.L.W[0]) + 140 * s, hip[0] + 120, shH + 120);
  bx0 = -reachX; bx1 = reachX;
  const by0 = -10, by1 = H + 40, bz0 = -260 * s, bz1 = 260 * s;
  const nx = Math.ceil((bx1 - bx0) / g) + 1, ny = Math.ceil((by1 - by0) / g) + 1, nz = Math.ceil((bz1 - bz0) / g) + 1;
  const grid = new Float32Array(nx * ny * nz);
  const CL = 80;
  // coarse-to-fine: exact SDF only near the surface (SDF is 1-Lipschitz)
  const CS = 4, cnx = Math.ceil((nx - 1) / CS) + 1, cny = Math.ceil((ny - 1) / CS) + 1, cnz = Math.ceil((nz - 1) / CS) + 1;
  const coarse = new Float32Array(cnx * cny * cnz);
  for (let k = 0; k < cnz; k++) for (let j = 0; j < cny; j++) for (let i = 0; i < cnx; i++)
    coarse[i + cnx * (j + cny * k)] = sdf(bx0 + i * CS * g, by0 + j * CS * g, bz0 + k * CS * g);
  const diag = Math.sqrt(3) * CS * g;
  for (let k = 0; k < nz; k++) {
    const z = bz0 + k * g, ck = Math.min(cnz - 2, Math.floor(k / CS)), wz = k / CS - ck;
    for (let j = 0; j < ny; j++) {
      const y = by0 + j * g, cj = Math.min(cny - 2, Math.floor(j / CS)), wy = j / CS - cj;
      for (let i = 0; i < nx; i++) {
        const ci = Math.min(cnx - 2, Math.floor(i / CS)), wx = i / CS - ci;
        const o = ci + cnx * (cj + cny * ck), nxy = cnx * cny;
        const c000 = coarse[o], c100 = coarse[o + 1], c010 = coarse[o + cnx], c110 = coarse[o + cnx + 1];
        const c001 = coarse[o + nxy], c101 = coarse[o + nxy + 1], c011 = coarse[o + nxy + cnx], c111 = coarse[o + nxy + cnx + 1];
        const mn = Math.min(Math.abs(c000), Math.abs(c100), Math.abs(c010), Math.abs(c110), Math.abs(c001), Math.abs(c101), Math.abs(c011), Math.abs(c111));
        let d;
        if (mn > diag && (c000 > 0) === (c111 > 0)) {
          const x00 = c000 + (c100 - c000) * wx, x10 = c010 + (c110 - c010) * wx, x01 = c001 + (c101 - c001) * wx, x11 = c011 + (c111 - c011) * wx;
          const y0 = x00 + (x10 - x00) * wy, y1 = x01 + (x11 - x01) * wy;
          d = y0 + (y1 - y0) * wz;
        } else d = sdf(bx0 + i * g, y, z);
        grid[i + nx * (j + ny * k)] = d > CL ? CL : d < -CL ? -CL : d;
      }
    }
  }
  const G = { x0: bx0, y0: by0, z0: bz0, g: g, nx: nx, ny: ny, nz: nz, data: grid };
  return { m: m, H: H, torso: torso, prims: prims, joints: joints, sdf: sdf, grid: G, ys: TP.ys, neckR: neckR, shoulderY: TP.shoulderY };
};
// trilinear sample: returns distance; writes gradient into out[0..2]
AT.gridSample = function (G, x, y, z, out) {
  const fx = (x - G.x0) / G.g, fy = (y - G.y0) / G.g, fz = (z - G.z0) / G.g;
  if (fx < 0 || fy < 0 || fz < 0 || fx >= G.nx - 1 || fy >= G.ny - 1 || fz >= G.nz - 1) return 1e3;
  const i = fx | 0, j = fy | 0, k = fz | 0, u = fx - i, v = fy - j, w = fz - k;
  const nx = G.nx, nxy = G.nx * G.ny, D = G.data, o = i + nx * j + nxy * k;
  const c000 = D[o], c100 = D[o + 1], c010 = D[o + nx], c110 = D[o + nx + 1];
  const c001 = D[o + nxy], c101 = D[o + nxy + 1], c011 = D[o + nxy + nx], c111 = D[o + nxy + nx + 1];
  const x00 = c000 + (c100 - c000) * u, x10 = c010 + (c110 - c010) * u, x01 = c001 + (c101 - c001) * u, x11 = c011 + (c111 - c011) * u;
  const y0 = x00 + (x10 - x00) * v, y1 = x01 + (x11 - x01) * v;
  const d = y0 + (y1 - y0) * w;
  if (out) {
    const dx0 = (c100 - c000) + ((c110 - c010) - (c100 - c000)) * v, dx1 = (c101 - c001) + ((c111 - c011) - (c101 - c001)) * v;
    out[0] = (dx0 + (dx1 - dx0) * w) / G.g;
    out[1] = ((x10 - x00) + ((x11 - x01) - (x10 - x00)) * w) / G.g;
    out[2] = (y1 - y0) / G.g;
  }
  return d;
};
// Surface nets mesh of the avatar grid (smooth mannequin)
AT.avatarMesh = function (av) {
  const G = av.grid, nx = G.nx, ny = G.ny, nz = G.nz, D = G.data, g = G.g;
  const idx = new Int32Array(nx * ny * nz).fill(-1);
  const P = [], N = [], F = [];
  const tmp = [0, 0, 0];
  const corner = [[0, 0, 0], [1, 0, 0], [0, 1, 0], [1, 1, 0], [0, 0, 1], [1, 0, 1], [0, 1, 1], [1, 1, 1]];
  const edges = [[0, 1], [2, 3], [4, 5], [6, 7], [0, 2], [1, 3], [4, 6], [5, 7], [0, 4], [1, 5], [2, 6], [3, 7]];
  const v = new Float32Array(8);
  for (let k = 0; k < nz - 1; k++) for (let j = 0; j < ny - 1; j++) for (let i = 0; i < nx - 1; i++) {
    let mask = 0;
    for (let c = 0; c < 8; c++) { const q = corner[c]; v[c] = D[(i + q[0]) + nx * ((j + q[1]) + ny * (k + q[2]))]; if (v[c] < 0) mask |= 1 << c; }
    if (mask === 0 || mask === 255) continue;
    let sx = 0, sy = 0, sz = 0, cnt = 0;
    for (const e of edges) {
      const a = v[e[0]], b = v[e[1]];
      if ((a < 0) === (b < 0)) continue;
      const t = a / (a - b), qa = corner[e[0]], qb = corner[e[1]];
      sx += qa[0] + (qb[0] - qa[0]) * t; sy += qa[1] + (qb[1] - qa[1]) * t; sz += qa[2] + (qb[2] - qa[2]) * t; cnt++;
    }
    const x = G.x0 + (i + sx / cnt) * g, y = G.y0 + (j + sy / cnt) * g, z = G.z0 + (k + sz / cnt) * g;
    idx[i + nx * (j + ny * k)] = P.length / 3;
    P.push(x, y, z);
    AT.gridSample(G, x, y, z, tmp);
    const L = Math.hypot(tmp[0], tmp[1], tmp[2]) || 1;
    N.push(tmp[0] / L, tmp[1] / L, tmp[2] / L);
  }
  const at = function (i, j, k) { return idx[i + nx * (j + ny * k)]; };
  for (let k = 1; k < nz - 1; k++) for (let j = 1; j < ny - 1; j++) for (let i = 1; i < nx - 1; i++) {
    const d0 = D[i + nx * (j + ny * k)] < 0;
    // x edge
    if ((D[(i + 1) + nx * (j + ny * k)] < 0) !== d0) {
      const a = at(i, j - 1, k - 1), b = at(i, j, k - 1), c = at(i, j, k), d = at(i, j - 1, k);
      if (a >= 0 && b >= 0 && c >= 0 && d >= 0) { if (d0) F.push(a, b, c, a, c, d); else F.push(a, c, b, a, d, c); }
    }
    if ((D[i + nx * ((j + 1) + ny * k)] < 0) !== d0) {
      const a = at(i - 1, j, k - 1), b = at(i - 1, j, k), c = at(i, j, k), d = at(i, j, k - 1);
      if (a >= 0 && b >= 0 && c >= 0 && d >= 0) { if (d0) F.push(a, b, c, a, c, d); else F.push(a, c, b, a, d, c); }
    }
    if ((D[i + nx * (j + ny * (k + 1))] < 0) !== d0) {
      const a = at(i - 1, j - 1, k), b = at(i, j - 1, k), c = at(i, j, k), d = at(i - 1, j, k);
      if (a >= 0 && b >= 0 && c >= 0 && d >= 0) { if (d0) F.push(a, b, c, a, c, d); else F.push(a, c, b, a, d, c); }
    }
  }
  return { pos: new Float32Array(P), nor: new Float32Array(N), idx: new Uint32Array(F) };
};
/* ---------- arrangement ---------- */
// Body slots: angle (deg, 0 = front, 90 = avatar's left side, 180 = back). Limb slots wrap around the limb axis.
AT.SLOTS = [
  { id: 'front', label: 'Front', kind: 'body', ang: 0 }, { id: 'frontL', label: 'Front left', kind: 'body', ang: 40 },
  { id: 'frontR', label: 'Front right', kind: 'body', ang: -40 }, { id: 'left', label: 'Left side', kind: 'body', ang: 90 },
  { id: 'right', label: 'Right side', kind: 'body', ang: -90 }, { id: 'backL', label: 'Back left', kind: 'body', ang: 140 },
  { id: 'backR', label: 'Back right', kind: 'body', ang: -140 }, { id: 'back', label: 'Back', kind: 'body', ang: 180 },
  { id: 'armL', label: 'Left arm', kind: 'arm', side: 'L', ang: 0 }, { id: 'armR', label: 'Right arm', kind: 'arm', side: 'R', ang: 0 },
  { id: 'neck', label: 'Neck', kind: 'neck', ang: 0 },
  { id: 'legLF', label: 'Left leg front', kind: 'leg', side: 'L', ang: 0 }, { id: 'legLB', label: 'Left leg back', kind: 'leg', side: 'L', ang: 180 },
  { id: 'legRF', label: 'Right leg front', kind: 'leg', side: 'R', ang: 0 }, { id: 'legRB', label: 'Right leg back', kind: 'leg', side: 'R', ang: 180 }
];
AT.slotById = function (id) { return AT.SLOTS.find(function (s) { return s.id === id; }) || AT.SLOTS[0]; };
AT.cross = function (a, b) { return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]; };
AT.norm = function (a) { const L = Math.hypot(a[0], a[1], a[2]) || 1; return [a[0] / L, a[1] / L, a[2] / L]; };
// returns frame {O, a(axis down), n(outward), r(viewer right), R}
AT.slotFrame = function (av, arr, vRange) {
  const sl = AT.slotById(arr.slot || 'front');
  const off = arr.off == null ? 60 : arr.off;
  const ang = ((arr.ang != null ? arr.ang : sl.ang) || 0) * AT.deg;
  if (sl.kind === 'body') {
    const h = arr.h != null ? arr.h : av.ys.waist;
    let R = 0;
    const y0 = h + vRange[0], y1 = h + vRange[1];
    for (let y = y0; y <= y1; y += 10) { const e = av.torso.eval(AT.clamp(y, av.torso.y0, av.torso.y1)); R = Math.max(R, Math.max(e[0], e[1] + Math.abs(e[2]))); }
    if (!R) R = 160;
    const n = [Math.sin(ang), 0, Math.cos(ang)], a = [0, -1, 0];
    return { O: [0, h, 0], a: a, n: n, r: AT.cross(n, a), R: R + off, t0: 0, wrapAxis: true };
  }
  if (sl.kind === 'neck') {
    const h = arr.h != null ? arr.h : av.ys.neck + 10;
    const n = [Math.sin(ang), 0, Math.cos(ang)], a = [0, -1, 0];
    return { O: [0, h, -10], a: a, n: n, r: AT.cross(n, a), R: av.neckR + off * 0.5 + 8, t0: 0 };
  }
  const J = av.joints[sl.side];
  const sd = sl.side === 'L' ? 1 : -1;
  const t0 = arr.t != null ? arr.t : 0;
  if (sl.kind === 'arm') {
    // wrap around the real bone chain shoulder -> elbow -> wrist (zero angle = top of the arm, positive = towards the front)
    const chain = AT.chainFrames([J.S, J.E || [(J.S[0] + J.W[0]) / 2, (J.S[1] + J.W[1]) / 2, (J.S[2] + J.W[2]) / 2], J.W], [0, 1, 0], [0, 0, 1], [sd, 0, 0], ang);
    const c0 = chain[0];
    return { O: J.S, a: c0.a, n: c0.n, r: c0.r, R: J.uaR + off * 0.7, t0: t0, chain: chain };
  }
  // leg: hip -> knee -> ankle (zero angle = front, positive = outward)
  const chain = AT.chainFrames([J.Hj, J.K || [(J.Hj[0] + J.A[0]) / 2, (J.Hj[1] + J.A[1]) / 2, (J.Hj[2] + J.A[2]) / 2], J.A], [0, 0, 1], [sd, 0, 0], [0, 0, 1], ang);
  const c0 = chain[0];
  return { O: J.Hj, a: c0.a, n: c0.n, r: c0.r, R: J.thR + off * 0.7, t0: t0, chain: chain };
};
// Frames along a limb chain. ref = zero-angle direction, ref2 = direction of positive angle, fb = fallback when ref is parallel to the bone.
AT.chainFrames = function (pts, ref, ref2, fb, ang) {
  const out = []; let t = 0;
  const proj = function (v, a) { const d = v[0] * a[0] + v[1] * a[1] + v[2] * a[2]; return [v[0] - a[0] * d, v[1] - a[1] * d, v[2] - a[2] * d]; };
  for (let i = 0; i < pts.length - 1; i++) {
    const A = pts[i], B = pts[i + 1], d = [B[0] - A[0], B[1] - A[1], B[2] - A[2]], L = Math.hypot(d[0], d[1], d[2]) || 1, a = [d[0] / L, d[1] / L, d[2] / L];
    let n0 = proj(ref, a); if (Math.hypot(n0[0], n0[1], n0[2]) < 0.2) n0 = proj(fb, a);
    n0 = AT.norm(n0);
    let q = proj(ref2, a); const qd = q[0] * n0[0] + q[1] * n0[1] + q[2] * n0[2]; q = [q[0] - n0[0] * qd, q[1] - n0[1] * qd, q[2] - n0[2] * qd];
    if (Math.hypot(q[0], q[1], q[2]) < 1e-6) q = AT.cross(a, n0);
    q = AT.norm(q);
    const n = AT.norm([n0[0] * Math.cos(ang) + q[0] * Math.sin(ang), n0[1] * Math.cos(ang) + q[1] * Math.sin(ang), n0[2] * Math.cos(ang) + q[2] * Math.sin(ang)]);
    out.push({ A: A, a: a, n: n, r: AT.cross(n, a), s: t, L: L });
    t += L;
  }
  return out;
};
// position on a limb chain at arc length t, wrap angle phi, radius R (smooth blend across joints)
AT.chainPos = function (chain, t, phi, R, flatU) {
  const at = function (c) {
    const u = t - c.s;
    if (flatU != null) return [c.A[0] + c.a[0] * u + c.n[0] * R + c.r[0] * flatU, c.A[1] + c.a[1] * u + c.n[1] * R + c.r[1] * flatU, c.A[2] + c.a[2] * u + c.n[2] * R + c.r[2] * flatU];
    const cr = Math.cos(phi) * R, sr = Math.sin(phi) * R;
    return [c.A[0] + c.a[0] * u + c.n[0] * cr + c.r[0] * sr, c.A[1] + c.a[1] * u + c.n[1] * cr + c.r[1] * sr, c.A[2] + c.a[2] * u + c.n[2] * cr + c.r[2] * sr];
  };
  let i = 0; while (i < chain.length - 1 && t > chain[i + 1].s) i++;
  let p = at(chain[i]);
  const BW = 45;
  const blend = function (j) { // junction between chain[j-1] and chain[j]
    const tj = chain[j].s, x = (t - (tj - BW)) / (2 * BW);
    if (x <= 0 || x >= 1) return null;
    const w = x * x * (3 - 2 * x), p1 = at(chain[j - 1]), p2 = at(chain[j]);
    return [p1[0] + (p2[0] - p1[0]) * w, p1[1] + (p2[1] - p1[1]) * w, p1[2] + (p2[2] - p1[2]) * w];
  };
  for (let j = 1; j < chain.length; j++) { const q = blend(j); if (q) { p = q; break; } }
  return p;
};
// map 2D mesh coordinates to 3D arranged positions
AT.arrangePositions = function (av, piece, mesh, out, o3) {
  const bb = AT.bbox(mesh.ol.poly), cx = bb.cx, cy = bb.cy;
  const arr = piece.arr || { slot: 'front' };
  // Radial head drape: a semicircular chador, with a shared polar datum
  // across the main cloth and the pieced lower crescent. No effect on other garments.
  if (arr.drape === 'chador') {
    const ox = arr.originX || 0, oy = arr.originY || 0;
    const crown = av.H + (arr.crownLift == null ? 12 : arr.crownLift);
    const z0 = arr.headZ || 0;
    for (let i = 0; i < mesh.n; i++) {
      const u = mesh.xy[2 * i] - ox, v = mesh.xy[2 * i + 1] - oy;
      const r = Math.hypot(u, v);
      const gap = 0.02 + 0.9 * Math.exp(-Math.pow((r - 225) / 200, 2));
      const phi = Math.PI + (2 - 2 * gap / Math.PI) * Math.atan2(u, v);
      const k = (o3 + i) * 3;
      out[k] = 0.5 * r * Math.sin(phi);
      const rr = Math.min(100, r * 0.5);
      out[k + 1] = r <= 200 ? crown - 100 + Math.sqrt(Math.max(0, 10000 - rr * rr)) : crown - 100 - Math.sqrt(0.75) * (r - 200);
      out[k + 2] = z0 + 0.5 * r * Math.cos(phi);
    }
    return { O: [0, crown, z0], a: [0, -1, 0], n: [0, 0, -1], r: [-1, 0, 0], R: 1 };
  }
  const F = AT.slotFrame(av, arr, [-(bb.y1 - cy), (cy - bb.y0)]);
  const flip = arr.flip ? -1 : 1;
  const R = arr.flat ? 1e5 : F.R;
  if (F.chain) {
    for (let i = 0; i < mesh.n; i++) {
      const u = (mesh.xy[2 * i] - cx) * flip, v = -(mesh.xy[2 * i + 1] - cy);
      const p = AT.chainPos(F.chain, F.t0 - v, u / F.R, F.R, arr.flat ? u : null);
      const k = (o3 + i) * 3; out[k] = p[0]; out[k + 1] = p[1]; out[k + 2] = p[2];
    }
    return F;
  }
  const shift = F.wrapAxis ? (arr.shift || 0) : 0; // lateral offset (mm) along the body, e.g. to place two open front panels side by side
  for (let i = 0; i < mesh.n; i++) {
    const u = (mesh.xy[2 * i] - cx) * flip + shift, v = -(mesh.xy[2 * i + 1] - cy);
    const phi = u / R, t = F.t0 - v;
    const cr = Math.cos(phi) * R, sr = Math.sin(phi) * R;
    const base = arr.flat ? [F.n[0] * F.R + F.r[0] * u, F.n[1] * F.R + F.r[1] * u, F.n[2] * F.R + F.r[2] * u] :
      [F.n[0] * cr + F.r[0] * sr, F.n[1] * cr + F.r[1] * sr, F.n[2] * cr + F.r[2] * sr];
    const k = (o3 + i) * 3;
    out[k] = F.O[0] + F.a[0] * t + base[0];
    out[k + 1] = F.O[1] + F.a[1] * t + base[1];
    out[k + 2] = F.O[2] + F.a[2] * t + base[2];
  }
  return F;
};
AT.arrangementPoints = function (av) {
  const pts = [];
  const hs = [['Chest', av.ys.bust], ['Waist', av.ys.waist], ['Hip', av.ys.hip], ['Thigh', av.ys.crotch - 120], ['Knee', av.ys.knee + 140]];
  AT.SLOTS.forEach(function (sl) {
    if (sl.kind === 'body') hs.forEach(function (h) {
      const F = AT.slotFrame(av, { slot: sl.id, h: h[1], off: 60 }, [-10, 10]);
      pts.push({ slot: sl.id, h: h[1], label: sl.label + ' · ' + h[0], pos: [F.O[0] + F.n[0] * F.R, F.O[1], F.O[2] + F.n[2] * F.R] });
    });
    else {
      const F = AT.slotFrame(av, { slot: sl.id, off: 60, t: 0 }, [-10, 10]);
      const tt = sl.kind === 'arm' ? 140 : sl.kind === 'leg' ? 260 : 0;
      const pos = F.chain ? AT.chainPos(F.chain, tt, 0, F.R) : [F.O[0] + F.a[0] * tt + F.n[0] * F.R, F.O[1] + F.a[1] * tt + F.n[1] * F.R, F.O[2] + F.a[2] * tt + F.n[2] * F.R];
      pts.push({ slot: sl.id, t: tt, label: sl.label, pos: pos });
    }
  });
  return pts;
};


/* ============================================================
   MAKEHUMAN AVATAR (CC0 1.0 assets): morph-target body with tailor measurements
   ============================================================ */
AT.MH_URL = 'avatar/makehuman-cc0-v2.bin';
AT.MEAS = [
  { k: 'height', label: 'Height', grp: 'Body' },
  { k: 'bust', label: 'Bust / chest', grp: 'Body', t: 'measure-bust-circ', type: 'circ' },
  { k: 'underbust', label: 'Underbust', grp: 'Body', t: 'measure-underbust-circ', type: 'circ' },
  { k: 'waist', label: 'Waist', grp: 'Body', t: 'measure-waist-circ', type: 'circ' },
  { k: 'hip', label: 'Hip', grp: 'Body', t: 'measure-hips-circ', type: 'circ' },
  { k: 'neck', label: 'Neck', grp: 'Neck & torso', t: 'measure-neck-circ', type: 'circ' },
  { k: 'shoulder', label: 'Shoulder width', grp: 'Neck & torso', t: 'measure-shoulder-dist', type: 'shoulder' },
  { k: 'acrossFront', label: 'Across front', grp: 'Neck & torso', t: 'measure-frontchest-dist', type: 'dist' },
  { k: 'backLength', label: 'Back length (nape–waist)', grp: 'Neck & torso', t: 'measure-napetowaist-dist', type: 'dist' },
  { k: 'waistToHip', label: 'Waist to hip', grp: 'Neck & torso', t: 'measure-waisttohip-dist', type: 'dist' },
  { k: 'arm', label: 'Arm length (shoulder–wrist)', grp: 'Arms', t: ['measure-upperarm-length', 'measure-lowerarm-length'], type: 'arm' },
  { k: 'upperArm', label: 'Upper arm', grp: 'Arms', t: 'measure-upperarm-circ', type: 'circ' },
  { k: 'wrist', label: 'Wrist', grp: 'Arms', t: 'measure-wrist-circ', type: 'circ' },
  { k: 'crotch', label: 'Crotch height (inseam)', grp: 'Legs', t: ['measure-upperleg-height', 'measure-lowerleg-height'], type: 'crotch' },
  { k: 'kneeHeight', label: 'Knee to floor', grp: 'Legs', t: 'measure-lowerleg-height', type: 'dist' },
  { k: 'thigh', label: 'Thigh', grp: 'Legs', t: 'measure-thigh-circ', type: 'circ' },
  { k: 'knee', label: 'Knee', grp: 'Legs', t: 'measure-knee-circ', type: 'circ' },
  { k: 'calf', label: 'Calf', grp: 'Legs', t: 'measure-calf-circ', type: 'circ' },
  { k: 'ankle', label: 'Ankle', grp: 'Legs', t: 'measure-ankle-circ', type: 'circ' }
];
AT.MH_FIT_TARGETS = ['measure-neck-circ', 'measure-upperarm-circ', 'measure-upperarm-length', 'measure-lowerarm-length', 'measure-wrist-circ', 'measure-frontchest-dist', 'measure-bust-circ', 'measure-underbust-circ', 'measure-waist-circ', 'measure-napetowaist-dist', 'measure-waisttohip-dist', 'measure-shoulder-dist', 'measure-hips-circ', 'measure-upperleg-height', 'measure-thigh-circ', 'measure-lowerleg-height', 'measure-calf-circ', 'measure-ankle-circ', 'measure-knee-circ'];
AT.mhParse = function (buf) {
  const u8 = new Uint8Array(buf);
  if (String.fromCharCode(u8[0], u8[1], u8[2], u8[3]) !== 'ATMH') throw new Error('Not an Atelier avatar pack');
  const hlen = new DataView(buf).getUint32(4, true);
  const h = JSON.parse(new TextDecoder().decode(u8.subarray(8, 8 + hlen)));
  const base0 = 8 + hlen;
  const view = function (name) {
    const a = h.arrays[name]; if (!a) return null;
    const C = { float32: Float32Array, uint16: Uint16Array, int16: Int16Array, uint8: Uint8Array, int32: Int32Array, uint32: Uint32Array }[a.dtype];
    return new C(buf, base0 + a.offset, a.length);
  };
  const M = { h: h, nV: h.nV, bodyV: h.bodyV, q: h.q, base: view('base'), bodyTris: view('bodyTris'), eyeTris: view('eyeTris'), w: { armL: view('w_armL'), armR: view('w_armR'), legL: view('w_legL'), legR: view('w_legR') }, targets: {}, measures: h.measures, lm: h.landmarks, joints: h.joints };
  h.targets.forEach(function (nm) { M.targets[nm] = { i: view('ti_' + nm), d: view('td_' + nm) }; });
  // probe vertex set for fast fitting
  const set = new Set();
  Object.values(M.measures).forEach(function (l) { l.forEach(function (v) { set.add(v); }); });
  Object.values(M.lm).forEach(function (v) { set.add(v); });
  const ys = [];
  for (let v = 0; v < M.bodyV; v++) ys.push([M.base[v * 3 + 1], v]);
  ys.sort(function (a, b) { return a[0] - b[0]; });
  for (let i = 0; i < 260; i++) { set.add(ys[i][1]); set.add(ys[ys.length - 1 - i][1]); }
  const probe = Int32Array.from(Array.from(set).sort(function (a, b) { return a - b; }));
  const local = new Int32Array(M.nV).fill(-1); probe.forEach(function (g, i) { local[g] = i; });
  M.probe = probe; M.local = local; M.pbase = new Float32Array(probe.length * 3);
  probe.forEach(function (g, i) { M.pbase[i * 3] = M.base[g * 3]; M.pbase[i * 3 + 1] = M.base[g * 3 + 1]; M.pbase[i * 3 + 2] = M.base[g * 3 + 2]; });
  M.ptargets = {};
  Object.keys(M.targets).forEach(function (nm) {
    const t = M.targets[nm], li = [], dd = [];
    for (let e = 0; e < t.i.length; e++) { const l = local[t.i[e]]; if (l >= 0) { li.push(l); dd.push(t.d[e * 3], t.d[e * 3 + 1], t.d[e * 3 + 2]); } }
    M.ptargets[nm] = { i: Int32Array.from(li), d: Int16Array.from(dd) };
  });
  M.pBodyMask = new Uint8Array(probe.length); probe.forEach(function (g, i) { M.pBodyMask[i] = g < M.bodyV ? 1 : 0; });
  return M;
};
AT.mhLoad = function (gzUrl, rawUrl) {
  if (AT.MH.model) return Promise.resolve(AT.MH.model);
  if (AT.MH.loading) return AT.MH.loading;
  AT.MH.loading = (async function () {
    let buf = null;
    if (typeof DecompressionStream !== 'undefined') {
      try {
        const r = await fetch(gzUrl); if (!r.ok) throw new Error('HTTP ' + r.status);
        const ds = r.body.pipeThrough(new DecompressionStream('gzip'));
        buf = await new Response(ds).arrayBuffer();
      } catch (e) { buf = null; }
    }
    if (!buf) { const r = await fetch(rawUrl); if (!r.ok) throw new Error('Avatar pack HTTP ' + r.status); buf = await r.arrayBuffer(); }
    AT.MH.model = AT.mhParse(buf);
    return AT.MH.model;
  })();
  AT.MH.loading.catch(function () { AT.MH.loading = null; });
  return AT.MH.loading;
};
AT.MH = { model: null, loading: null };
AT.mhTri = function (v) { const mx = Math.max(0, v * 2 - 1), mn = Math.max(0, 1 - v * 2); return [mn, 1 - (mx + mn), mx]; };
// compose positions; probe=true composes only the fitting probe vertices (unscaled, ungrounded)
AT.mhCompose = function (M, P, probe) {
  const base = probe ? M.pbase : M.base, T = probe ? M.ptargets : M.targets;
  const out = new Float32Array(base);
  const q = M.q;
  const add = function (nm, f) {
    if (!f) return; const t = T[nm]; if (!t) return;
    const I = t.i, Dd = t.d, s = q * f;
    for (let e = 0; e < I.length; e++) { const k = I[e] * 3; out[k] += Dd[e * 3] * s; out[k + 1] += Dd[e * 3 + 1] * s; out[k + 2] += Dd[e * 3 + 2] * s; }
  };
  const g = AT.clamp(P.gender == null ? 0 : P.gender, 0, 1);
  add('g_female', 1 - g); add('g_male', g);
  const mu = AT.mhTri(AT.clamp(P.muscle == null ? 0.5 : P.muscle, 0, 1)), we = AT.mhTri(AT.clamp(P.weight == null ? 0.5 : P.weight, 0, 1));
  const MU = ['minmuscle', 'averagemuscle', 'maxmuscle'], WE = ['minweight', 'averageweight', 'maxweight'];
  [['female', 1 - g], ['male', g]].forEach(function (gg) {
    if (!gg[1]) return;
    for (let i = 0; i < 3; i++) for (let j = 0; j < 3; j++) add('universal-' + gg[0] + '-young-' + MU[i] + '-' + WE[j], gg[1] * mu[i] * we[j]);
  });
  const meas = P.meas || {};
  Object.keys(meas).forEach(function (k) { const w = meas[k]; if (w) add(k + (w > 0 ? '-incr' : '-decr'), Math.abs(w)); });
  // fitting-form bust: MakeHuman's own nipple targets flatten detail smaller than the cloth particle spacing
  const sm = P.smooth == null ? 1 : P.smooth;
  if (sm) { add('nipple-point-decr', sm); add('nipple-size-decr', sm); }
  return out;
};
// geometry helpers on a position array with optional global->local map
AT.mhP = function (pos, map, g) { const i = (map ? map[g] : g) * 3; return [pos[i], pos[i + 1], pos[i + 2]]; };
AT.mhTape = function (pts) { // convex-hull perimeter of a loop in its own plane (how a tape measure reads)
  const n = pts.length; let c = [0, 0, 0];
  pts.forEach(function (p) { c[0] += p[0] / n; c[1] += p[1] / n; c[2] += p[2] / n; });
  let nx = 0, ny = 0, nz = 0; // Newell normal
  for (let i = 0; i < n; i++) { const a = pts[i], b = pts[(i + 1) % n]; nx += (a[1] - b[1]) * (a[2] + b[2]); ny += (a[2] - b[2]) * (a[0] + b[0]); nz += (a[0] - b[0]) * (a[1] + b[1]); }
  let L = Math.hypot(nx, ny, nz); if (L < 1e-9) { nx = 0; ny = 1; nz = 0; L = 1; }
  const N = [nx / L, ny / L, nz / L];
  const ref = Math.abs(N[1]) < 0.9 ? [0, 1, 0] : [1, 0, 0];
  const e1 = AT.norm(AT.cross(ref, N)), e2 = AT.cross(N, e1);
  const q = pts.map(function (p) { const d = [p[0] - c[0], p[1] - c[1], p[2] - c[2]]; return [d[0] * e1[0] + d[1] * e1[1] + d[2] * e1[2], d[0] * e2[0] + d[1] * e2[1] + d[2] * e2[2]]; });
  q.sort(function (a, b) { return a[0] - b[0] || a[1] - b[1]; });
  const cr = function (o, a, b) { return (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]); };
  const lo = [], up = [];
  q.forEach(function (p) { while (lo.length >= 2 && cr(lo[lo.length - 2], lo[lo.length - 1], p) <= 0) lo.pop(); lo.push(p); });
  for (let i = q.length - 1; i >= 0; i--) { const p = q[i]; while (up.length >= 2 && cr(up[up.length - 2], up[up.length - 1], p) <= 0) up.pop(); up.push(p); }
  const hull = lo.slice(0, -1).concat(up.slice(0, -1));
  let per = 0; for (let i = 0; i < hull.length; i++) { const a = hull[i], b = hull[(i + 1) % hull.length]; per += Math.hypot(a[0] - b[0], a[1] - b[1]); }
  return { len: per, hull3: hull.map(function (h) { return [c[0] + e1[0] * h[0] + e2[0] * h[1], c[1] + e1[1] * h[0] + e2[1] * h[1], c[2] + e1[2] * h[0] + e2[2] * h[1]]; }), c: c };
};
AT.mhHeightRange = function (M, pos, map) { // body min/max y (probe or full)
  let y0 = Infinity, y1 = -Infinity;
  if (map) { for (let i = 0; i < M.probe.length; i++) if (M.pBodyMask[i]) { const y = pos[i * 3 + 1]; if (y < y0) y0 = y; if (y > y1) y1 = y; } }
  else for (let v = 0; v < M.bodyV; v++) { const y = pos[v * 3 + 1]; if (y < y0) y0 = y; if (y > y1) y1 = y; }
  return [y0, y1];
};
AT.mhMeasureOne = function (M, pos, map, def, yr) {
  const P = function (g) { return AT.mhP(pos, map, g); };
  const dist = function (a, b) { const p = P(a), q = P(b); return Math.hypot(p[0] - q[0], p[1] - q[1], p[2] - q[2]); };
  if (def.k === 'height') return yr[1] - yr[0];
  if (def.type === 'circ') return AT.mhTape(M.measures[def.t].map(P)).len;
  if (def.type === 'dist') { const l = M.measures[def.t]; return dist(l[0], l[1]); }
  if (def.type === 'shoulder') return dist(M.lm.shoulderL, M.lm.shoulderR);
  if (def.type === 'arm') { const a = M.measures['measure-upperarm-length'], b = M.measures['measure-lowerarm-length']; return dist(a[0], a[1]) + dist(b[0], b[1]); }
  if (def.type === 'crotch') return P(M.lm.crotch)[1] - yr[0];
  return 0;
};
AT.mhMeasureAll = function (M, pos, map) {
  const yr = AT.mhHeightRange(M, pos, map), out = {};
  AT.MEAS.forEach(function (def) { out[def.k] = AT.mhMeasureOne(M, pos, map, def, yr); });
  return out;
};
// Fit body to tailor measurements: Levenberg–Marquardt over [weight, 19 measurement targets], uniform scale for height.
AT.mhFit = function (M, spec) {
  const sex = spec.sex === 'male' ? 1 : 0;
  const muscle = spec.muscle == null ? 0.5 : spec.muscle;
  const lockW = spec.fat != null;
  const want = {};
  AT.MEAS.forEach(function (d) { const v = spec[d.k]; if (v != null && v !== '' && isFinite(v) && v > 0) want[d.k] = +v; });
  const keys = Object.keys(want).filter(function (k) { return k !== 'height'; });
  const NT = AT.MH_FIT_TARGETS.length;
  const np = NT + 1;
  let p = new Float64Array(np); p[0] = lockW ? spec.fat : 0.5;
  const lo = new Float64Array(np).fill(-1.6), hi = new Float64Array(np).fill(1.6); lo[0] = 0; hi[0] = 1;
  const evalP = function (pp) {
    const meas = {}; for (let j = 0; j < NT; j++) meas[AT.MH_FIT_TARGETS[j]] = pp[j + 1];
    const pos = AT.mhCompose(M, { gender: sex, muscle: muscle, weight: pp[0], meas: meas }, true);
    const yr = AT.mhHeightRange(M, pos, M.local);
    const h = yr[1] - yr[0], s = want.height ? want.height / h : (spec.scale || 1);
    const r = [];
    keys.forEach(function (k) { const d = AT.MEAS.find(function (x) { return x.k === k; }); const m = AT.mhMeasureOne(M, pos, M.local, d, yr) * s; r.push((m - want[k]) / Math.max(4, want[k] * 0.006)); });
    for (let j = 1; j < np; j++) r.push(pp[j] * 0.35);
    r.push(lockW ? (pp[0] - spec.fat) * 50 : (pp[0] - 0.5) * 0.6);
    return { r: r, s: s };
  };
  let cur = evalP(p), cost = cur.r.reduce(function (a, b) { return a + b * b; }, 0), lambda = 1e-2;
  for (let it = 0; it < 40; it++) {
    const m = cur.r.length, J = [];
    for (let j = 0; j < np; j++) {
      if (j === 0 && lockW) { J.push(new Float64Array(m)); continue; }
      const h = 1e-3, pp = Float64Array.from(p); pp[j] += h;
      const e = evalP(pp).r, col = new Float64Array(m);
      for (let i = 0; i < m; i++) col[i] = (e[i] - cur.r[i]) / h;
      J.push(col);
    }
    // normal equations
    const A = [], g = new Float64Array(np);
    for (let a = 0; a < np; a++) { A.push(new Float64Array(np)); for (let b = 0; b <= a; b++) { let s = 0; for (let i = 0; i < m; i++) s += J[a][i] * J[b][i]; A[a][b] = s; A[b] && (A[b][a] = s); } let s2 = 0; for (let i = 0; i < m; i++) s2 += J[a][i] * cur.r[i]; g[a] = s2; }
    for (let a = 0; a < np; a++) for (let b = a + 1; b < np; b++) A[a][b] = A[b][a];
    let improved = false;
    for (let tries = 0; tries < 8; tries++) {
      const Ad = A.map(function (row, i) { const rr = Float64Array.from(row); rr[i] += lambda * (row[i] + 1e-6); return rr; });
      const dx = AT.solveLin(Ad, Array.from(g).map(function (v) { return -v; }));
      if (!dx) { lambda *= 10; continue; }
      const pn = Float64Array.from(p); for (let j = 0; j < np; j++) pn[j] = AT.clamp(pn[j] + dx[j], lo[j], hi[j]);
      const nx = evalP(pn), nc = nx.r.reduce(function (a2, b2) { return a2 + b2 * b2; }, 0);
      if (nc < cost) { p = pn; cur = nx; lambda = Math.max(1e-6, lambda * 0.3); improved = cost - nc > 1e-6 * cost; cost = nc; break; }
      lambda *= 10;
    }
    if (!improved) break;
  }
  const meas = {}; for (let j = 0; j < NT; j++) meas[AT.MH_FIT_TARGETS[j]] = p[j + 1];
  return { gender: sex, muscle: muscle, weight: p[0], meas: meas, scale: cur.s, want: want };
};
AT.solveLin = function (A, b) { // Gaussian elimination with partial pivoting
  const n = b.length, M2 = A.map(function (r, i) { return Array.from(r).concat([b[i]]); });
  for (let c = 0; c < n; c++) {
    let piv = c; for (let r = c + 1; r < n; r++) if (Math.abs(M2[r][c]) > Math.abs(M2[piv][c])) piv = r;
    if (Math.abs(M2[piv][c]) < 1e-12) return null;
    const t = M2[c]; M2[c] = M2[piv]; M2[piv] = t;
    for (let r = c + 1; r < n; r++) { const f = M2[r][c] / M2[c][c]; if (!f) continue; for (let k = c; k <= n; k++) M2[r][k] -= f * M2[c][k]; }
  }
  const x = new Array(n).fill(0);
  for (let r = n - 1; r >= 0; r--) { let s = M2[r][n]; for (let k = r + 1; k < n; k++) s -= M2[r][k] * x[k]; x[r] = s / M2[r][r]; }
  return x;
};
AT.mhJoint = function (M, pos, name) { const l = M.joints[name]; let c = [0, 0, 0]; l.forEach(function (v) { c[0] += pos[v * 3]; c[1] += pos[v * 3 + 1]; c[2] += pos[v * 3 + 2]; }); return [c[0] / l.length, c[1] / l.length, c[2] / l.length]; };
// Pose: rotate arms about the shoulder joints and legs about the hips (frontal plane), linear-blend skinning
AT.mhPose = function (M, pos, armAngle, legSpread) {
  const rot = function (wArr, pivot, ang) {
    if (Math.abs(ang) < 1e-4) return;
    const c = Math.cos(ang), s = Math.sin(ang);
    for (let v = 0; v < M.nV; v++) {
      const wt = wArr[v] / 255; if (!wt) continue;
      const k = v * 3, dx = pos[k] - pivot[0], dy = pos[k + 1] - pivot[1];
      const rx = pivot[0] + dx * c - dy * s, ry = pivot[1] + dx * s + dy * c;
      pos[k] += (rx - pos[k]) * wt; pos[k + 1] += (ry - pos[k + 1]) * wt;
    }
  };
  const info = {};
  ['l', 'r'].forEach(function (sd) {
    const S = AT.mhJoint(M, pos, 'joint-' + sd + '-shoulder'), W = AT.mhJoint(M, pos, 'joint-' + sd + '-hand');
    const nat = Math.atan2(Math.abs(W[0] - S[0]), S[1] - W[1]);
    info[sd + 'Arm'] = nat / AT.deg;
    if (armAngle != null) { const d = armAngle * AT.deg - nat; rot(sd === 'l' ? M.w.armL : M.w.armR, S, sd === 'l' ? d : -d); }
    const Hh = AT.mhJoint(M, pos, 'joint-' + sd + '-upper-leg'), A = AT.mhJoint(M, pos, 'joint-' + sd + '-ankle');
    const natL = Math.atan2(Math.abs(A[0] - Hh[0]), Hh[1] - A[1]);
    info[sd + 'Leg'] = natL / AT.deg;
    if (legSpread != null) { const d = legSpread * AT.deg - natL; rot(sd === 'l' ? M.w.legL : M.w.legR, Hh, sd === 'l' ? d : -d); }
  });
  return info;
};
// Signed distance grid from a triangle mesh: exact narrow band + flood-fill sign + chamfer propagation
AT.meshSDF = function (pos, tris, g, pad, nrm) {
  let x0 = Infinity, y0 = Infinity, z0 = Infinity, x1 = -Infinity, y1 = -Infinity, z1 = -Infinity;
  for (let t = 0; t < tris.length; t++) { const k = tris[t] * 3; const x = pos[k], y = pos[k + 1], z = pos[k + 2]; if (x < x0) x0 = x; if (y < y0) y0 = y; if (z < z0) z0 = z; if (x > x1) x1 = x; if (y > y1) y1 = y; if (z > z1) z1 = z; }
  x0 -= pad; y0 = Math.min(y0 - pad, -10); z0 -= pad; x1 += pad; y1 += pad; z1 += pad;
  const nx = Math.ceil((x1 - x0) / g) + 1, ny = Math.ceil((y1 - y0) / g) + 1, nz = Math.ceil((z1 - z0) / g) + 1, nxy = nx * ny, N = nxy * nz;
  const D = new Float32Array(N).fill(1e9), sg = new Int8Array(N);
  const band = g * 1.9;
  for (let t = 0; t < tris.length; t += 3) {
    const ia = tris[t], ib = tris[t + 1], ic = tris[t + 2];
    const ax = pos[ia * 3], ay = pos[ia * 3 + 1], az = pos[ia * 3 + 2], bx = pos[ib * 3], by = pos[ib * 3 + 1], bz = pos[ib * 3 + 2], cx = pos[ic * 3], cy = pos[ic * 3 + 1], cz = pos[ic * 3 + 2];
    const i0 = Math.max(0, Math.floor((Math.min(ax, bx, cx) - band - x0) / g)), i1 = Math.min(nx - 1, Math.ceil((Math.max(ax, bx, cx) + band - x0) / g));
    const j0 = Math.max(0, Math.floor((Math.min(ay, by, cy) - band - y0) / g)), j1 = Math.min(ny - 1, Math.ceil((Math.max(ay, by, cy) + band - y0) / g));
    const k0 = Math.max(0, Math.floor((Math.min(az, bz, cz) - band - z0) / g)), k1 = Math.min(nz - 1, Math.ceil((Math.max(az, bz, cz) + band - z0) / g));
    const abx = bx - ax, aby = by - ay, abz = bz - az, acx = cx - ax, acy = cy - ay, acz = cz - az;
    for (let k = k0; k <= k1; k++) { const pz = z0 + k * g;
      for (let j = j0; j <= j1; j++) { const py = y0 + j * g;
        for (let i = i0; i <= i1; i++) { const px = x0 + i * g;
          // closest point on triangle (Ericson)
          const apx = px - ax, apy = py - ay, apz = pz - az;
          const d1 = abx * apx + aby * apy + abz * apz, d2 = acx * apx + acy * apy + acz * apz;
          let u, v;
          if (d1 <= 0 && d2 <= 0) { u = 0; v = 0; }
          else {
            const bpx = px - bx, bpy = py - by, bpz = pz - bz, d3 = abx * bpx + aby * bpy + abz * bpz, d4 = acx * bpx + acy * bpy + acz * bpz;
            if (d3 >= 0 && d4 <= d3) { u = 1; v = 0; }
            else {
              const vc = d1 * d4 - d3 * d2;
              if (vc <= 0 && d1 >= 0 && d3 <= 0) { u = d1 / (d1 - d3); v = 0; }
              else {
                const cpx = px - cx, cpy = py - cy, cpz = pz - cz, d5 = abx * cpx + aby * cpy + abz * cpz, d6 = acx * cpx + acy * cpy + acz * cpz;
                if (d6 >= 0 && d5 <= d6) { u = 0; v = 1; }
                else {
                  const vb = d5 * d2 - d1 * d6;
                  if (vb <= 0 && d2 >= 0 && d6 <= 0) { u = 0; v = d2 / (d2 - d6); }
                  else {
                    const va = d3 * d6 - d5 * d4;
                    if (va <= 0 && (d4 - d3) >= 0 && (d5 - d6) >= 0) { const ww = (d4 - d3) / ((d4 - d3) + (d5 - d6)); u = 1 - ww; v = ww; }
                    else { const den = 1 / (va + vb + vc); u = vb * den; v = vc * den; }
                  }
                }
              }
            }
          }
          const qx = ax + abx * u + acx * v, qy = ay + aby * u + acy * v, qz = az + abz * u + acz * v;
          const ddx = px - qx, ddy = py - qy, ddz = pz - qz, d = Math.sqrt(ddx * ddx + ddy * ddy + ddz * ddz);
          const o = i + nx * j + nxy * k;
          if (d < D[o]) {
            D[o] = d;
            const w0 = 1 - u - v;
            const n0 = w0 * nrm[ia * 3] + u * nrm[ib * 3] + v * nrm[ic * 3], n1 = w0 * nrm[ia * 3 + 1] + u * nrm[ib * 3 + 1] + v * nrm[ic * 3 + 1], n2 = w0 * nrm[ia * 3 + 2] + u * nrm[ib * 3 + 2] + v * nrm[ic * 3 + 2];
            sg[o] = (ddx * n0 + ddy * n1 + ddz * n2) >= 0 ? 1 : -1;
          }
        }
      }
    }
  }
  // flood fill outside through non-wall cells (6-connected)
  const wall = g * 0.87, out = new Uint8Array(N), st = new Int32Array(N);
  let sp = 0;
  const push = function (o) { if (!out[o] && D[o] >= wall) { out[o] = 1; st[sp++] = o; } };
  for (let k = 0; k < nz; k++) for (let j = 0; j < ny; j++) { push(nxy * k + nx * j); push(nxy * k + nx * j + nx - 1); }
  for (let k = 0; k < nz; k++) for (let i = 0; i < nx; i++) { push(nxy * k + i); push(nxy * k + nx * (ny - 1) + i); }
  for (let j = 0; j < ny; j++) for (let i = 0; i < nx; i++) { push(nx * j + i); push(nxy * (nz - 1) + nx * j + i); }
  while (sp) {
    const o = st[--sp], k = (o / nxy) | 0, r = o - k * nxy, j = (r / nx) | 0, i = r - j * nx;
    if (i > 0) push(o - 1); if (i < nx - 1) push(o + 1); if (j > 0) push(o - nx); if (j < ny - 1) push(o + nx); if (k > 0) push(o - nxy); if (k < nz - 1) push(o + nxy);
  }
  // magnitudes: chamfer propagation for cells outside the band
  const a1 = g, a2 = g * Math.SQRT2, a3 = g * Math.sqrt(3);
  const nb = [];
  for (let dk = -1; dk <= 1; dk++) for (let dj = -1; dj <= 1; dj++) for (let di = -1; di <= 1; di++) {
    const c = Math.abs(di) + Math.abs(dj) + Math.abs(dk); if (!c) continue;
    nb.push([di, dj, dk, c === 1 ? a1 : c === 2 ? a2 : a3]);
  }
  const fwd = nb.filter(function (n) { return n[2] < 0 || (n[2] === 0 && (n[1] < 0 || (n[1] === 0 && n[0] < 0))); });
  const bwd = nb.filter(function (n) { return fwd.indexOf(n) < 0; });
  const pass = function (list, dir) {
    const ks = dir > 0 ? 0 : nz - 1, ke = dir > 0 ? nz : -1;
    for (let k = ks; k !== ke; k += dir) for (let j = dir > 0 ? 0 : ny - 1; j !== (dir > 0 ? ny : -1); j += dir) for (let i = dir > 0 ? 0 : nx - 1; i !== (dir > 0 ? nx : -1); i += dir) {
      const o = i + nx * j + nxy * k; let best = D[o];
      for (let e = 0; e < list.length; e++) { const n = list[e], ii = i + n[0], jj = j + n[1], kk = k + n[2]; if (ii < 0 || jj < 0 || kk < 0 || ii >= nx || jj >= ny || kk >= nz) continue; const c = D[ii + nx * jj + nxy * kk] + n[3]; if (c < best) best = c; }
      D[o] = best;
    }
  };
  pass(fwd, 1); pass(bwd, -1);
  const CL = 80, data = new Float32Array(N);
  for (let o = 0; o < N; o++) {
    let s;
    if (D[o] < wall) s = sg[o] || (out[o] ? 1 : -1); else s = out[o] ? 1 : -1;
    const d = Math.min(D[o], CL);
    data[o] = s * d;
  }
  return { x0: x0, y0: y0, z0: z0, g: g, nx: nx, ny: ny, nz: nz, data: data };
};
AT.vertexNormals = function (pos, tris, nV) {
  const n = new Float32Array(nV * 3);
  for (let t = 0; t < tris.length; t += 3) {
    const a = tris[t] * 3, b = tris[t + 1] * 3, c = tris[t + 2] * 3;
    const e1x = pos[b] - pos[a], e1y = pos[b + 1] - pos[a + 1], e1z = pos[b + 2] - pos[a + 2], e2x = pos[c] - pos[a], e2y = pos[c + 1] - pos[a + 1], e2z = pos[c + 2] - pos[a + 2];
    const fx = e1y * e2z - e1z * e2y, fy = e1z * e2x - e1x * e2z, fz = e1x * e2y - e1y * e2x;
    [a, b, c].forEach(function (k) { n[k] += fx; n[k + 1] += fy; n[k + 2] += fz; });
  }
  for (let v = 0; v < nV; v++) { const k = v * 3, L = Math.hypot(n[k], n[k + 1], n[k + 2]) || 1; n[k] /= L; n[k + 1] /= L; n[k + 2] /= L; }
  return n;
};
// Build the simulation/render avatar from tailor measurements
AT.buildAvatarMH = function (M, spec0, opts) {
  opts = opts || {};
  const preset = AT.AVATAR_PRESETS[spec0 && spec0.sex === 'male' ? 'male' : 'female'];
  const spec = Object.assign({}, preset, spec0 || {});
  const t0 = Date.now();
  const fit = AT.mhFit(M, spec);
  const pos = AT.mhCompose(M, fit, false);
  for (let i = 0; i < pos.length; i++) pos[i] *= fit.scale;
  // ground + centre
  let yr = AT.mhHeightRange(M, pos, null);
  for (let v = 0; v < M.nV; v++) pos[v * 3 + 1] -= yr[0];
  const measured = AT.mhMeasureAll(M, pos, null); // measured in fitted (natural) pose
  const pose = AT.mhPose(M, pos, spec.armAngle, spec.legSpread);
  yr = AT.mhHeightRange(M, pos, null);
  if (Math.abs(yr[0]) > 0.01) for (let v = 0; v < M.nV; v++) pos[v * 3 + 1] -= yr[0];
  const H = measured.height;
  const tris = new Uint32Array(M.bodyTris.length + M.eyeTris.length); tris.set(M.bodyTris); tris.set(M.eyeTris, M.bodyTris.length);
  const nrm = AT.vertexNormals(pos, tris, M.nV);
  // landmarks
  const J = function (n) { return AT.mhJoint(M, pos, n); };
  const loopY = function (key) { const l = M.measures[key]; let s = 0; l.forEach(function (v) { s += pos[v * 3 + 1]; }); return s / l.length; };
  const ys = { shoulder: pos[M.lm.shoulderL * 3 + 1], neck: loopY('measure-neck-circ'), chin: J('joint-head')[1] - 40, bust: loopY('measure-bust-circ'), under: loopY('measure-underbust-circ'), waist: loopY('measure-waist-circ'), hip: loopY('measure-hips-circ'), crotch: pos[M.lm.crotch * 3 + 1], knee: loopY('measure-knee-circ') };
  ys.high = (ys.waist + ys.hip) / 2;
  const joints = {};
  ['L', 'R'].forEach(function (S) {
    const s = S.toLowerCase(), Sh = J('joint-' + s + '-shoulder'), E = J('joint-' + s + '-elbow'), W = J('joint-' + s + '-hand'), Hj = J('joint-' + s + '-upper-leg'), K = J('joint-' + s + '-knee'), A = J('joint-' + s + '-ankle');
    joints[S] = { S: Sh, E: E, W: W, a: AT.norm([W[0] - Sh[0], W[1] - Sh[1], W[2] - Sh[2]]), uaR: measured.upperArm / (2 * Math.PI), Hj: Hj, K: K, A: A, la: AT.norm([A[0] - Hj[0], A[1] - Hj[1], A[2] - Hj[2]]), thR: measured.thigh / (2 * Math.PI) };
  });
  // torso slices (exclude arm & leg vertices by skin weight)
  const isTorso = function (v) { return M.w.armL[v] < 70 && M.w.armR[v] < 70 && M.w.legL[v] < 90 && M.w.legR[v] < 90; };
  const ty0 = ys.crotch - 40, ty1 = ys.neck + 10, tStep = 4, tN = Math.ceil((ty1 - ty0) / tStep) + 1;
  const A = new Float32Array(tN), Bz = new Float32Array(tN), C = new Float32Array(tN), zmin = new Float32Array(tN).fill(1e9), zmax = new Float32Array(tN).fill(-1e9);
  for (let v = 0; v < M.bodyV; v++) {
    if (!isTorso(v)) continue;
    const y = pos[v * 3 + 1]; if (y < ty0 - 6 || y > ty1 + 6) continue;
    const bi = Math.round((y - ty0) / tStep); if (bi < 0 || bi >= tN) continue;
    const x = Math.abs(pos[v * 3]), z = pos[v * 3 + 2];
    if (x > A[bi]) A[bi] = x; if (z < zmin[bi]) zmin[bi] = z; if (z > zmax[bi]) zmax[bi] = z;
  }
  for (let i = 0; i < tN; i++) { if (zmax[i] < zmin[i]) { const j = i > 0 ? i - 1 : i; A[i] = A[j] || 100; zmin[i] = zmin[j] < 1e8 ? zmin[j] : -80; zmax[i] = zmax[j] > -1e8 ? zmax[j] : 80; } Bz[i] = (zmax[i] - zmin[i]) / 2; C[i] = (zmax[i] + zmin[i]) / 2; }
  const smooth = function (arr) { const o = Float32Array.from(arr); for (let i = 1; i < arr.length - 1; i++) o[i] = (arr[i - 1] + 2 * arr[i] + arr[i + 1]) / 4; return o; };
  const TA = smooth(A), TB = smooth(Bz), TC = smooth(C);
  const tEval = function (y) { const f = AT.clamp((y - ty0) / tStep, 0, tN - 1.0001), i = Math.floor(f), u = f - i; return [TA[i] + (TA[i + 1] - TA[i]) * u, TB[i] + (TB[i + 1] - TB[i]) * u, TC[i] + (TC[i + 1] - TC[i]) * u]; };
  const yCap = ys.neck + 25;
  const shoulderY = function (x) {
    let best = -Infinity;
    for (let v = 0; v < M.bodyV; v++) { if (M.w.armL[v] > 150 || M.w.armR[v] > 150) continue; const px = Math.abs(pos[v * 3]); if (Math.abs(px - x) > 7) continue; const y = pos[v * 3 + 1]; if (y > yCap || y < ys.shoulder - 120) continue; if (y > best) best = y; }
    return best > -Infinity ? best : ys.shoulder;
  };
  const neckR = measured.neck / (2 * Math.PI);
  // tapes for display
  const tapes = [];
  AT.MEAS.forEach(function (def) {
    const P = function (g) { return [pos[g * 3], pos[g * 3 + 1], pos[g * 3 + 2]]; };
    if (def.type === 'circ') { const tp = AT.mhTape(M.measures[def.t].map(P)); tapes.push({ k: def.k, pts: tp.hull3, closed: true }); }
    else if (def.type === 'dist') { const l = M.measures[def.t]; tapes.push({ k: def.k, pts: [P(l[0]), P(l[1])] }); }
    else if (def.type === 'shoulder') tapes.push({ k: def.k, pts: [P(M.lm.shoulderL), P(M.lm.shoulderR)] });
    else if (def.type === 'arm') { const a = M.measures['measure-upperarm-length'], b = M.measures['measure-lowerarm-length']; tapes.push({ k: def.k, pts: [P(a[0]), P(a[1]), P(b[0]), P(b[1])] }); }
    else if (def.type === 'crotch') { const c = P(M.lm.crotch); tapes.push({ k: def.k, pts: [[c[0], 0, c[2]], c] }); }
    else if (def.k === 'height') { const hx = -260; tapes.push({ k: def.k, pts: [[hx, 0, 0], [hx, H, 0]] }); }
  });
  const av = { kind: 'makehuman', bodyV: M.bodyV, m: spec, H: H, torso: { y0: ty0, y1: ty1, step: tStep, n: tN, A: TA, B: TB, C: TC, eval: tEval }, joints: joints, ys: ys, neckR: neckR, shoulderY: shoulderY, measured: measured, fit: fit, pose: pose, tapes: tapes,
    mesh: { pos: pos, nor: nrm, idx: tris }, tFit: Date.now() - t0 };
  if (opts.sdf !== false) { const t1 = Date.now(); av.grid = AT.meshSDF(pos, tris, opts.g || 7.5, 90, nrm); av.tSDF = Date.now() - t1; }
  return av;
};
// drafting profile (same interface as AT.torsoProfile) taken from a built avatar
AT.profileFromAvatar = function (av) {
  const m = av.m, H = av.H;
  return { m: m, H: H, s: H / 1700, fem: m.sex !== 'male', torso: av.torso, shoulderY: av.shoulderY, neckR: av.neckR, shH: (av.measured.shoulder || m.shoulder) / 2, ys: av.ys,
    hip: AT.ellAxes(av.measured.hip, 0.76) };
};


/* ---------- fabrics ---------- */
AT.PHYS_KEYS = [
  ['stretchWeft', 'Stretch-Weft'], ['stretchWarp', 'Stretch-Warp'], ['shear', 'Shear (bias)'],
  ['bendWeft', 'Bending-Weft'], ['bendWarp', 'Bending-Warp'], ['buckling', 'Buckling ratio'],
  ['density', 'Density (g/m²)'], ['thickness', 'Thickness (mm)'], ['friction', 'Friction'], ['damping', 'Internal damping']
];
AT.FABRIC_PRESETS = {
  poplin: { name: 'Cotton Poplin', weave: 'plain', rough: 0.82, sheen: 0.1, phys: { stretchWeft: 72, stretchWarp: 76, shear: 60, bendWeft: 34, bendWarp: 36, buckling: 50, density: 120, thickness: 0.3, friction: 30, damping: 30 } },
  jersey: { name: 'Cotton Jersey', weave: 'jersey', rough: 0.9, sheen: 0.05, phys: { stretchWeft: 16, stretchWarp: 30, shear: 12, bendWeft: 12, bendWarp: 14, buckling: 70, density: 165, thickness: 0.55, friction: 35, damping: 30 } },
  rib: { name: 'Rib Knit 1×1', weave: 'rib', rough: 0.9, sheen: 0.05, phys: { stretchWeft: 6, stretchWarp: 40, shear: 8, bendWeft: 14, bendWarp: 16, buckling: 70, density: 230, thickness: 0.8, friction: 40, damping: 30 } },
  denim: { name: 'Denim 12 oz', weave: 'denim', rough: 0.88, sheen: 0.05, phys: { stretchWeft: 80, stretchWarp: 84, shear: 70, bendWeft: 62, bendWarp: 66, buckling: 40, density: 400, thickness: 0.9, friction: 40, damping: 35 } },
  silk: { name: 'Silk Charmeuse', weave: 'satin', rough: 0.35, sheen: 0.8, phys: { stretchWeft: 60, stretchWarp: 64, shear: 28, bendWeft: 4, bendWarp: 5, buckling: 60, density: 80, thickness: 0.2, friction: 15, damping: 20 } },
  chiffon: { name: 'Polyester Chiffon', weave: 'plain', rough: 0.6, sheen: 0.3, opacity: 0.82, phys: { stretchWeft: 55, stretchWarp: 58, shear: 25, bendWeft: 2, bendWarp: 2, buckling: 60, density: 45, thickness: 0.15, friction: 15, damping: 20 } },
  wool: { name: 'Wool Suiting', weave: 'twill', rough: 0.85, sheen: 0.1, phys: { stretchWeft: 70, stretchWarp: 72, shear: 55, bendWeft: 42, bendWarp: 44, buckling: 45, density: 260, thickness: 0.6, friction: 35, damping: 35 } },
  linen: { name: 'Linen', weave: 'plain', rough: 0.92, sheen: 0.05, phys: { stretchWeft: 76, stretchWarp: 78, shear: 66, bendWeft: 46, bendWarp: 48, buckling: 40, density: 180, thickness: 0.4, friction: 40, damping: 30 } },
  satin: { name: 'Duchess Satin', weave: 'satin', rough: 0.3, sheen: 0.9, phys: { stretchWeft: 66, stretchWarp: 70, shear: 40, bendWeft: 30, bendWarp: 32, buckling: 45, density: 200, thickness: 0.35, friction: 15, damping: 25 } },
  leather: { name: 'Lamb Leather', weave: 'leather', rough: 0.55, sheen: 0.4, phys: { stretchWeft: 86, stretchWarp: 86, shear: 80, bendWeft: 74, bendWarp: 74, buckling: 30, density: 700, thickness: 1.0, friction: 45, damping: 40 } },
  scuba: { name: 'Scuba Neoprene', weave: 'none', rough: 0.7, sheen: 0.15, phys: { stretchWeft: 30, stretchWarp: 34, shear: 30, bendWeft: 58, bendWarp: 58, buckling: 30, density: 300, thickness: 2.0, friction: 40, damping: 40 } },
  fleece: { name: 'Fleece', weave: 'fleece', rough: 0.98, sheen: 0, phys: { stretchWeft: 25, stretchWarp: 35, shear: 22, bendWeft: 30, bendWarp: 30, buckling: 50, density: 280, thickness: 2.5, friction: 50, damping: 40 } },
  corduroy: { name: 'Corduroy', weave: 'corduroy', rough: 0.9, sheen: 0.1, phys: { stretchWeft: 74, stretchWarp: 78, shear: 60, bendWeft: 52, bendWarp: 56, buckling: 40, density: 320, thickness: 1.1, friction: 40, damping: 35 } }
};
AT.makeFabric = function (presetKey, over) {
  const p = AT.FABRIC_PRESETS[presetKey] || AT.FABRIC_PRESETS.poplin;
  return Object.assign({ id: AT.uid('f'), name: p.name, preset: presetKey, color: '#d9d6cf', color2: '#ffffff', weave: p.weave, scale: 6, rough: p.rough, sheen: p.sheen, opacity: p.opacity || 1, phys: AT.clone(p.phys) }, over || {});
};
// 2D tensile modulus in N/m from the 0-100 stretch scale; compliance in s²/g (mm, g, s units)
AT.E2D = function (k) { return Math.pow(10, 0.9 + 3.0 * AT.clamp(k, 0, 100) / 100); };
AT.rStretch = function (k) { return 1 / (0.87 * AT.E2D(k) * 1000); };
AT.rBend = function (k) { return Math.pow(10, 2.8 - 3.8 * AT.clamp(k, 0, 100) / 100); };
AT.stressKPa = function (k, strain, thicknessMM) { return AT.E2D(k) * strain / Math.max(0.1, thicknessMM || 0.5) ; };
AT.limitStrain = function (k) { const q = 1 - AT.clamp(k, 0, 100) / 100; return 0.02 + q * q * 0.5; };


/* ---------- simulation settings: presets + per-project overrides (doc.sim = {preset, over}) ---------- */
AT.SIM_DEFAULTS = {
  pdScale: 1,          // multiplies every piece's particle distance (mesh density); < 1 = finer
  dt: 1 / 60,          // simulated seconds per frame
  substeps: 10,        // solver substeps per frame
  iterations: 3,       // stretch + sewing constraint iterations per substep
  framesPerStep: 1,    // simulated frames per screen frame (speeds up draping)
  selfCollision: true, // cloth-cloth collision (within and between pieces / layers)
  ccEveryGPU: 2,       // re-detect cloth contacts every N substeps
  ccIters: 2,          // contact resolution passes per substep
  clothThickness: 2.5, // mm kept between cloth surfaces
  ccFric: 2,           // cloth-on-cloth friction, times the fabric friction
  ccRepair: true,      // pull layers that crossed back to their own side
  untangle: true,      // find cloth that has passed through cloth and push it back (background worker)
  fricModel: 'grip',   // cloth-on-cloth friction while touching: 'grip' (holds, x0.8 cap) or 'slide' (Coulomb: layers slide under load)
  gravity: 1,          // in g
  air: 0.3,            // air damping
  sewSpeed: 1600,      // mm/s at which open seams are pulled closed
  grabSpeed: 700,      // mm/s: fastest the grab point follows the mouse
  autoStop: false,     // stop a run once the garment has settled
  settle: 14,          // settle threshold (motion energy)
  maxTime: 20          // stop after this many simulated seconds (auto-stop runs)
};
// presets measured on the robe + aba project on Apple M-series WebGPU (8–12 s simulated): see the skill doc
AT.SIM_PRESETS = {
  draft: { name: 'Draft', desc: 'Coarser mesh, two simulated frames per redraw. About twice as fast as Normal; for blocking out fit while you edit.', v: { pdScale: 1.3, substeps: 8, iterations: 2, framesPerStep: 2 } },
  normal: { name: 'Normal', desc: 'Balanced default for working on a garment.', v: {} },
  fine: { name: 'Fine', desc: 'Finer mesh and more solver work: smaller wrinkles, tighter seams. About 1.5× the cost of Normal.', v: { pdScale: 0.8, substeps: 14, iterations: 4, air: 0.5 } },
  layered: { name: 'Layered', desc: 'More substeps and damping, and sliding friction between cloth, for garments worn over each other (robe + aba, coat over shirt): far fewer cloth pass-throughs.', v: { substeps: 16, air: 0.6, fricModel: 'slide' } },
  final: { name: 'Final', desc: 'Finest mesh and most solver work, for presentation and fit sign-off. About 3× the cost of Normal; stops by itself once settled.', v: { pdScale: 0.65, substeps: 20, iterations: 5, air: 0.8, autoStop: true, maxTime: 30 } }
};
// settle detection for auto-stopping runs: below the threshold for a while, or motion has stopped decreasing
AT.settleTracker = function () {
  const T = { s: [], n: 0 };
  T.push = function (t, E, gap, thr) {
    const s = T.s;
    if (!s.length || t - s[s.length - 1][0] >= 0.25) s.push([t, E]);
    while (s.length && t - s[0][0] > 4.25) s.shift();
    if (gap >= 2.5) { T.n = 0; return false; }
    if (t > 1 && E < thr) { if (++T.n > 40) return 'threshold'; } else T.n = 0;
    if (t >= 4) {
      let a = 0, na = 0, b = 0, nb = 0;
      s.forEach(function (e) { if (t - e[0] <= 2) { a += e[1]; na++; } else { b += e[1]; nb++; } });
      if (na >= 4 && nb >= 4 && a / na < thr * 8 && a / na > 0.9 * (b / nb)) return 'plateau';
    }
    return false;
  };
  return T;
};
AT.SIM_LIMITS = {
  pdScale: [0.4, 2.5], dt: [1 / 240, 1 / 24], substeps: [2, 40], iterations: [1, 12], framesPerStep: [1, 6], ccEveryGPU: [1, 6], ccIters: [1, 5],
  clothThickness: [0.5, 8], ccFric: [0, 5], gravity: [0, 2], air: [0, 3], sewSpeed: [200, 6000], grabSpeed: [100, 3000], settle: [2, 60], maxTime: [2, 120]
};
AT.simSettings = function (doc) {
  const s = (doc && doc.sim) || {}, legacy = { fast: 'draft', complete: 'fine' };
  const preset = AT.SIM_PRESETS[s.preset] ? s.preset : (legacy[s.quality] || (AT.SIM_PRESETS[s.quality] ? s.quality : 'normal'));
  const base = Object.assign({}, AT.SIM_DEFAULTS, AT.SIM_PRESETS[preset].v);
  const v = Object.assign({}, base), over = {};
  // fields written by older versions count as overrides
  if (s.air != null && s.air !== base.air) over.air = s.air;
  if (s.g != null && s.g !== base.gravity) over.gravity = s.g;
  if (s.selfCollision === false) over.selfCollision = false;
  Object.keys(s.over || {}).forEach(function (k) { if (k in AT.SIM_DEFAULTS && s.over[k] != null && s.over[k] !== base[k]) over[k] = s.over[k]; });
  Object.keys(over).forEach(function (k) { const L = AT.SIM_LIMITS[k]; v[k] = L && typeof over[k] === 'number' ? AT.clamp(over[k], L[0], L[1]) : over[k]; });
  return { preset: preset, v: v, base: base, over: over, modified: Object.keys(over).length > 0 };
};
// solver params for AT.buildSim from resolved settings
AT.simParams = function (st) {
  const v = st.v;
  return { pdScale: v.pdScale, substeps: Math.round(v.substeps), iterations: Math.round(v.iterations), framesPerStep: Math.round(v.framesPerStep), gravity: 9810 * v.gravity, air: v.air, sewSpeed: v.sewSpeed, grabSpeed: v.grabSpeed,
    selfCollision: v.selfCollision !== false, ccEveryGPU: Math.round(v.ccEveryGPU), ccIters: Math.round(v.ccIters), clothThickness: v.clothThickness, ccFric: v.ccFric, ccRepair: v.ccRepair !== false,
    untangle: v.untangle !== false, ccFricMode: v.fricModel === 'slide' ? 0 : 1, ccMuCap: v.fricModel === 'slide' ? 1.5 : 0.8 };
};
// grabbed cloth follows a grab point that chases the mouse at a bounded speed: a fast flick cannot push cloth
// through other cloth faster than contacts can stop it (the grip itself stays as firm as before)
// fraction of this substep's tangential slip removed by a touching contact (same model as the GPU resolve pass)
AT.fricK = function (P, mu0, dn, tl) {
  const slide = P && P.ccFricMode === 0, mu = Math.min(P && P.ccMuCap != null ? P.ccMuCap : 0.8, mu0 * (P && P.ccFric != null ? P.ccFric : 2));
  if (!slide) return mu;
  return tl > mu * dn ? Math.min(1, 0.8 * mu * dn / tl) : 1;
};
AT.grabAnchor = function (sim, dt) {
  const gb = sim.grab; if (!gb) return null;
  if (!gb.cur) gb.cur = (gb.start || gb.target).slice();
  const v = (sim.params && sim.params.grabSpeed) || 700, mx = v * dt;
  const dx = gb.target[0] - gb.cur[0], dy = gb.target[1] - gb.cur[1], dz = gb.target[2] - gb.cur[2], L = Math.hypot(dx, dy, dz);
  if (L <= mx) gb.cur = gb.target.slice(); else { const f = mx / L; gb.cur = [gb.cur[0] + dx * f, gb.cur[1] + dy * f, gb.cur[2] + dz * f]; }
  return gb.cur;
};
AT.piecePd = function (pc, pdScale) { return Math.max(3, (pc.pd || 20) * (pdScale || 1)); };
// rough particle count for a mesh density (for the settings panel)
AT.estimateParticles = function (doc, pdScale) {
  let n = 0;
  AT.asArray(doc.pieces).forEach(function (pc) { if (!pc || !pc.pts || pc.pts.length < 3 || pc.state === 'off') return; const a = Math.abs(AT.area(AT.outline(pc.pts).poly)), d = AT.piecePd(pc, pdScale); n += a / (d * d * 0.866) + AT.outline(pc.pts).total / d; });
  return Math.round(n);
};
/* ---------- cloth simulation ---------- */
AT.pieceHash = function (doc, pc, pdScale) {
  const pts = AT.gradedPts(pc, doc.activeSize, doc.baseSize);
  return JSON.stringify([pts.map(function (p) { return [Math.round(p.x * 100), Math.round(p.y * 100), p.c ? 1 : 0, p.id]; }), (pc.internals || []).map(function (il) { return [il.id, il.fold, (il.pts || []).map(function (p) { return [Math.round(p.x), Math.round(p.y), p.c ? 1 : 0]; })]; }), AT.piecePd(pc, pdScale), pc.grain || 0]);
};
AT.asArray = function (v) { if (!v) return []; if (Array.isArray(v)) return v.filter(function (x) { return x != null; }); return Object.keys(v).sort().map(function (k) { return v[k]; }); };
AT.buildSim = function (doc, av, prev, opts) {
  opts = opts || {};
  const pieces = AT.asArray(doc.pieces).filter(function (p) { return p && p.pts && p.pts.length >= 3; });
  pieces.sort(function (a, b) { return (a.order || 0) - (b.order || 0) || (a.id < b.id ? -1 : 1); });
  const recs = [];
  let N = 0;
  const prevMap = {};
  if (prev) prev.recs.forEach(function (r) { prevMap[r.id] = r; });
  pieces.forEach(function (pc) {
    const hash = AT.pieceHash(doc, pc, opts.params && opts.params.pdScale);
    const pr = prevMap[pc.id];
    let mesh;
    if (pr && pr.hash === hash) mesh = pr.mesh;
    else {
      const pts = AT.gradedPts(pc, doc.activeSize, doc.baseSize);
      try { mesh = AT.meshPiece(pts, AT.asArray(pc.internals), AT.piecePd(pc, opts.params && opts.params.pdScale), pc.grain || 0); } catch (e) { mesh = null; }
      if (!mesh || mesh.tris.length < 3) return;
      mesh.topo = AT.topology(mesh);
    }
    const gp = AT.gradedPts(pc, doc.activeSize, doc.baseSize).map(function (q) { return { id: q.id, x: q.x, y: q.y }; });
    recs.push({ id: pc.id, pc: pc, mesh: mesh, hash: hash, off: N, n: mesh.n, gpts: gp });
    N += mesh.n;
  });
  const x = new Float32Array(N * 3), p = new Float32Array(N * 3), v = new Float32Array(N * 3), w = new Float32Array(N), w0 = new Float32Array(N);
  const damp = new Float32Array(N), margin = new Float32Array(N), pieceOf = new Int32Array(N), active = new Uint8Array(N), rad = new Float32Array(N);
  const fric = new Float32Array(N), layerOf = new Int8Array(N), pdOf = new Float32Array(N), thickOf = new Float32Array(N);
  recs.forEach(function (r, ri) {
    const pr = prevMap[r.id];
    if (pr && pr.hash === r.hash && prev) {
      x.set(prev.x.subarray(pr.off * 3, (pr.off + pr.n) * 3), r.off * 3);
    } else if (pr && prev && !opts.rearrange) {
      // remap old drape through barycentric coordinates in 2D (rigid-motion aware)
      const rm = AT.rigidMap(pr.gpts || [], r.gpts);
      for (let i = 0; i < r.n; i++) {
        const q2 = rm ? rm(r.mesh.xy[2 * i], r.mesh.xy[2 * i + 1]) : [r.mesh.xy[2 * i], r.mesh.xy[2 * i + 1]];
        const L = AT.locate(pr.mesh, q2[0], q2[1]);
        const k = (r.off + i) * 3;
        if (!L) { continue; }
        for (let c = 0; c < 3; c++) x[k + c] = L.l1 * prev.x[(pr.off + L.a) * 3 + c] + L.l2 * prev.x[(pr.off + L.b) * 3 + c] + L.l3 * prev.x[(pr.off + L.c) * 3 + c];
      }
    } else {
      AT.arrangePositions(av, r.pc, r.mesh, x, r.off);
    }
  });
  if (opts.rearrangeIds) recs.forEach(function (r) { if (opts.rearrangeIds.indexOf(r.id) >= 0) AT.arrangePositions(av, r.pc, r.mesh, x, r.off); });
  p.set(x);
  const fabrics = doc.fabrics || {};
  const defFab = AT.makeFabric('poplin');
  const S = { i: [], j: [], L: [], r: [], cm: [] }, Bd = { i: [], j: [], L: [], r: [] };
  recs.forEach(function (r, ri) {
    const pc = r.pc, fab = fabrics[pc.fabric] || defFab, ph = Object.assign({}, defFab.phys, fab.phys || {});
    const state = pc.state || 'active';
    const strong = pc.strengthen ? 0.02 : 1;
    const g = (pc.grain || 0) * AT.deg, wx = Math.cos(g), wy = Math.sin(g);
    const rW = AT.rStretch(ph.stretchWeft) * strong, rP = AT.rStretch(ph.stretchWarp) * strong, rS = AT.rStretch(ph.shear) * strong;
    const bW = AT.rBend(ph.bendWeft) * strong, bP = AT.rBend(ph.bendWarp) * strong;
    const shW = (pc.shrinkWeft || 100) / 100, shP = (pc.shrinkWarp || 100) / 100;
    const compMul = 1 + (ph.buckling == null ? 50 : ph.buckling) / 100 * 24;
    const dens = (ph.density || 150) * 1e-6;
    const thick = ph.thickness || 0.5;
    const layer = pc.layer || 0;
    const M = r.mesh, xy = M.xy, off = r.off;
    // masses
    const mass = new Float32Array(M.n);
    for (let t = 0; t < M.tris.length; t += 3) {
      const a = M.tris[t], b = M.tris[t + 1], c = M.tris[t + 2];
      const A = Math.abs((xy[2 * b] - xy[2 * a]) * (xy[2 * c + 1] - xy[2 * a + 1]) - (xy[2 * b + 1] - xy[2 * a + 1]) * (xy[2 * c] - xy[2 * a])) / 2;
      mass[a] += A * dens / 3; mass[b] += A * dens / 3; mass[c] += A * dens / 3;
    }
    const isAct = state !== 'off';
    for (let i = 0; i < M.n; i++) {
      const q = off + i;
      const wi = (state === 'active' && mass[i] > 0) ? 1 / mass[i] : 0;
      w[q] = wi; w0[q] = wi;
      damp[q] = 0.4 + (ph.damping == null ? 30 : ph.damping) / 100 * 3;
      margin[q] = 3 + thick * 0.5 + layer * (thick + 3);
      pieceOf[q] = ri; active[q] = isAct ? 1 : 0;
      rad[q] = AT.clamp(M.pd * 0.42, 3, 10);
      layerOf[q] = layer; pdOf[q] = M.pd; thickOf[q] = thick;
      fric[q] = AT.clamp((ph.friction == null ? 30 : ph.friction) / 100, 0, 0.9);
    }
    if (!isAct) return;
    // elastic segments
    const elastic = pc.elastic || {};
    const segElastic = M.ol.segs.map(function (sg) { const e = elastic[sg.id]; return e ? e / 100 : 1; });
    const bIndex = new Int32Array(M.n).fill(-1);
    for (let i = 0; i < M.nB; i++) bIndex[M.bnd[i]] = i;
    // stretch
    M.topo.edges.forEach(function (E) {
      const a = E.a, b = E.b;
      const dx = xy[2 * b] - xy[2 * a], dy = xy[2 * b + 1] - xy[2 * a + 1], L = Math.hypot(dx, dy) || 1;
      const c = Math.abs(dx * wx + dy * wy) / L, s = Math.abs(-dx * wy + dy * wx) / L, c2 = c * c, s2 = s * s;
      let rr = rW * c2 * c2 + rP * s2 * s2 + rS * 2 * c2 * s2;
      let rest = L * (shW * c2 + shP * s2);
      const ia = bIndex[a], ib = bIndex[b];
      if (ia >= 0 && ib >= 0 && (Math.abs(ia - ib) === 1 || Math.abs(ia - ib) === M.nB - 1)) {
        const k = Math.abs(ia - ib) === 1 ? M.bk[Math.min(ia, ib)] : M.bk[Math.max(ia, ib)];
        if (segElastic[k] !== 1) { rest *= segElastic[k]; rr *= 4; }
      }
      S.i.push(off + a); S.j.push(off + b); S.L.push(rest); S.r.push(rr); S.cm.push(compMul);
      if (E.o2 >= 0) {
        const o1 = E.o1, o2 = E.o2;
        const ex = xy[2 * o2] - xy[2 * o1], ey = xy[2 * o2 + 1] - xy[2 * o1 + 1], EL = Math.hypot(ex, ey) || 1;
        const cc = Math.abs(ex * wx + ey * wy) / EL, ss = Math.abs(-ex * wy + ey * wx) / EL;
        Bd.i.push(off + o1); Bd.j.push(off + o2); Bd.L.push(EL * (shW * cc * cc + shP * ss * ss)); Bd.r.push(bW * cc * cc + bP * ss * ss);
        E.bendIndex = Bd.i.length - 1;
      }
    });
    // fold lines
    M.internals.forEach(function (il) {
      if (il.fold == null || il.fold === '' || Math.abs(il.fold - 180) < 0.5) return;
      const phi = AT.clamp(il.fold, 0, 360) * AT.deg;
      for (let q = 0; q < il.verts.length - 1; q++) {
        const a = Math.min(il.verts[q], il.verts[q + 1]), b = Math.max(il.verts[q], il.verts[q + 1]);
        const E = M.topo.map.get(a * M.n + b);
        if (!E || E.o2 < 0 || E.bendIndex == null) continue;
        const ax = xy[2 * a], ay = xy[2 * a + 1], bx = xy[2 * b], by = xy[2 * b + 1];
        const hx = bx - ax, hy = by - ay, HL = Math.hypot(hx, hy) || 1, ux = hx / HL, uy = hy / HL;
        const pr = function (o) { const qx = xy[2 * o] - ax, qy = xy[2 * o + 1] - ay; return [qx * ux + qy * uy, Math.abs(-qx * uy + qy * ux)]; };
        const P3 = pr(E.o1), P4 = pr(E.o2);
        const phiIn = phi > Math.PI ? 2 * Math.PI - phi : phi;
        const d2 = (P3[0] - P4[0]) * (P3[0] - P4[0]) + P3[1] * P3[1] + P4[1] * P4[1] - 2 * P3[1] * P4[1] * Math.cos(phiIn);
        Bd.L[E.bendIndex] = Math.sqrt(Math.max(1, d2));
        Bd.r[E.bendIndex] = -0.01; // negative = bilateral fold constraint
      }
    });
  });
  // sewing
  const SW = { a: [], b0: [], b1: [], u: [], s: [] };
  const recById = {};
  recs.forEach(function (r) { recById[r.id] = r; });
  const sideVerts = function (side) {
    const r = recById[side.p];
    if (!r || (r.pc.state === 'off')) return null;
    const M = r.mesh, ol = M.ol, R = AT.sideRange(ol, side);
    if (!R) return null;
    const out = [];
    for (let i = 0; i < M.nB; i++) {
      const P = ol.start[M.bk[i]] + ol.segs[M.bk[i]].len * M.bt[i];
      let o = R.dir > 0 ? P - R.Pa : R.Pa - P;
      o = ((o % R.T) + R.T) % R.T;
      if (o > R.T - 0.5) o = 0;
      if (o <= R.L + 0.5) out.push({ v: r.off + M.bnd[i], u: Math.min(o, R.L), len: R.L });
    }
    out.sort(function (a, b) { return a.u - b.u; });
    return { list: out, L: R.L };
  };
  const combine = function (sides) {
    const parts = [];
    let total = 0;
    AT.asArray(sides).forEach(function (sd) { const r = sideVerts(sd); if (r && r.list.length) { parts.push(r); total += r.L; } });
    if (!parts.length || total <= 0) return [];
    const all = [];
    let acc = 0;
    parts.forEach(function (pt) { pt.list.forEach(function (e) { all.push({ v: e.v, u: (acc + e.u) / total }); }); acc += pt.L; });
    return all;
  };
  let seamIdx = 0;
  const pairUp = function (A, B) {
    if (B.length < 2) return;
    let j = 0;
    A.forEach(function (a) {
      while (j < B.length - 2 && B[j + 1].u < a.u) j++;
      const b0 = B[j], b1 = B[j + 1];
      const du = b1.u - b0.u;
      let u = du > 1e-9 ? AT.clamp((a.u - b0.u) / du, 0, 1) : 0;
      if (b0.v === a.v || b1.v === a.v) return;
      SW.a.push(a.v); SW.b0.push(b0.v); SW.b1.push(b1.v); SW.u.push(u); SW.s.push(seamIdx);
    });
  };
  const sewn = new Uint8Array(N);
  const sewList = AT.asArray(doc.sewing);
  sewList.forEach(function (sw) {
    if (!sw) return;
    const A = combine(sw.a), B = combine(sw.b);
    seamIdx = sewList.indexOf(sw);
    if (A.length < 1 || B.length < 1) return;
    pairUp(A, B); pairUp(B, A);
  });
  // fasteners (buttons -> buttonholes)
  const FA = { a: [], b: [] };
  const nearestVert = function (pid, x2, y2) {
    const r = recById[pid]; if (!r) return -1;
    const L = AT.locate(r.mesh, x2, y2); if (!L) return -1;
    const m = Math.max(L.l1, L.l2, L.l3);
    return r.off + (m === L.l1 ? L.a : m === L.l2 ? L.b : L.c);
  };
  const buttons = doc.buttons || {}, holes = doc.holes || {};
  Object.keys(buttons).forEach(function (k) {
    const bt = buttons[k]; if (!bt || !bt.fasten || !holes[bt.fasten]) return;
    const h = holes[bt.fasten];
    const a = nearestVert(bt.p, bt.x, bt.y), b = nearestVert(h.p, h.x, h.y);
    if (a >= 0 && b >= 0) { FA.a.push(a); FA.b.push(b); }
  });
  // pins
  const pinned = [];
  recs.forEach(function (r) {
    AT.asArray(r.pc.pins).forEach(function (pn) {
      const q = nearestVert(r.id, pn.x, pn.y);
      if (q >= 0) pinned.push(q);
    });
  });
  const sim = {
    recs: recs, N: N, x: x, p: p, v: v, w: w, w0: w0, damp: damp, margin: margin, pieceOf: pieceOf, active: active, rad: rad, fric: fric, layerOf: layerOf, pdOf: pdOf, thickOf: thickOf,
    S: { n: S.i.length, i: Int32Array.from(S.i), j: Int32Array.from(S.j), L: Float32Array.from(S.L), r: Float32Array.from(S.r), cm: Float32Array.from(S.cm) },
    B: { n: Bd.i.length, i: Int32Array.from(Bd.i), j: Int32Array.from(Bd.j), L: Float32Array.from(Bd.L), r: Float32Array.from(Bd.r) },
    SW: { n: SW.a.length, a: Int32Array.from(SW.a), b0: Int32Array.from(SW.b0), b1: Int32Array.from(SW.b1), u: Float32Array.from(SW.u), s: Int32Array.from(SW.s), list: sewList },
    FA: { n: FA.a.length, a: Int32Array.from(FA.a), b: Int32Array.from(FA.b) },
    pins: Int32Array.from(pinned), pinPos: new Float32Array(pinned.length * 3), sewn: sewn,
    av: av, params: Object.assign({ substeps: 10, gravity: 9810, air: 0.3, sewSpeed: 1600, selfCollision: true, iterations: 3 }, opts.params || {}),
    grab: null, time: 0, energy: 0, frames: 0, nrm: new Float32Array(N * 3), pressure: new Float32Array(N), good: new Float32Array(x)
  };
  for (let c = 0; c < sim.SW.n; c++) { sewn[sim.SW.a[c]] = 1; sewn[sim.SW.b0[c]] = 1; sewn[sim.SW.b1[c]] = 1; }
  for (let i = 0; i < pinned.length; i++) { const q = pinned[i]; sim.pinPos[i * 3] = x[q * 3]; sim.pinPos[i * 3 + 1] = x[q * 3 + 1]; sim.pinPos[i * 3 + 2] = x[q * 3 + 2]; w[q] = 0; }
  recs.forEach(function (r, ri) { const pr = r.pc.pressure || 0; if (pr) for (let i = 0; i < r.n; i++) sim.pressure[r.off + i] = pr; });
  if (opts.projectOut !== false) AT.projectOut(sim);
  AT.prepBodyTri(sim);
  return sim;
};
AT.projectOut = function (sim) {
  const G = sim.av.grid, g = [0, 0, 0], x = sim.x;
  for (let q = 0; q < sim.N; q++) {
    if (!sim.active[q]) continue;
    const k = q * 3;
    for (let it = 0; it < 4; it++) {
      const d = AT.gridSample(G, x[k], x[k + 1], x[k + 2], g);
      const m = sim.margin[q];
      if (d >= m) break;
      const L = Math.hypot(g[0], g[1], g[2]) || 1;
      const push = (m - d) + 0.5;
      x[k] += g[0] / L * push; x[k + 1] += g[1] / L * push; x[k + 2] += g[2] / L * push;
    }
  }
  sim.p.set(x);
};
AT.computeNormals = function (sim) {
  const nrm = sim.nrm, x = sim.x; nrm.fill(0);
  sim.recs.forEach(function (r) {
    const T = r.mesh.tris, o = r.off;
    for (let t = 0; t < T.length; t += 3) {
      const a = (o + T[t]) * 3, b = (o + T[t + 1]) * 3, c = (o + T[t + 2]) * 3;
      const e1x = x[b] - x[a], e1y = x[b + 1] - x[a + 1], e1z = x[b + 2] - x[a + 2];
      const e2x = x[c] - x[a], e2y = x[c + 1] - x[a + 1], e2z = x[c + 2] - x[a + 2];
      const nx = e1y * e2z - e1z * e2y, ny = e1z * e2x - e1x * e2z, nz = e1x * e2y - e1y * e2x;
      nrm[a] += nx; nrm[a + 1] += ny; nrm[a + 2] += nz; nrm[b] += nx; nrm[b + 1] += ny; nrm[b + 2] += nz; nrm[c] += nx; nrm[c + 1] += ny; nrm[c + 2] += nz;
    }
  });
  for (let q = 0; q < sim.N; q++) { const k = q * 3, L = Math.hypot(nrm[k], nrm[k + 1], nrm[k + 2]) || 1; nrm[k] /= L; nrm[k + 1] /= L; nrm[k + 2] /= L; }
};
/* ---------- cloth-cloth collision: self-collision and collision between pieces ----------
   Vertex-triangle proximity contacts. Every particle is kept a collision thickness away from every cloth
   triangle that is not its neighbour (triangles of its own piece included, so a piece cannot pass through
   itself when it folds). Same layer: the particle keeps the side it came from. Different layers: the side is
   fixed by layer order (the higher layer stays outside along the body normal), which also repairs layers that
   have crossed. Contacts are detected every few substeps into a list (AT.selfCollide) and resolved with
   friction on every substep (AT.contactPass). */
AT.prepClothCollide = function (sim) {
  const recs = sim.recs, N = sim.N, po = sim.pieceOf, SW = sim.SW;
  const T = [], tp = [];
  let eSum = 0, eN = 0;
  recs.forEach(function (r, ri) {
    if (r.pc.state === 'off') return;
    const tr = r.mesh.tris, xy = r.mesh.xy;
    for (let i = 0; i < tr.length; i += 3) {
      T.push(tr[i] + r.off, tr[i + 1] + r.off, tr[i + 2] + r.off); tp.push(ri);
      eSum += Math.hypot(xy[2 * tr[i]] - xy[2 * tr[i + 1]], xy[2 * tr[i] + 1] - xy[2 * tr[i + 1] + 1]); eN++;
    }
  });
  // which pieces each particle is sewn to (bit per piece, folded to 32): contacts across its own seam are skipped
  const pm = new Uint32Array(N);
  for (let c = 0; c < SW.n; c++) {
    const qa = SW.a[c], q0 = SW.b0[c], q1 = SW.b1[c], ba = 1 << (po[qa] & 31), bb = 1 << (po[q0] & 31);
    pm[qa] |= bb | ba; pm[q0] |= ba | bb; pm[q1] |= ba | bb;
  }
  const nt = tp.length, cap = Math.max(1024, nt * 12);
  sim.cc = { T: Int32Array.from(T), tp: Int32Array.from(tp), nt: nt, pm: pm, edge: eN ? eSum / eN : 20, HS: 1 << 17,
    head: new Int32Array(1 << 17), next: new Int32Array(cap), item: new Int32Array(cap),
    cq: new Int32Array(4096), ct: new Int32Array(4096), cs: new Int8Array(4096), n: 0, g: [0, 0, 0] };
};
AT.selfCollide = function (sim) {
  if (!sim.cc) AT.prepClothCollide(sim);
  const cc = sim.cc, x = sim.x, p = sim.p, w = sim.w, act = sim.active, T = cc.T, nt = cc.nt, tp = cc.tp, pm = cc.pm;
  const recs = sim.recs, po = sim.pieceOf, lay = sim.layerOf, sewn = sim.sewn, th = sim.thickOf, P = sim.params;
  const H0 = P.clothThickness == null ? 2.5 : P.clothThickness;
  const R = H0 + 2 + (P.ccSlack == null ? 10 : P.ccSlack);
  const C = AT.clamp(cc.edge * 1.1, 10, 60), HS = cc.HS, mask = HS - 1, head = cc.head;
  const hk = function (i, j, k) { return ((i * 73856093) ^ (j * 19349663) ^ (k * 83492791)) & mask; };
  head.fill(-1);
  let cnt = 0;
  for (let t = 0; t < nt; t++) {
    const ia = T[t * 3], ib = T[t * 3 + 1], ic = T[t * 3 + 2], a = ia * 3, b = ib * 3, c = ic * 3;
    const i0 = Math.floor((Math.min(x[a], x[b], x[c]) - R) / C), i1 = Math.floor((Math.max(x[a], x[b], x[c]) + R) / C);
    const j0 = Math.floor((Math.min(x[a + 1], x[b + 1], x[c + 1]) - R) / C), j1 = Math.floor((Math.max(x[a + 1], x[b + 1], x[c + 1]) + R) / C);
    const k0 = Math.floor((Math.min(x[a + 2], x[b + 2], x[c + 2]) - R) / C), k1 = Math.floor((Math.max(x[a + 2], x[b + 2], x[c + 2]) + R) / C);
    if ((i1 - i0 + 1) * (j1 - j0 + 1) * (k1 - k0 + 1) > 64) continue; // degenerate / exploded triangle
    for (let i = i0; i <= i1; i++) for (let j = j0; j <= j1; j++) for (let k = k0; k <= k1; k++) {
      if (cnt >= cc.item.length) { const ni = new Int32Array(cc.item.length * 2), nn = new Int32Array(cc.item.length * 2); ni.set(cc.item); nn.set(cc.next); cc.item = ni; cc.next = nn; }
      const h = hk(i, j, k); cc.item[cnt] = t; cc.next[cnt] = head[h]; head[h] = cnt; cnt++;
    }
  }
  const item = cc.item, next = cc.next, g = cc.g, grid = sim.av.grid;
  let n = 0;
  const push = function (q, t, sd) {
    if (n >= cc.cq.length) { const L = cc.cq.length * 2, a1 = new Int32Array(L), a2 = new Int32Array(L), a3 = new Int8Array(L); a1.set(cc.cq); a2.set(cc.ct); a3.set(cc.cs); cc.cq = a1; cc.ct = a2; cc.cs = a3; }
    cc.cq[n] = q; cc.ct[n] = t; cc.cs[n] = sd; n++;
  };
  for (let q = 0; q < sim.N; q++) {
    if (!act[q]) continue;
    const kq = q * 3, qx = x[kq], qy = x[kq + 1], qz = x[kq + 2];
    let e = head[hk(Math.floor(qx / C), Math.floor(qy / C), Math.floor(qz / C))];
    if (e < 0) continue;
    const pq = po[q], rq = recs[pq], xyq = rq.mesh.xy, lq = q - rq.off, ex2 = Math.pow(2.2 * (sim.pdOf[q] || 20), 2);
    const sq = sewn[q], mq = pm[q], wq = w[q];
    for (; e >= 0; e = next[e]) {
      const t = item[e], ia = T[t * 3], ib = T[t * 3 + 1], ic = T[t * 3 + 2];
      if (ia === q || ib === q || ic === q) continue;
      if (wq === 0 && w[ia] === 0 && w[ib] === 0 && w[ic] === 0) continue;
      const pt = tp[t];
      if (pt === pq) { // own piece: skip triangles that are neighbours in the flat pattern
        const la = ia - rq.off, lb = ib - rq.off, lc = ic - rq.off;
        let dx = xyq[2 * la] - xyq[2 * lq], dy = xyq[2 * la + 1] - xyq[2 * lq + 1]; if (dx * dx + dy * dy < ex2) continue;
        dx = xyq[2 * lb] - xyq[2 * lq]; dy = xyq[2 * lb + 1] - xyq[2 * lq + 1]; if (dx * dx + dy * dy < ex2) continue;
        dx = xyq[2 * lc] - xyq[2 * lq]; dy = xyq[2 * lc + 1] - xyq[2 * lq + 1]; if (dx * dx + dy * dy < ex2) continue;
      }
      if (sq && (mq & (1 << (pt & 31))) && (sewn[ia] || sewn[ib] || sewn[ic])) continue; // across its own seam
      const a = ia * 3, b = ib * 3, c = ic * 3;
      const e1x = x[b] - x[a], e1y = x[b + 1] - x[a + 1], e1z = x[b + 2] - x[a + 2];
      const e2x = x[c] - x[a], e2y = x[c + 1] - x[a + 1], e2z = x[c + 2] - x[a + 2];
      let nx = e1y * e2z - e1z * e2y, ny = e1z * e2x - e1x * e2z, nz = e1x * e2y - e1y * e2x;
      const nl = Math.sqrt(nx * nx + ny * ny + nz * nz); if (nl < 1e-9) continue;
      nx /= nl; ny /= nl; nz /= nl;
      const rx = qx - x[a], ry = qy - x[a + 1], rz = qz - x[a + 2];
      const sd = rx * nx + ry * ny + rz * nz; if (sd > R || sd < -R) continue;
      const d00 = e1x * e1x + e1y * e1y + e1z * e1z, d01 = e1x * e2x + e1y * e2y + e1z * e2z, d11 = e2x * e2x + e2y * e2y + e2z * e2z;
      const d20 = rx * e1x + ry * e1y + rz * e1z, d21 = rx * e2x + ry * e2y + rz * e2z, den = d00 * d11 - d01 * d01;
      if (den < 1e-9) continue;
      const bv = (d11 * d20 - d01 * d21) / den, bw = (d00 * d21 - d01 * d20) / den, bu = 1 - bv - bw;
      if (bu < -0.08 || bv < -0.08 || bw < -0.08) continue;
      let side;
      if (!AT._noLayer && lay[q] !== lay[ia]) { // layer order decides
        AT.gridSample(grid, (x[a] + x[b] + x[c]) / 3, (x[a + 1] + x[b + 1] + x[c + 1]) / 3, (x[a + 2] + x[b + 2] + x[c + 2]) / 3, g);
        const so = g[0] * nx + g[1] * ny + g[2] * nz >= 0 ? 1 : -1;
        side = lay[q] > lay[ia] ? so : -so;
      } else { // keep the side it came from (start of this substep)
        const sp = (p[kq] - (bu * p[a] + bv * p[b] + bw * p[c])) * nx + (p[kq + 1] - (bu * p[a + 1] + bv * p[b + 1] + bw * p[c + 1])) * ny + (p[kq + 2] - (bu * p[a + 2] + bv * p[b + 2] + bw * p[c + 2])) * nz;
        side = (Math.abs(sp) > 1e-6 ? sp : sd) >= 0 ? 1 : -1;
      }
      push(q, t, side);
    }
  }
  cc.n = n;
  AT.contactPass(sim);
};
AT.contactPass = function (sim) {
  const cc = sim.cc; if (!cc || !cc.n) return;
  const x = sim.x, pp = sim.p, w = sim.w, T = cc.T, fr = sim.fric, th = sim.thickOf, P = sim.params;
  const H0 = P.clothThickness == null ? 2.5 : P.clothThickness;
  const maxStep = Math.max(2, cc.edge * 0.25);
  const cq = cc.cq, ct = cc.ct, cs = cc.cs;
  for (let e = 0; e < cc.n; e++) {
    const q = cq[e], t = ct[e], side = cs[e], ia = T[t * 3], ib = T[t * 3 + 1], ic = T[t * 3 + 2];
    const kq = q * 3, a = ia * 3, b = ib * 3, c = ic * 3;
    const e1x = x[b] - x[a], e1y = x[b + 1] - x[a + 1], e1z = x[b + 2] - x[a + 2];
    const e2x = x[c] - x[a], e2y = x[c + 1] - x[a + 1], e2z = x[c + 2] - x[a + 2];
    let nx = e1y * e2z - e1z * e2y, ny = e1z * e2x - e1x * e2z, nz = e1x * e2y - e1y * e2x;
    const nl = Math.sqrt(nx * nx + ny * ny + nz * nz); if (nl < 1e-9) continue;
    nx /= nl; ny /= nl; nz /= nl;
    const rx = x[kq] - x[a], ry = x[kq + 1] - x[a + 1], rz = x[kq + 2] - x[a + 2];
    const d00 = e1x * e1x + e1y * e1y + e1z * e1z, d01 = e1x * e2x + e1y * e2y + e1z * e2z, d11 = e2x * e2x + e2y * e2y + e2z * e2z;
    const d20 = rx * e1x + ry * e1y + rz * e1z, d21 = rx * e2x + ry * e2y + rz * e2z, den0 = d00 * d11 - d01 * d01;
    if (den0 < 1e-9) continue;
    let bv = (d11 * d20 - d01 * d21) / den0, bw = (d00 * d21 - d01 * d20) / den0, bu = 1 - bv - bw;
    if (bu < -0.12 || bv < -0.12 || bw < -0.12) continue; // slid off this triangle
    bu = bu < 0 ? 0 : bu; bv = bv < 0 ? 0 : bv; bw = bw < 0 ? 0 : bw; const bs = bu + bv + bw; bu /= bs; bv /= bs; bw /= bs;
    const h = H0 + 0.5 * (th[q] + (th[ia] + th[ib] + th[ic]) / 3);
    const sCur = side * ((x[kq] - (bu * x[a] + bv * x[b] + bw * x[c])) * nx + (x[kq + 1] - (bu * x[a + 1] + bv * x[b + 1] + bw * x[c + 1])) * ny + (x[kq + 2] - (bu * x[a + 2] + bv * x[b + 2] + bw * x[c + 2])) * nz);
    if (sCur >= h) continue; // not touching: no push, no friction
    const wq = w[q], wa = w[ia] * bu, wb = w[ib] * bv, wc = w[ic] * bw;
    const den = wq + wa * bu + wb * bv + wc * bw; if (den <= 0) continue;
    const Cn = Math.min(h - sCur, maxStep);
    {
      const lam = Cn / den, fx = side * nx * lam, fy = side * ny * lam, fz = side * nz * lam;
      x[kq] += fx * wq; x[kq + 1] += fy * wq; x[kq + 2] += fz * wq;
      x[a] -= fx * wa; x[a + 1] -= fy * wa; x[a + 2] -= fz * wa;
      x[b] -= fx * wb; x[b + 1] -= fy * wb; x[b + 2] -= fz * wb;
      x[c] -= fx * wc; x[c + 1] -= fy * wc; x[c + 2] -= fz * wc;
    }
    // Coulomb friction: tangential correction bounded by mu x the normal correction (static inside the cone, kinetic outside)
    const mu = (fr[q] + fr[ia]) * 0.5;
    if (mu > 0) {
      let tx = (x[kq] - pp[kq]) - (bu * (x[a] - pp[a]) + bv * (x[b] - pp[b]) + bw * (x[c] - pp[c]));
      let ty = (x[kq + 1] - pp[kq + 1]) - (bu * (x[a + 1] - pp[a + 1]) + bv * (x[b + 1] - pp[b + 1]) + bw * (x[c + 1] - pp[c + 1]));
      let tz = (x[kq + 2] - pp[kq + 2]) - (bu * (x[a + 2] - pp[a + 2]) + bv * (x[b + 2] - pp[b + 2]) + bw * (x[c + 2] - pp[c + 2]));
      const tn = tx * nx + ty * ny + tz * nz; tx -= tn * nx; ty -= tn * ny; tz -= tn * nz;
      const tl = Math.hypot(tx, ty, tz); if (tl < 1e-9) continue;
      const f = AT.fricK(sim.params, mu, Cn, tl) / den;
      x[kq] -= tx * f * wq; x[kq + 1] -= ty * f * wq; x[kq + 2] -= tz * f * wq;
      x[a] += tx * f * wa; x[a + 1] += ty * f * wa; x[a + 2] += tz * f * wa;
      x[b] += tx * f * wb; x[b + 1] += ty * f * wb; x[b + 2] += tz * f * wb;
      x[c] += tx * f * wc; x[c + 1] += ty * f * wc; x[c + 2] += tz * f * wc;
    }
  }
};
// CPU fallback: particle-based cloth collision (fast enough for the CPU), used when WebGPU is unavailable
AT.selfCollideCPU = function (sim) {
  const N = sim.N, x = sim.x, w = sim.w, act = sim.active, rad = sim.rad;
  let rmax = 0, pmax = 0, lmin = 99, lmax = -99; const lay = sim.layerOf, pdOf = sim.pdOf, g3 = [0, 0, 0];
  for (let q = 0; q < N; q++) { if (rad[q] > rmax) rmax = rad[q]; if (pdOf[q] > pmax) pmax = pdOf[q]; if (lay[q] < lmin) lmin = lay[q]; if (lay[q] > lmax) lmax = lay[q]; }
  if (!rmax || N < 2) return;
  const multiLayer = lmax > lmin, cs = Math.max(rmax * 2, multiLayer ? (AT.LAYER_GAP == null ? 0.6 : AT.LAYER_GAP) * pmax * 1.3 : 0), TS = 2 * N + 1;
  if (!sim._hc || sim._hc.length !== TS + 1) { sim._hc = new Int32Array(TS + 1); sim._he = new Int32Array(N); }
  const cellStart = sim._hc, entries = sim._he;
  const hashF = function (ix, iy, iz) { return Math.abs((ix * 92837111) ^ (iy * 689287499) ^ (iz * 283923481)) % TS; };
  cellStart.fill(0);
  for (let q = 0; q < N; q++) { const k = q * 3; cellStart[hashF(Math.floor(x[k] / cs), Math.floor(x[k + 1] / cs), Math.floor(x[k + 2] / cs))]++; }
  let start = 0; for (let i = 0; i < TS; i++) { start += cellStart[i]; cellStart[i] = start; } cellStart[TS] = start;
  for (let q = 0; q < N; q++) { const k = q * 3; const h = hashF(Math.floor(x[k] / cs), Math.floor(x[k + 1] / cs), Math.floor(x[k + 2] / cs)); cellStart[h]--; entries[cellStart[h]] = q; }
  const recs = sim.recs, pieceOf = sim.pieceOf, sewn = sim.sewn, fr = sim.fric, pp = sim.p;
  const cq = sim._cpq = [], co = sim._cpo = [];
  for (let q = 0; q < N; q++) {
    if (!act[q]) continue;
    const k = q * 3, ix = Math.floor(x[k] / cs), iy = Math.floor(x[k + 1] / cs), iz = Math.floor(x[k + 2] / cs);
    const rq = recs[pieceOf[q]], lq = q - rq.off, xyq = rq.mesh.xy;
    for (let a = ix - 1; a <= ix + 1; a++) for (let b = iy - 1; b <= iy + 1; b++) for (let c = iz - 1; c <= iz + 1; c++) {
      const h = hashF(a, b, c);
      for (let e = cellStart[h]; e < cellStart[h + 1]; e++) {
        const o = entries[e];
        if (o <= q || !act[o]) continue;
        if (sewn[q] && sewn[o]) continue;
        const ko = o * 3;
        const dx = x[ko] - x[k], dy = x[ko + 1] - x[k + 1], dz = x[ko + 2] - x[k + 2];
        const layered = multiLayer && lay[q] !== lay[o];
        const minD = layered ? Math.max((rad[q] + rad[o]) * 0.5, (AT.LAYER_GAP == null ? 0.6 : AT.LAYER_GAP) * Math.max(pdOf[q], pdOf[o])) : (rad[q] + rad[o]) * 0.5;
        const d2 = dx * dx + dy * dy + dz * dz;
        if (d2 < 1e-12) continue;
        if (layered) { if (d2 < 1.69 * minD * minD) { cq.push(q); co.push(o); AT.layerContactCPU(sim, q, o, g3, minD); } continue; }
        if (d2 >= minD * minD) { if (pieceOf[o] !== pieceOf[q] && d2 < 1.69 * minD * minD) { cq.push(q); co.push(o); } continue; }
        if (pieceOf[o] !== pieceOf[q]) { cq.push(q); co.push(o); }
        if (pieceOf[o] === pieceOf[q]) {
          const lo = o - rq.off; const ex = xyq[2 * lo] - xyq[2 * lq], ey = xyq[2 * lo + 1] - xyq[2 * lq + 1];
          if (ex * ex + ey * ey < 16 * minD * minD) continue;
        }
        const wsum = w[q] + w[o]; if (wsum === 0) continue;
        const d = Math.sqrt(d2), corr = (minD - d) / d / wsum;
        x[k] -= dx * corr * w[q]; x[k + 1] -= dy * corr * w[q]; x[k + 2] -= dz * corr * w[q];
        x[ko] += dx * corr * w[o]; x[ko + 1] += dy * corr * w[o]; x[ko + 2] += dz * corr * w[o];
        // cloth-on-cloth friction: damp the relative sliding of the two contacts during this substep
        if (fr && pieceOf[o] !== pieceOf[q]) {
          const mu = (fr[q] + fr[o]) * 0.5;
          const nx = dx / d, ny = dy / d, nz = dz / d;
          let rx = (x[ko] - pp[ko]) - (x[k] - pp[k]), ry = (x[ko + 1] - pp[ko + 1]) - (x[k + 1] - pp[k + 1]), rz = (x[ko + 2] - pp[ko + 2]) - (x[k + 2] - pp[k + 2]);
          const rn = rx * nx + ry * ny + rz * nz; rx -= rn * nx; ry -= rn * ny; rz -= rn * nz;
          const tl = Math.hypot(rx, ry, rz), dn = minD - d;
          const f = tl < 1e-9 ? 0 : AT.fricK(sim.params, mu, dn, tl) / wsum;
          x[k] += rx * f * w[q]; x[k + 1] += ry * f * w[q]; x[k + 2] += rz * f * w[q];
          x[ko] -= rx * f * w[o]; x[ko + 1] -= ry * f * w[o]; x[ko + 2] -= rz * f * w[o];
        }
      }
    }
  }
};
// cloth layers: contacts between different layers are separated radially with a gap that covers the coarser mesh;
// if the outer layer has crossed to the inner side (along the body normal) it is moved back out
AT.layerContactCPU = function (sim, q, o, g, minD) {
  const x = sim.x, w = sim.w, lay = sim.layerOf;
  const ws = w[q] + w[o]; if (ws === 0) return;
  const k = q * 3, ko = o * 3;
  let dx = x[ko] - x[k], dy = x[ko + 1] - x[k + 1], dz = x[ko + 2] - x[k + 2];
  const d = Math.sqrt(dx * dx + dy * dy + dz * dz);
  if (d < minD && d > 1e-9) {
    const c = (minD - d) / d / ws;
    x[k] -= dx * c * w[q]; x[k + 1] -= dy * c * w[q]; x[k + 2] -= dz * c * w[q];
    x[ko] += dx * c * w[o]; x[ko + 1] += dy * c * w[o]; x[ko + 2] += dz * c * w[o];
  }
  const inn = lay[q] < lay[o] ? q : o, out = inn === q ? o : q, ki = inn * 3, kk = out * 3;
  AT.gridSample(sim.av.grid, (x[ki] + x[kk]) * 0.5, (x[ki + 1] + x[kk + 1]) * 0.5, (x[ki + 2] + x[kk + 2]) * 0.5, g);
  const L = Math.hypot(g[0], g[1], g[2]); if (L < 1e-9) return;
  const nx = g[0] / L, ny = g[1] / L, nz = g[2] / L;
  const sep = (x[kk] - x[ki]) * nx + (x[kk + 1] - x[ki + 1]) * ny + (x[kk + 2] - x[ki + 2]) * nz;
  if (sep < 0) {
    const wi = w[inn], wo = w[out], c = -sep / ws;
    x[kk] += nx * c * wo; x[kk + 1] += ny * c * wo; x[kk + 2] += nz * c * wo;
    x[ki] -= nx * c * wi; x[ki + 1] -= ny * c * wi; x[ki + 2] -= nz * c * wi;
  }
};
// persistent cloth-cloth contacts (found by the last selfCollide): keep layers apart and apply friction on every substep
AT.contactPassCPU = function (sim) {
  const cq = sim._cpq, co = sim._cpo; if (!cq || !cq.length) return;
  const x = sim.x, w = sim.w, rad = sim.rad, fr = sim.fric, pp = sim.p, lay = sim.layerOf, pdOf = sim.pdOf, g3 = [0, 0, 0];
  for (let e = 0; e < cq.length; e++) {
    const q = cq[e], o = co[e], k = q * 3, ko = o * 3;
    const wsum = w[q] + w[o]; if (wsum === 0) continue;
    const dx = x[ko] - x[k], dy = x[ko + 1] - x[k + 1], dz = x[ko + 2] - x[k + 2];
    const layered = lay[q] !== lay[o];
    const minD = layered ? Math.max((rad[q] + rad[o]) * 0.5, (AT.LAYER_GAP == null ? 0.6 : AT.LAYER_GAP) * Math.max(pdOf[q], pdOf[o])) : (rad[q] + rad[o]) * 0.5, d2 = dx * dx + dy * dy + dz * dz;
    if (d2 >= 1.69 * minD * minD || d2 < 1e-12) continue;
    const d = Math.sqrt(d2), nx = dx / d, ny = dy / d, nz = dz / d;
    if (layered) AT.layerContactCPU(sim, q, o, g3, minD);
    else if (d < minD) {
      const corr = (minD - d) / wsum;
      x[k] -= nx * corr * w[q]; x[k + 1] -= ny * corr * w[q]; x[k + 2] -= nz * corr * w[q];
      x[ko] += nx * corr * w[o]; x[ko + 1] += ny * corr * w[o]; x[ko + 2] += nz * corr * w[o];
    }
    if (d >= minD) continue; // not touching: no friction
    const mu = (fr[q] + fr[o]) * 0.5, dn = minD - d;
    let rx = (x[ko] - pp[ko]) - (x[k] - pp[k]), ry = (x[ko + 1] - pp[ko + 1]) - (x[k + 1] - pp[k + 1]), rz = (x[ko + 2] - pp[ko + 2]) - (x[k + 2] - pp[k + 2]);
    const rn = rx * nx + ry * ny + rz * nz; rx -= rn * nx; ry -= rn * ny; rz -= rn * nz;
    const tl = Math.hypot(rx, ry, rz); if (tl < 1e-9) continue;
    const f = AT.fricK(sim.params, mu, dn, tl) / wsum;
    x[k] += rx * f * w[q]; x[k + 1] += ry * f * w[q]; x[k + 2] += rz * f * w[q];
    x[ko] -= rx * f * w[o]; x[ko + 1] -= ry * f * w[o]; x[ko + 2] -= rz * f * w[o];
  }
};
AT.step = function (sim, dt) {
  const P = sim.params, n = P.substeps, h = (dt || 1 / 60) / n, N = sim.N;
  const x = sim.x, p = sim.p, v = sim.v, w = sim.w, G = sim.av.grid, act = sim.active;
  const S = sim.S, B = sim.B, SW = sim.SW, FA = sim.FA;
  const Si = S.i, Sj = S.j, SL = S.L, Sr = S.r, Scm = S.cm, Sn = S.n;
  const Bi = B.i, Bj = B.j, BL = B.L, Br = B.r, Bn = B.n;
  const SWa = SW.a, SWb0 = SW.b0, SWb1 = SW.b1, SWu = SW.u, SWn = SW.n;
  const margin = sim.margin, fricA = sim.fric, dampA = sim.damp;
  const ih2 = 1 / (h * h);
  const g = [0, 0, 0];
  // sewing phase: hold gravity back while seams are still open (prevents garments dropping off before they close)
  let gap = 0;
  if (SWn) {
    let acc = 0;
    for (let c = 0; c < SWn; c += 2) {
      const a = SWa[c] * 3, b0 = SWb0[c] * 3, b1 = SWb1[c] * 3, u = SWu[c];
      const dx = (1 - u) * x[b0] + u * x[b1] - x[a], dy = (1 - u) * x[b0 + 1] + u * x[b1 + 1] - x[a + 1], dz = (1 - u) * x[b0 + 2] + u * x[b1 + 2] - x[a + 2];
      acc += Math.sqrt(dx * dx + dy * dy + dz * dz);
    }
    gap = acc / Math.ceil(SWn / 2);
  }
  sim.sewGap = gap;
  const gTarget = AT.clamp(1 - (gap - 6) / 30, 0.03, 1);
  sim.gScale = sim.gScale == null ? gTarget : (gTarget < sim.gScale ? gTarget : sim.gScale + (gTarget - sim.gScale) * 0.04);
  const gy = -P.gravity * sim.gScale;
  // per-piece holdback: a garment added over an already-sewn one keeps floating until its own seams close
  const nPc = sim.recs.length, pieceOf = sim.pieceOf;
  if (!sim.gP || sim.gP.length !== nPc) sim.gP = new Float32Array(nPc).fill(1);
  if (SWn) {
    const gs = new Float64Array(nPc), gc = new Float64Array(nPc);
    for (let c = 0; c < SWn; c += 2) {
      const a = SWa[c] * 3, b0 = SWb0[c] * 3, b1 = SWb1[c] * 3, u = SWu[c];
      const dx = (1 - u) * x[b0] + u * x[b1] - x[a], dy = (1 - u) * x[b0 + 1] + u * x[b1 + 1] - x[a + 1], dz = (1 - u) * x[b0 + 2] + u * x[b1 + 2] - x[a + 2];
      const L = Math.sqrt(dx * dx + dy * dy + dz * dz), pa = pieceOf[SWa[c]], pb = pieceOf[SWb0[c]];
      gs[pa] += L; gc[pa]++; if (pb !== pa) { gs[pb] += L; gc[pb]++; }
    }
    for (let r = 0; r < nPc; r++) { const tg = AT.clamp(1 - ((gc[r] ? gs[r] / gc[r] : 0) - 6) / 30, 0.03, 1); sim.gP[r] = tg < sim.gP[r] ? tg : sim.gP[r] + (tg - sim.gP[r]) * 0.04; }
  }
  const gyP = new Float32Array(nPc); for (let r = 0; r < nPc; r++) gyP[r] = -P.gravity * Math.min(sim.gScale, sim.gP[r]);
  const maxSew = P.sewSpeed * h;
  const hasPressure = sim.pressure.some(function (a) { return a !== 0; });
  if (hasPressure) AT.computeNormals(sim);
  const vmax = 6000;
  for (let s = 0; s < n; s++) {
    for (let q = 0; q < N; q++) {
      const k = q * 3;
      if (w[q] === 0) { p[k] = x[k]; p[k + 1] = x[k + 1]; p[k + 2] = x[k + 2]; continue; }
      const dmp = Math.max(0, 1 - (dampA[q] + P.air) * h);
      v[k] *= dmp; v[k + 1] = (v[k + 1] + gyP[pieceOf[q]] * h) * dmp; v[k + 2] *= dmp;
      if (hasPressure && sim.pressure[q]) { const f = sim.pressure[q] * 40 * h; v[k] += sim.nrm[k] * f; v[k + 1] += sim.nrm[k + 1] * f; v[k + 2] += sim.nrm[k + 2] * f; }
      const sp = Math.hypot(v[k], v[k + 1], v[k + 2]);
      if (sp > vmax) { const f = vmax / sp; v[k] *= f; v[k + 1] *= f; v[k + 2] *= f; }
      p[k] = x[k]; p[k + 1] = x[k + 1]; p[k + 2] = x[k + 2];
      x[k] += v[k] * h; x[k + 1] += v[k + 1] * h; x[k + 2] += v[k + 2] * h;
    }
    // avatar & floor collision
    for (let q = 0; q < N; q++) {
      if (w[q] === 0) continue;
      const k = q * 3;
      const m = margin[q];
      const d = AT.gridSample(G, x[k], x[k + 1], x[k + 2], g);
      if (d < m) {
        const L = Math.hypot(g[0], g[1], g[2]) || 1, nx = g[0] / L, ny = g[1] / L, nz = g[2] / L, push = m - d;
        x[k] += nx * push; x[k + 1] += ny * push; x[k + 2] += nz * push;
        const ddx = x[k] - p[k], ddy = x[k + 1] - p[k + 1], ddz = x[k + 2] - p[k + 2];
        const dn = ddx * nx + ddy * ny + ddz * nz;
        const tx = ddx - dn * nx, ty = ddy - dn * ny, tz = ddz - dn * nz;
        const mu = fricA[q];
        x[k] -= tx * mu; x[k + 1] -= ty * mu; x[k + 2] -= tz * mu;
      }
      if (x[k + 1] < 1 + m * 0.3) { x[k + 1] = 1 + m * 0.3; x[k] = p[k] + (x[k] - p[k]) * 0.5; x[k + 2] = p[k + 2] + (x[k + 2] - p[k + 2]) * 0.5; }
    }
    for (let it = 0; it < P.iterations; it++) {
      // stretch
      for (let c = 0; c < Sn; c++) {
        const i = Si[c], j = Sj[c], wi = w[i], wj = w[j], ws = wi + wj;
        if (ws === 0) continue;
        const a = i * 3, b = j * 3;
        const dx = x[b] - x[a], dy = x[b + 1] - x[a + 1], dz = x[b + 2] - x[a + 2];
        const L = Math.sqrt(dx * dx + dy * dy + dz * dz); if (L < 1e-9) continue;
        const C = L - SL[c];
        const al = Sr[c] * (C < 0 ? Scm[c] : 1) * ih2;
        const dl = -C / (ws + al) / L;
        x[a] -= wi * dl * dx; x[a + 1] -= wi * dl * dy; x[a + 2] -= wi * dl * dz;
        x[b] += wj * dl * dx; x[b + 1] += wj * dl * dy; x[b + 2] += wj * dl * dz;
      }
      // sewing
      for (let c = 0; c < SWn; c++) {
        const ia = SWa[c], i0 = SWb0[c], i1 = SWb1[c], u = SWu[c], u0 = 1 - u;
        const wa = w[ia], wb0 = w[i0], wb1 = w[i1];
        const den = wa + u0 * u0 * wb0 + u * u * wb1;
        if (den === 0) continue;
        const a = ia * 3, b0 = i0 * 3, b1 = i1 * 3;
        const tx = u0 * x[b0] + u * x[b1], ty = u0 * x[b0 + 1] + u * x[b1 + 1], tz = u0 * x[b0 + 2] + u * x[b1 + 2];
        const dx = tx - x[a], dy = ty - x[a + 1], dz = tz - x[a + 2];
        const L = Math.sqrt(dx * dx + dy * dy + dz * dz); if (L < 1e-6) continue;
        const C = L <= 10 ? L : Math.min(L, Math.max(10, maxSew + L * 0.02)); // hard once closed, rate-limited while approaching
        const s2 = C / L / den;
        x[a] += wa * s2 * dx; x[a + 1] += wa * s2 * dy; x[a + 2] += wa * s2 * dz;
        x[b0] -= wb0 * u0 * s2 * dx; x[b0 + 1] -= wb0 * u0 * s2 * dy; x[b0 + 2] -= wb0 * u0 * s2 * dz;
        x[b1] -= wb1 * u * s2 * dx; x[b1 + 1] -= wb1 * u * s2 * dy; x[b1 + 2] -= wb1 * u * s2 * dz;
      }
    }
    // bending
    for (let c = 0; c < Bn; c++) {
      const i = Bi[c], j = Bj[c], wi = w[i], wj = w[j], ws = wi + wj;
      if (ws === 0) continue;
      const a = i * 3, b = j * 3;
      const dx = x[b] - x[a], dy = x[b + 1] - x[a + 1], dz = x[b + 2] - x[a + 2];
      const L = Math.sqrt(dx * dx + dy * dy + dz * dz); if (L < 1e-9) continue;
      const C = L - BL[c];
      const rb = Br[c];
      if (rb >= 0 && C > 0) continue; // bending resists folding only, never stretch
      const dl = -C / (ws + (rb < 0 ? -rb : rb) * ws) / L;
      x[a] -= wi * dl * dx; x[a + 1] -= wi * dl * dy; x[a + 2] -= wi * dl * dz;
      x[b] += wj * dl * dx; x[b + 1] += wj * dl * dy; x[b + 2] += wj * dl * dz;
    }
    for (let c = 0; c < FA.n; c++) {
      const i = FA.a[c], j = FA.b[c], wi = w[i], wj = w[j], ws = wi + wj; if (ws === 0) continue;
      const a = i * 3, b = j * 3;
      const dx = x[b] - x[a], dy = x[b + 1] - x[a + 1], dz = x[b + 2] - x[a + 2];
      const L = Math.sqrt(dx * dx + dy * dy + dz * dz); if (L < 2) continue;
      const C = Math.min(L - 2, maxSew), f = C / L / ws;
      x[a] += wi * f * dx; x[a + 1] += wi * f * dy; x[a + 2] += wi * f * dz;
      x[b] -= wj * f * dx; x[b + 1] -= wj * f * dy; x[b + 2] -= wj * f * dz;
    }
    if (P.selfCollision) { if (P.cpuTriCollision) { const ke = P.ccEvery || 4; if (s % ke === ke - 1) AT.selfCollide(sim); else AT.contactPass(sim); } else if (s % 5 === 4) AT.selfCollideCPU(sim); else AT.contactPassCPU(sim); }
    // grab
    if (sim.grab) {
      const gb = sim.grab, an = s === 0 ? AT.grabAnchor(sim, dt || 1 / 60) : (gb.cur || gb.target);
      for (let e = 0; e < gb.idx.length; e++) {
        const q = gb.idx[e], k = q * 3, f = gb.wt[e];
        const tx = an[0] + gb.rel[e * 3], ty = an[1] + gb.rel[e * 3 + 1], tz = an[2] + gb.rel[e * 3 + 2];
        x[k] += (tx - x[k]) * f; x[k + 1] += (ty - x[k + 1]) * f; x[k + 2] += (tz - x[k + 2]) * f;
      }
    }
    // final guard: only true penetration (inside the skin), no friction
    for (let q = 0; q < N; q++) {
      if (w[q] === 0) continue;
      const k = q * 3;
      const d = AT.gridSample(G, x[k], x[k + 1], x[k + 2], g);
      const m2 = margin[q] * 0.35;
      if (d < m2) { const L = Math.hypot(g[0], g[1], g[2]) || 1, push = m2 - d; x[k] += g[0] / L * push; x[k + 1] += g[1] / L * push; x[k + 2] += g[2] / L * push; }
      if (x[k + 1] < 1) x[k + 1] = 1;
    }
    if (sim.bt && (s === n - 1 || s === (n >> 1) - 1)) sim.btFixes = AT.bodyTriCollide(sim);
    // pins
    for (let e = 0; e < sim.pins.length; e++) { const q = sim.pins[e], k = q * 3; x[k] = sim.pinPos[e * 3]; x[k + 1] = sim.pinPos[e * 3 + 1]; x[k + 2] = sim.pinPos[e * 3 + 2]; }
    const ih = 1 / h;
    for (let q = 0; q < N; q++) {
      const k = q * 3;
      v[k] = (x[k] - p[k]) * ih; v[k + 1] = (x[k + 1] - p[k + 1]) * ih; v[k + 2] = (x[k + 2] - p[k + 2]) * ih;
    }
  }
  // NaN guard + energy
  let bad = false, e = 0, cnt = 0;
  for (let q = 0; q < N; q++) {
    const k = q * 3;
    if (!(x[k] === x[k]) || !(x[k + 1] === x[k + 1]) || !(x[k + 2] === x[k + 2])) { bad = true; break; }
    if (w[q] > 0) { e += v[k] * v[k] + v[k + 1] * v[k + 1] + v[k + 2] * v[k + 2]; cnt++; }
  }
  if (bad) { x.set(sim.good); p.set(sim.good); v.fill(0); }
  else if ((sim.frames & 15) === 0) sim.good.set(x);
  sim.energy = cnt ? Math.sqrt(e / cnt) : 0;
  sim.frames++;
  sim.time += dt || 1 / 60;
};

// Body-vertex vs cloth-triangle collision: keeps the cloth SURFACE (not only its particles) clear of the skin,
// so convex body detail smaller than the particle distance cannot poke through.
AT.prepBodyTri = function (sim) {
  const av = sim.av;
  if (!av || !av.mesh || av.kind !== 'makehuman') { sim.bt = null; return; }
  const nb = av.bodyV || av.mesh.pos.length / 3;
  const T = []; const tm = [];
  sim.recs.forEach(function (r) {
    const tr = r.mesh.tris;
    for (let i = 0; i < tr.length; i += 3) {
      const a = tr[i] + r.off, b = tr[i + 1] + r.off, c = tr[i + 2] + r.off;
      T.push(a, b, c); tm.push(Math.min(sim.margin[a], sim.margin[b], sim.margin[c]));
    }
  });
  const HS = 1 << 16;
  sim.bt = { P: av.mesh.pos, Nn: av.mesh.nor, nb: nb, T: Int32Array.from(T), tm: Float32Array.from(tm), C: 24, HS: HS, head: new Int32Array(HS), next: new Int32Array(T.length / 3 * 8 + 16), item: new Int32Array(T.length / 3 * 8 + 16), cand: null };
};
AT.bodyTriCollide = function (sim) {
  const bt = sim.bt; if (!bt) return 0;
  const x = sim.x, w = sim.w, T = bt.T, C = bt.C, HS = bt.HS, head = bt.head, mask = HS - 1;
  const hk = function (i, j, k) { return ((i * 73856093) ^ (j * 19349663) ^ (k * 83492791)) & mask; };
  head.fill(-1);
  let cnt = 0;
  const nt = T.length / 3;
  for (let t = 0; t < nt; t++) {
    const a = T[t * 3] * 3, b = T[t * 3 + 1] * 3, c = T[t * 3 + 2] * 3;
    if (w[T[t * 3]] === 0 && w[T[t * 3 + 1]] === 0 && w[T[t * 3 + 2]] === 0) continue;
    const i0 = Math.floor(Math.min(x[a], x[b], x[c]) / C), i1 = Math.floor(Math.max(x[a], x[b], x[c]) / C);
    const j0 = Math.floor(Math.min(x[a + 1], x[b + 1], x[c + 1]) / C), j1 = Math.floor(Math.max(x[a + 1], x[b + 1], x[c + 1]) / C);
    const k0 = Math.floor(Math.min(x[a + 2], x[b + 2], x[c + 2]) / C), k1 = Math.floor(Math.max(x[a + 2], x[b + 2], x[c + 2]) / C);
    for (let i = i0; i <= i1; i++) for (let j = j0; j <= j1; j++) for (let k = k0; k <= k1; k++) {
      if (cnt >= bt.item.length) break;
      const h = hk(i, j, k); bt.item[cnt] = t; bt.next[cnt] = head[h]; head[h] = cnt; cnt++;
    }
  }
  const P = bt.P, Nn = bt.Nn;
  let fixes = 0;
  for (let v = 0; v < bt.nb; v++) {
    const px = P[v * 3], py = P[v * 3 + 1], pz = P[v * 3 + 2];
    let e = head[hk(Math.floor(px / C), Math.floor(py / C), Math.floor(pz / C))];
    if (e < 0) continue;
    const nx = Nn[v * 3], ny = Nn[v * 3 + 1], nz = Nn[v * 3 + 2];
    for (; e >= 0; e = bt.next[e]) {
      const t = bt.item[e], ia = T[t * 3], ib = T[t * 3 + 1], ic = T[t * 3 + 2];
      const a = ia * 3, b = ib * 3, c = ic * 3;
      const e1x = x[b] - x[a], e1y = x[b + 1] - x[a + 1], e1z = x[b + 2] - x[a + 2];
      const e2x = x[c] - x[a], e2y = x[c + 1] - x[a + 1], e2z = x[c + 2] - x[a + 2];
      // ray from the skin point along its normal: P + n s  (Moller-Trumbore)
      const hx = ny * e2z - nz * e2y, hy = nz * e2x - nx * e2z, hz = nx * e2y - ny * e2x;
      const det = e1x * hx + e1y * hy + e1z * hz;
      if (det > -1e-6 && det < 1e-6) continue;
      const inv = 1 / det, sx = px - x[a], sy = py - x[a + 1], sz = pz - x[a + 2];
      const u = (sx * hx + sy * hy + sz * hz) * inv; if (u < -0.02 || u > 1.02) continue;
      const qx = sy * e1z - sz * e1y, qy = sz * e1x - sx * e1z, qz = sx * e1y - sy * e1x;
      const vv = (nx * qx + ny * qy + nz * qz) * inv; if (vv < -0.02 || u + vv > 1.02) continue;
      const s = (e2x * qx + e2y * qy + e2z * qz) * inv; // signed distance from skin to cloth along the normal
      const m = bt.tm[t] * 0.8;
      if (s >= m || s < -14) continue;
      const b0 = Math.max(0, 1 - u - vv), b1 = Math.max(0, u), b2 = Math.max(0, vv);
      const wa = w[ia] ? b0 : 0, wb = w[ib] ? b1 : 0, wc = w[ic] ? b2 : 0, den = wa * wa + wb * wb + wc * wc;
      if (den < 1e-6) continue;
      const f = (m - s) / den;
      // position projection only: shift the previous position too so the correction adds no velocity
      const pp = sim.p;
      if (wa) { const dx = nx * f * wa, dy = ny * f * wa, dz = nz * f * wa; x[a] += dx; x[a + 1] += dy; x[a + 2] += dz; pp[a] += dx; pp[a + 1] += dy; pp[a + 2] += dz; }
      if (wb) { const dx = nx * f * wb, dy = ny * f * wb, dz = nz * f * wb; x[b] += dx; x[b + 1] += dy; x[b + 2] += dz; pp[b] += dx; pp[b + 1] += dy; pp[b + 2] += dz; }
      if (wc) { const dx = nx * f * wc, dy = ny * f * wc, dz = nz * f * wc; x[c] += dx; x[c + 1] += dy; x[c + 2] += dz; pp[c] += dx; pp[c + 1] += dy; pp[c + 2] += dz; }
      fixes++;
    }
  }
  return fixes;
};
// rigid translate of a piece (simulation paused)
AT.translatePiece = function (sim, ri, dx, dy, dz) {
  const r = sim.recs[ri]; if (!r) return;
  sim._dirtyX = true;
  for (let i = 0; i < r.n; i++) { const k = (r.off + i) * 3; sim.x[k] += dx; sim.x[k + 1] += dy; sim.x[k + 2] += dz; sim.p[k] = sim.x[k]; sim.p[k + 1] = sim.x[k + 1]; sim.p[k + 2] = sim.x[k + 2]; }
};
// per-vertex fit values: mode 'strain' (ratio), 'stress' (kPa), 'fit' (fraction of limit), 'pressure' (contact 0/1)
AT.fitValues = function (sim, doc, mode) {
  const out = new Float32Array(sim.N), cnt = new Float32Array(sim.N), x = sim.x;
  const fabrics = doc.fabrics || {};
  if (mode === 'pressure') {
    const g = [0, 0, 0];
    for (let q = 0; q < sim.N; q++) { const k = q * 3; const d = AT.gridSample(sim.av.grid, x[k], x[k + 1], x[k + 2], g); out[q] = d < sim.margin[q] + 1.2 ? 1 : 0; }
    return out;
  }
  sim.recs.forEach(function (r) {
    const M = r.mesh, T = M.tris, xy = M.xy, o = r.off;
    const fab = fabrics[r.pc.fabric]; const ph = (fab && fab.phys) || AT.FABRIC_PRESETS.poplin.phys;
    const kAvg = (ph.stretchWeft + ph.stretchWarp) / 2;
    const lim = AT.limitStrain(kAvg), th = ph.thickness || 0.5;
    for (let t = 0; t < T.length; t += 3) {
      const a = T[t], b = T[t + 1], c = T[t + 2];
      const u1 = xy[2 * b] - xy[2 * a], v1 = xy[2 * b + 1] - xy[2 * a + 1], u2 = xy[2 * c] - xy[2 * a], v2 = xy[2 * c + 1] - xy[2 * a + 1];
      const det = u1 * v2 - u2 * v1; if (Math.abs(det) < 1e-9) continue;
      const i00 = v2 / det, i01 = -u2 / det, i10 = -v1 / det, i11 = u1 / det;
      const A = (o + a) * 3, Bk = (o + b) * 3, Ck = (o + c) * 3;
      const e1 = [x[Bk] - x[A], x[Bk + 1] - x[A + 1], x[Bk + 2] - x[A + 2]], e2 = [x[Ck] - x[A], x[Ck + 1] - x[A + 1], x[Ck + 2] - x[A + 2]];
      const f0 = [e1[0] * i00 + e2[0] * i10, e1[1] * i00 + e2[1] * i10, e1[2] * i00 + e2[2] * i10];
      const f1 = [e1[0] * i01 + e2[0] * i11, e1[1] * i01 + e2[1] * i11, e1[2] * i01 + e2[2] * i11];
      const C00 = f0[0] * f0[0] + f0[1] * f0[1] + f0[2] * f0[2], C11 = f1[0] * f1[0] + f1[1] * f1[1] + f1[2] * f1[2], C01 = f0[0] * f1[0] + f0[1] * f1[1] + f0[2] * f1[2];
      const tr = C00 + C11, dt2 = Math.sqrt(Math.max(0, (C00 - C11) * (C00 - C11) / 4 + C01 * C01));
      const lmax = Math.sqrt(Math.max(0, tr / 2 + dt2));
      let val;
      if (mode === 'strain') val = lmax;
      else if (mode === 'stress') val = AT.stressKPa(kAvg, Math.max(0, lmax - 1), th);
      else val = Math.max(0, lmax - 1) / lim;
      out[o + a] += val; out[o + b] += val; out[o + c] += val; cnt[o + a]++; cnt[o + b]++; cnt[o + c]++;
    }
  });
  for (let q = 0; q < sim.N; q++) if (cnt[q]) out[q] /= cnt[q]; else out[q] = mode === 'strain' ? 1 : 0;
  return out;
};
AT.fitSummary = function (vals, sim) {
  let tight = 0, cant = 0, tot = 0;
  for (let q = 0; q < sim.N; q++) { if (!sim.active[q]) continue; tot++; if (vals[q] >= 1) cant++; else if (vals[q] >= 0.6) tight++; }
  return { tight: tot ? tight / tot : 0, cant: tot ? cant / tot : 0 };
};


/* ---------- project model & garment templates ---------- */
AT.SEW_COLORS = ['#E8654A', '#3B82F6', '#10B981', '#F59E0B', '#8B5CF6', '#EC4899', '#14B8A6', '#EF4444', '#6366F1', '#84CC16', '#06B6D4', '#F97316'];
AT.newProject = function (name, sex) {
  const fab = AT.makeFabric('jersey', { id: 'fab_main', name: 'Cotton Jersey', color: '#efeae1' });
  return {
    meta: { name: name || 'Untitled garment', createdAt: Date.now(), updatedAt: Date.now(), version: 1 },
    units: 'cm',
    avatar: Object.assign({}, AT.AVATAR_PRESETS[sex === 'male' ? 'male' : 'female'], { skin: '#b4b1ac', show: true }),
    pieces: {}, sewing: {}, fabrics: { fab_main: fab }, buttons: {}, holes: {}, stitches: {}, poms: {}, comments: {},
    colorways: { cw_main: { id: 'cw_main', name: 'Colorway 1', order: 0, over: {} } }, activeColorway: 'cw_main',
    sizes: ['XS', 'S', 'M', 'L', 'XL'], baseSize: 'M', activeSize: 'M',
    sim: { quality: 'normal', selfCollision: true }
  };
};
AT.P = function (id, x, y, c) { return c ? { id: id, x: x, y: y, c: 1 } : { id: id, x: x, y: y }; };
AT.newPiece = function (name, pts, over) {
  return Object.assign({ id: AT.uid('pc'), name: name, pts: pts, internals: [], notches: [], sa: 10, grain: 0, fabric: 'fab_main', pd: 20, layer: 0, state: 'active', arr: { slot: 'front', off: 60 }, order: 0 }, over || {});
};
AT.translatePts = function (pts, dx, dy) { return pts.map(function (p) { const q = Object.assign({}, p); q.x += dx; q.y += dy; return q; }); };
AT.mirrorPts = function (pts, cx, prefix) { // mirror horizontally around cx, keep CCW order, remap ids
  const out = pts.slice().reverse().map(function (p) { const q = Object.assign({}, p); q.x = 2 * cx - p.x; q.id = (prefix || 'm') + p.id; return q; });
  // after reversal, curve flags must stay attached to their own points (they are) but the start corner of each segment changes
  return out;
};
AT.side = function (piece, startId, rev, t0, t1) {
  const s = rev ? { a: { s: startId, t: t1 == null ? 1 : t1 }, b: { s: startId, t: t0 == null ? 0 : t0 }, dir: -1 } : { a: { s: startId, t: t0 == null ? 0 : t0 }, b: { s: startId, t: t1 == null ? 1 : t1 }, dir: 1 };
  s.p = piece.id; return s;
};
AT.addSew = function (doc, a, b, extra) {
  const id = AT.uid('sw');
  const n = Object.keys(doc.sewing).length;
  doc.sewing[id] = Object.assign({ id: id, a: Array.isArray(a) ? a : [a], b: Array.isArray(b) ? b : [b], color: AT.SEW_COLORS[n % AT.SEW_COLORS.length] }, extra || {});
  return id;
};
AT.addPiece = function (doc, pc) { pc.order = Object.keys(doc.pieces).length; doc.pieces[pc.id] = pc; return pc; };

// --- T-shirt (bodice + set-in short sleeves + neck rib), drafted from avatar measures ---
AT.draftTee = function (doc, opts) {
  opts = opts || {};
  const m = doc.avatar, ease = opts.ease == null ? 90 : opts.ease, len = opts.length || Math.round(m.height * 0.385);
  const TP = opts.TP || AT.torsoProfile(m);
  const W = (m.bust + ease) / 4, sh = m.shoulder / 2 + 18, nk = m.neck / (2 * Math.PI) + 16, drop = 42, ah = Math.round(m.height * 0.10);
  const hemW = Math.max(W, (m.hip + ease * 0.6) / 4);
  const front = function (name, depth, ox, isBack) {
    const c = depth / 0.75;
    const neck = isBack ? [AT.P('hpsR', nk, 0), AT.P('nkR', nk * 0.5, depth, 1), AT.P('cbk', 0, depth), AT.P('nkL', -nk * 0.5, depth, 1)] : [AT.P('hpsR', nk, 0), AT.P('nkR', nk * 0.82, c, 1), AT.P('nkL', -nk * 0.82, c, 1)];
    const pts = neck.concat([AT.P('hpsL', -nk, 0), AT.P('shL', -sh, drop),
      AT.P('ahL1', -sh + 6, drop + ah * 0.55, 1), AT.P('ahL2', -W + 14, drop + ah * 0.95, 1), AT.P('uaL', -W, drop + ah), AT.P('hemL', -hemW, len),
      AT.P('hemR', hemW, len), AT.P('uaR', W, drop + ah), AT.P('ahR2', W - 14, drop + ah * 0.95, 1), AT.P('ahR1', sh - 6, drop + ah * 0.55, 1), AT.P('shR', sh, drop)]);
    return AT.translatePts(pts, ox, 0);
  };
  const chestH = TP.shoulderY(nk) + 10; // HPS seated on the shoulder surface
  const F = AT.addPiece(doc, AT.newPiece('Front', front('Front', opts.neckDepth || 78, 0), { arr: { slot: 'front', off: 70, h: chestH - len / 2 } }));
  const Bk = AT.addPiece(doc, AT.newPiece('Back', front('Back', 20, 2 * W + 120, true), { arr: { slot: 'back', off: 70, h: chestH - len / 2 } }));
  // sleeves
  const armholeLen = function (pc) {
    const ol = AT.outline(pc.pts); const k1 = ol.byId['uaR'], k2 = ol.byId['shL'];
    return ol.segs[k1].len + ol.segs[k2].len;
  };
  const AH = armholeLen(F) / 2 + armholeLen(Bk) / 2; // front+back armhole, one side
  const sLen = opts.sleeve || 210, capH = Math.round(m.height * 0.04);
  // solve sleeve half-width so cap length ≈ armhole + ease
  const capEase = 45;
  let sw = AH * 0.42;
  const capPts = function (hw) { return [AT.P('uaR', hw, capH), AT.P('cR', hw * 0.55, -capH * 0.42, 1), AT.P('cL', -hw * 0.55, -capH * 0.42, 1), AT.P('uaL', -hw, capH), AT.P('hemL', -hw * 0.9, capH + sLen), AT.P('hemR', hw * 0.9, capH + sLen)]; };
  for (let it = 0; it < 30; it++) { const ol = AT.outline(capPts(sw)); const L = ol.segs[0].len; sw *= (AH + capEase) / L; }
  const sx = 4 * W + 300;
  const SL = AT.addPiece(doc, AT.newPiece('Sleeve L', AT.translatePts(capPts(sw), sx, 0), { arr: { slot: 'armL', off: 55, t: 15 + (capH + sLen) / 2 } }));
  const SR = AT.addPiece(doc, AT.newPiece('Sleeve R', AT.translatePts(capPts(sw), sx + 2 * sw + 100, 0), { arr: { slot: 'armR', off: 55, t: 15 + (capH + sLen) / 2 } }));
  // neck rib
  const bol = AT.outline(Bk.pts); const neckLen = AT.outline(F.pts).segs[0].len + bol.segs[bol.byId.hpsR].len + bol.segs[bol.byId.cbk].len;
  const ribL = neckLen * 0.86, ribW = 22;
  const rib = AT.addPiece(doc, AT.newPiece('Neck Rib', [AT.P('a', 0, 0), AT.P('b', ribL, 0), AT.P('c', ribL, ribW), AT.P('d', 0, ribW)].map(function (p) { p.y += len + 80; return p; }), { pd: 10, fabric: 'fab_rib', arr: { slot: 'neck', off: 12, h: chestH + 8, ang: 0 } }));
  if (!doc.fabrics.fab_rib) doc.fabrics.fab_rib = AT.makeFabric('rib', { id: 'fab_rib', name: 'Rib Knit 1×1', color: '#efeae1' });
  // sewing: shoulders, sides, underarms, armholes (M:N), neck (M:N), rib ends
  AT.addSew(doc, AT.side(F, 'hpsL'), AT.side(Bk, 'shR', true));
  AT.addSew(doc, AT.side(F, 'shR', true), AT.side(Bk, 'hpsL'));
  AT.addSew(doc, AT.side(F, 'uaL'), AT.side(Bk, 'hemR', true));
  AT.addSew(doc, AT.side(F, 'hemR', true), AT.side(Bk, 'uaL'));
  AT.addSew(doc, AT.side(SL, 'uaL'), AT.side(SL, 'hemR', true));
  AT.addSew(doc, AT.side(SR, 'uaL'), AT.side(SR, 'hemR', true));
  AT.addSew(doc, AT.side(SL, 'uaR', true), [AT.side(F, 'uaR'), AT.side(Bk, 'shL')]);
  AT.addSew(doc, AT.side(SR, 'uaR'), [AT.side(F, 'shL', true), AT.side(Bk, 'uaR', true)]);
  AT.addSew(doc, AT.side(rib, 'c'), [AT.side(Bk, 'cbk'), AT.side(F, 'hpsR'), AT.side(Bk, 'hpsR')]);
  AT.addSew(doc, AT.side(rib, 'b'), AT.side(rib, 'd', true));
  doc.stitches = doc.stitches || {};
  const st = AT.uid('st'); doc.stitches[st] = { id: st, p: F.id, seg: 'hemL', off: 18, len: 3, gap: 1.6, w: 0.9, color: '#3b3a36', rows: 2, rowGap: 6 };
  const st2 = AT.uid('st'); doc.stitches[st2] = { id: st2, p: Bk.id, seg: 'hemL', off: 18, len: 3, gap: 1.6, w: 0.9, color: '#3b3a36', rows: 2, rowGap: 6 };
  doc.poms = doc.poms || {};
  const pm = function (name, refs) { const id = AT.uid('pm'); doc.poms[id] = { id: id, name: name, refs: refs }; };
  pm('Front neckline', [{ p: F.id, s: 'hpsR' }]);
  pm('Shoulder seam', [{ p: F.id, s: 'hpsL' }]);
  pm('Side seam', [{ p: F.id, s: 'uaL' }]);
  pm('Front hem width', [{ p: F.id, s: 'hemL' }]);
  pm('Sleeve cap', [{ p: SL.id, s: 'uaR' }]);
  pm('Sleeve hem', [{ p: SL.id, s: 'hemL' }]);
  doc.meta.name = doc.meta.name === 'Untitled garment' ? 'Essential Tee' : doc.meta.name;
  return doc;
};
// --- sleeveless A-line shift dress ---
AT.draftDress = function (doc, opts) {
  opts = opts || {};
  const m = doc.avatar, ease = 70, len = opts.length || Math.round(m.height * 0.56);
  const TP = opts.TP || AT.torsoProfile(m);
  const W = (m.bust + ease) / 4, sh = m.shoulder / 2 - 22, nk = m.neck / (2 * Math.PI) + 22, drop = 40, ah = Math.round(m.height * 0.105);
  const wW = (m.waist + 90) / 4, hW = (m.hip + 70) / 4, flare = hW + len * 0.12;
  const top = TP.shoulderY(nk) + 10;
  const yW = Math.round(top - TP.ys.waist), yH = Math.round(top - TP.ys.hip);
  const body = function (depth, ox) {
    const c = depth / 0.75;
    return AT.translatePts([AT.P('hpsR', nk, 0), AT.P('nkR', nk * 0.8, c, 1), AT.P('nkL', -nk * 0.8, c, 1), AT.P('hpsL', -nk, 0), AT.P('shL', -sh, drop),
      AT.P('ahL1', -sh + 2, drop + ah * 0.6, 1), AT.P('ahL2', -W + 18, drop + ah, 1), AT.P('uaL', -W, drop + ah), AT.P('wL', -wW - 6, yW, 1), AT.P('hipL', -hW, yH), AT.P('hemL', -flare, len),
      AT.P('hemC', 0, len + 8, 1), AT.P('hemR', flare, len), AT.P('hipR', hW, yH), AT.P('wR', wW + 6, yW, 1), AT.P('uaR', W, drop + ah), AT.P('ahR2', W - 18, drop + ah, 1), AT.P('ahR1', sh - 2, drop + ah * 0.6, 1), AT.P('shR', sh, drop)], ox, 0);
  };
  const F = AT.addPiece(doc, AT.newPiece('Front', body(opts.neckDepth || 95, 0), { arr: { slot: 'front', off: 75, h: top - len / 2 } }));
  const Bk = AT.addPiece(doc, AT.newPiece('Back', body(30, 2 * flare + 120), { arr: { slot: 'back', off: 75, h: top - len / 2 } }));
  AT.addSew(doc, AT.side(F, 'hpsL'), AT.side(Bk, 'shR', true));
  AT.addSew(doc, AT.side(F, 'shR', true), AT.side(Bk, 'hpsL'));
  AT.addSew(doc, [AT.side(F, 'uaL'), AT.side(F, 'hipL')], [AT.side(Bk, 'hipR', true), AT.side(Bk, 'hemR', true)]);
  AT.addSew(doc, [AT.side(F, 'hipR', true), AT.side(F, 'hemR', true)], [AT.side(Bk, 'uaL'), AT.side(Bk, 'hipL')]);
  const pm = function (name, refs) { const id = AT.uid('pm'); doc.poms[id] = { id: id, name: name, refs: refs }; };
  pm('Front neckline', [{ p: F.id, s: 'hpsR' }]); pm('Side seam', [{ p: F.id, s: 'uaL' }, { p: F.id, s: 'hipL' }]); pm('Front hem', [{ p: F.id, s: 'hemL' }]);
  doc.fabrics.fab_main = Object.assign(doc.fabrics.fab_main, AT.makeFabric('linen', { id: 'fab_main', name: 'Linen', color: '#b9c3b2' }));
  doc.meta.name = doc.meta.name === 'Untitled garment' ? 'Linen Shift Dress' : doc.meta.name;
  return doc;
};
// --- A-line skirt with waistband ---
AT.draftSkirt = function (doc, opts) {
  opts = opts || {};
  const m = doc.avatar, len = opts.length || Math.round(m.height * 0.34);
  const wW = (m.waist + 20) / 4, hW = (m.hip + 50) / 4, hip = 190, flare = hW + len * 0.2;
  const panel = function (ox) {
    return AT.translatePts([AT.P('wR', wW, 0), AT.P('wC', 0, 8, 1), AT.P('wL', -wW, 0), AT.P('hL', -hW - 2, hip * 0.7, 1), AT.P('hipL', -hW, hip), AT.P('hemL', -flare, len), AT.P('hemC', 0, len + 14, 1), AT.P('hemR', flare, len), AT.P('hipR', hW, hip), AT.P('hR', hW + 2, hip * 0.7, 1)], ox, 0);
  };
  const top = (opts.TP ? opts.TP.ys.waist : m.height * 0.625) + 18;
  const F = AT.addPiece(doc, AT.newPiece('Skirt Front', panel(0), { arr: { slot: 'front', off: 70, h: top - len / 2 } }));
  const Bk = AT.addPiece(doc, AT.newPiece('Skirt Back', panel(2 * flare + 120), { arr: { slot: 'back', off: 70, h: top - len / 2 } }));
  const wl = AT.outline(F.pts).segs[0].len * 2 + 4, bw = 36;
  const WB = AT.addPiece(doc, AT.newPiece('Waistband', [AT.P('a', 0, 0), AT.P('b', wl, 0), AT.P('c', wl, bw), AT.P('d', 0, bw)].map(function (p) { p.y -= 120; return p; }), { pd: 12, fabric: 'fab_wb', arr: { slot: 'front', off: 8, h: top + bw / 2 - 2, ang: -65 } }));
  if (!doc.fabrics.fab_wb) doc.fabrics.fab_wb = AT.makeFabric('wool', { id: 'fab_wb', name: 'Waistband — Wool', color: '#3e3a35' });
  AT.addSew(doc, [AT.side(F, 'wL'), AT.side(F, 'hipL')], [AT.side(Bk, 'hipR', true), AT.side(Bk, 'hemR', true)]);
  AT.addSew(doc, [AT.side(F, 'hipR', true), AT.side(F, 'hemR', true)], [AT.side(Bk, 'wL'), AT.side(Bk, 'hipL')]);
  AT.addSew(doc, AT.side(WB, 'c'), [AT.side(F, 'wR'), AT.side(Bk, 'wR')]);
  AT.addSew(doc, AT.side(WB, 'b'), AT.side(WB, 'd', true));
  doc.fabrics.fab_main = Object.assign(doc.fabrics.fab_main, AT.makeFabric('wool', { id: 'fab_main', name: 'Wool Suiting', color: '#7a6a58', weave: 'twill' }));
  const pm = function (name, refs) { const id = AT.uid('pm'); doc.poms[id] = { id: id, name: name, refs: refs }; };
  pm('Waist (half)', [{ p: F.id, s: 'wR' }]); pm('Side seam', [{ p: F.id, s: 'wL' }, { p: F.id, s: 'hipL' }]); pm('Hem (half)', [{ p: F.id, s: 'hemL' }]);
  doc.meta.name = doc.meta.name === 'Untitled garment' ? 'A-line Skirt' : doc.meta.name;
  return doc;
};
AT.TEMPLATES = [
  { id: 'tee', name: 'Essential Tee', desc: 'Set-in sleeve jersey tee with neck rib', fn: 'draftTee' },
  { id: 'dress', name: 'Shift Dress', desc: 'Sleeveless A-line linen dress', fn: 'draftDress' },
  { id: 'skirt', name: 'A-line Skirt', desc: 'Wool skirt with waistband', fn: 'draftSkirt' },
  { id: 'blank', name: 'Blank', desc: 'Empty board — draft from scratch', fn: null }
];
AT.fromTemplate = function (tid, sex, name, TP) {
  const doc = AT.newProject(name, sex);
  const t = AT.TEMPLATES.find(function (x) { return x.id === tid; });
  if (t && t.fn) AT[t.fn](doc, { TP: TP });
  return doc;
};
// normalize data coming back from Firebase (arrays/objects/missing collections)
AT.normalize = function (doc) {
  doc = doc || {};
  ['pieces', 'sewing', 'fabrics', 'buttons', 'holes', 'stitches', 'poms', 'comments', 'colorways', 'graphics'].forEach(function (k) { if (!doc[k] || typeof doc[k] !== 'object') doc[k] = {}; });
  doc.meta = doc.meta || { name: 'Untitled garment' };
  doc.avatar = Object.assign({}, AT.AVATAR_PRESETS[(doc.avatar && doc.avatar.sex) === 'male' ? 'male' : 'female'], { skin: '#b4b1ac', show: true }, doc.avatar || {});
  doc.sizes = AT.asArray(doc.sizes); if (!doc.sizes.length) doc.sizes = ['M'];
  doc.baseSize = doc.baseSize || doc.sizes[0]; doc.activeSize = doc.activeSize || doc.baseSize;
  if (!Object.keys(doc.colorways).length) doc.colorways = { cw_main: { id: 'cw_main', name: 'Colorway 1', order: 0, over: {} } };
  if (!doc.activeColorway || !doc.colorways[doc.activeColorway]) doc.activeColorway = Object.keys(doc.colorways)[0];
  doc.units = doc.units || 'cm';
  doc.sim = doc.sim || { quality: 'normal', selfCollision: true };
  Object.keys(doc.pieces).forEach(function (k) {
    const pc = doc.pieces[k]; if (!pc) { delete doc.pieces[k]; return; }
    pc.id = k; pc.pts = AT.asArray(pc.pts); pc.internals = AT.asArray(pc.internals).map(function (il) { il.pts = AT.asArray(il.pts); return il; });
    pc.notches = AT.asArray(pc.notches); pc.pins = AT.asArray(pc.pins); pc.graphics = AT.asArray(pc.graphics);
    pc.arr = pc.arr || { slot: 'front', off: 60 };
    if (pc.sa == null) pc.sa = 10;
  });
  Object.keys(doc.sewing).forEach(function (k) { const s = doc.sewing[k]; if (!s) { delete doc.sewing[k]; return; } s.id = k; s.a = AT.asArray(s.a); s.b = AT.asArray(s.b); });
  Object.keys(doc.poms).forEach(function (k) { const s = doc.poms[k]; if (s) s.refs = AT.asArray(s.refs); });
  return doc;
};
// effective fabric (colorway overrides)
AT.effFabric = function (doc, fid) {
  const f = doc.fabrics[fid]; if (!f) return null;
  const cw = doc.colorways && doc.colorways[doc.activeColorway];
  const o = cw && cw.over && cw.over[fid];
  return o ? Object.assign({}, f, o) : f;
};
// sewing length check (2D lengths of each side)
AT.sewLengths = function (doc) {
  const res = {};
  const ols = {};
  const olOf = function (pid) { if (!ols[pid]) { const pc = doc.pieces[pid]; if (!pc) return null; ols[pid] = AT.outline(AT.gradedPts(pc, doc.activeSize, doc.baseSize)); } return ols[pid]; };
  Object.keys(doc.sewing).forEach(function (k) {
    const sw = doc.sewing[k];
    const len = function (sides) { let L = 0; AT.asArray(sides).forEach(function (sd) { const ol = olOf(sd.p); if (!ol) return; const r = AT.sideRange(ol, sd); if (r) L += r.L; }); return L; };
    res[k] = { a: len(sw.a), b: len(sw.b) };
  });
  return res;
};
AT.pomValue = function (doc, pom, size) {
  let L = 0;
  AT.asArray(pom.refs).forEach(function (r) {
    const pc = doc.pieces[r.p]; if (!pc) return;
    const ol = AT.outline(AT.gradedPts(pc, size, doc.baseSize));
    const k = ol.byId[r.s]; if (k != null) L += ol.segs[k].len;
  });
  return L;
};
AT.pieceArea = function (doc, pc, size, withSA) {
  const ol = AT.outline(AT.gradedPts(pc, size || doc.activeSize, doc.baseSize));
  if (!withSA) return Math.abs(AT.area(ol.poly));
  const off = AT.cutOutline(pc, ol);
  return Math.abs(AT.area(off));
};

/* ---------- editing helpers ---------- */
AT.segBezParam = function (sg, tArc) { // arc-length fraction -> bezier parameter
  if (sg.ctrl.length === 2) return tArc;
  const L = sg.len * tArc, cum = sg.cum, n = cum.length - 1;
  let i = 0; while (i < n - 1 && cum[i + 1] < L) i++;
  const u = cum[i + 1] > cum[i] ? (L - cum[i]) / (cum[i + 1] - cum[i]) : 0;
  return (i + u) / n;
};
AT.deCasteljauSplit = function (ctrl, t) {
  const left = [], right = [];
  let pts = ctrl.map(function (p) { return { x: p.x, y: p.y }; });
  while (pts.length) {
    left.push(pts[0]); right.unshift(pts[pts.length - 1]);
    const nx = [];
    for (let i = 0; i < pts.length - 1; i++) nx.push({ x: pts[i].x + (pts[i + 1].x - pts[i].x) * t, y: pts[i].y + (pts[i + 1].y - pts[i].y) * t });
    pts = nx;
  }
  return { left: left, right: right };
};
// insert a corner point at arc fraction t on segment segId, preserving the curve exactly
AT.splitSegment = function (pc, segId, tArc) {
  const ol = AT.outline(pc.pts), k = ol.byId[segId]; if (k == null) return null;
  const sg = ol.segs[k], tb = AT.segBezParam(sg, tArc);
  const sp = AT.deCasteljauSplit(sg.ctrl, tb);
  const nid = AT.uid('p');
  const mid = sp.left[sp.left.length - 1];
  const newCtrl = sp.left.slice(1, -1).map(function (p) { return { id: AT.uid('p'), x: p.x, y: p.y, c: 1 }; })
    .concat([{ id: nid, x: mid.x, y: mid.y }])
    .concat(sp.right.slice(1, -1).map(function (p) { return { id: AT.uid('p'), x: p.x, y: p.y, c: 1 }; }));
  const n = pc.pts.length, out = [];
  // rebuild the points list: replace the interior of segment k
  const interior = {}; let j = (sg.i0 + 1) % n; while (j !== sg.i1) { interior[j] = 1; j = (j + 1) % n; }
  for (let i = 0; i < n; i++) {
    if (interior[i]) continue;
    out.push(pc.pts[i]);
    if (i === sg.i0) newCtrl.forEach(function (q) { out.push(q); });
  }
  pc.pts = out;
  return nid;
};
// add a curve control point to a segment near world point (x,y)
AT.addCurvePoint = function (pc, segId, tArc, x, y) {
  const ol = AT.outline(pc.pts), k = ol.byId[segId]; if (k == null) return null;
  const sg = ol.segs[k], n = pc.pts.length;
  const ctrlIdx = []; let j = sg.i0; ctrlIdx.push(j); j = (j + 1) % n; while (j !== sg.i1) { ctrlIdx.push(j); j = (j + 1) % n; } ctrlIdx.push(sg.i1);
  const m = ctrlIdx.length - 1, pos = Math.max(1, Math.min(m, Math.ceil(tArc * m)));
  const insertAfter = ctrlIdx[pos - 1];
  const id = AT.uid('p');
  pc.pts.splice(insertAfter + 1, 0, { id: id, x: x, y: y, c: 1 });
  return id;
};
AT.setCurvature = function (pc, segId, mx, my) { // single control point so the curve passes through (mx,my)
  const ol = AT.outline(pc.pts), k = ol.byId[segId]; if (k == null) return;
  const sg = ol.segs[k], n = pc.pts.length;
  const P0 = pc.pts[sg.i0], P1 = pc.pts[sg.i1];
  const interior = {}; let j = (sg.i0 + 1) % n; while (j !== sg.i1) { interior[pc.pts[j].id] = 1; j = (j + 1) % n; }
  const keepId = Object.keys(interior)[0] || AT.uid('p');
  const C = { id: keepId, x: 2 * mx - (P0.x + P1.x) / 2, y: 2 * my - (P0.y + P1.y) / 2, c: 1 };
  const out = [];
  for (let i = 0; i < n; i++) { const p = pc.pts[i]; if (interior[p.id]) continue; out.push(p); if (i === sg.i0) out.push(C); }
  pc.pts = out;
};
// re-anchor sewing/notches/stitches/POMs of a piece after its outline structure changed
AT.remapRefs = function (doc, pid, oldPts) {
  const pc = doc.pieces[pid]; if (!pc) return;
  const oldOl = AT.outline(oldPts), newOl = AT.outline(pc.pts);
  const remap = function (s, t) {
    const k = oldOl.byId[s]; if (k == null) return null;
    const p = AT.segPoint(oldOl.segs[k], AT.clamp(t, 0, 1));
    if (newOl.byId[s] != null) { // segment survives: prefer staying on it
      const sg = newOl.segs[newOl.byId[s]];
      let best = { d: Infinity }; for (let i = 0; i < sg.poly.length - 1; i++) { const a = sg.poly[i], b = sg.poly[i + 1], r = AT.distSeg(p.x, p.y, a.x, a.y, b.x, b.y); if (r.d < best.d) best = { d: r.d, t: (sg.cum[i] + (sg.cum[i + 1] - sg.cum[i]) * r.t) / (sg.len || 1) }; }
      if (best.d < 0.5) return { s: s, t: best.t };
    }
    const nr = AT.nearestOnOutline(newOl, p.x, p.y);
    return { s: nr.id, t: nr.t };
  };
  Object.values(doc.sewing).forEach(function (sw) {
    ['a', 'b'].forEach(function (side) {
      sw[side].forEach(function (sd) {
        if (sd.p !== pid) return;
        const A = remap(sd.a.s, sd.a.t), B = remap(sd.b.s, sd.b.t);
        if (A) sd.a = A; if (B) sd.b = B;
      });
    });
  });
  (pc.notches || []).forEach(function (nn) { const r = remap(nn.s, nn.t || 0); if (r) { nn.s = r.s; nn.t = r.t; } });
  Object.values(doc.stitches).forEach(function (st) { if (st.p === pid && st.seg !== '*' && newOl.byId[st.seg] == null) { const r = remap(st.seg, 0.5); if (r) st.seg = r.s; } });
};
// rigid best fit (2D Procrustes) of matching point ids: returns fn mapping new-frame point -> old frame
AT.rigidMap = function (oldPts, newPts) {
  const om = {}; oldPts.forEach(function (p) { om[p.id] = p; });
  const pairs = []; newPts.forEach(function (p) { if (om[p.id]) pairs.push([om[p.id], p]); });
  if (pairs.length < 2) return null;
  let cox = 0, coy = 0, cnx = 0, cny = 0;
  pairs.forEach(function (q) { cox += q[0].x; coy += q[0].y; cnx += q[1].x; cny += q[1].y; });
  cox /= pairs.length; coy /= pairs.length; cnx /= pairs.length; cny /= pairs.length;
  let a = 0, b = 0;
  pairs.forEach(function (q) { const ox = q[0].x - cox, oy = q[0].y - coy, nx = q[1].x - cnx, ny = q[1].y - cny; a += ox * nx + oy * ny; b += ox * ny - oy * nx; });
  const th = Math.atan2(b, a), c = Math.cos(th), s = Math.sin(th);
  return function (x, y) { const dx = x - cnx, dy = y - cny; return [c * dx + s * dy + cox, -s * dx + c * dy + coy]; };
};
AT.transformPiece = function (doc, pid, fn) { // fn(x,y)->[x,y] applied to everything anchored in piece space
  const pc = doc.pieces[pid]; if (!pc) return;
  const ap = function (o) { const r = fn(o.x, o.y); o.x = r[0]; o.y = r[1]; };
  pc.pts.forEach(ap);
  (pc.internals || []).forEach(function (il) { (il.pts || []).forEach(ap); });
  (pc.pins || []).forEach(ap);
  (pc.graphics || []).forEach(ap);
  Object.values(doc.buttons).forEach(function (b) { if (b.p === pid) ap(b); });
  Object.values(doc.holes).forEach(function (h) { if (h.p === pid) ap(h); });
  Object.values(doc.comments).forEach(function (c) { if (c.p === pid) ap(c); });
};
AT.deletePieces = function (doc, ids) {
  const del = {}; ids.forEach(function (i) { del[i] = 1; });
  ids.forEach(function (i) { delete doc.pieces[i]; });
  Object.keys(doc.sewing).forEach(function (k) { const sw = doc.sewing[k]; sw.a = sw.a.filter(function (s) { return !del[s.p]; }); sw.b = sw.b.filter(function (s) { return !del[s.p]; }); if (!sw.a.length || !sw.b.length) delete doc.sewing[k]; });
  ['buttons', 'holes', 'stitches', 'comments'].forEach(function (c) { Object.keys(doc[c]).forEach(function (k) { if (del[doc[c][k].p]) delete doc[c][k]; }); });
  Object.keys(doc.poms).forEach(function (k) { const pm = doc.poms[k]; pm.refs = pm.refs.filter(function (r) { return !del[r.p]; }); if (!pm.refs.length) delete doc.poms[k]; });
  Object.values(doc.pieces).forEach(function (pc) { if (pc.mirror && del[pc.mirror.src]) delete pc.mirror; });
};
AT.duplicatePiece = function (doc, pid, mirror, linked) {
  const src = doc.pieces[pid]; if (!src) return null;
  const pc = AT.clone(src); pc.id = AT.uid('pc'); pc.name = src.name + (mirror ? ' (mirror)' : ' copy');
  const bb = AT.bbox(src.pts), cx = bb.x1 + 40 + bb.w / 2;
  const map = {};
  if (mirror) {
    const axis = bb.x1 + 40; // mirror line
    pc.pts = src.pts.slice().reverse().map(function (p) { const id = 'm' + p.id; map[p.id] = id; return { id: id, x: 2 * axis - p.x + 0, y: p.y, c: p.c }; });
    // reversing order moves curve points relative to their corners; rotate so that each segment starts at a corner
    pc.internals = (src.internals || []).map(function (il) { return Object.assign({}, il, { id: AT.uid('il'), pts: (il.pts || []).map(function (p) { return { x: 2 * axis - p.x, y: p.y, c: p.c }; }) }); });
    pc.grain = -(src.grain || 0);
    if (src.arr) { pc.arr = Object.assign({}, src.arr); const sl = AT.slotById(src.arr.slot); const mirrorSlot = { frontL: 'frontR', frontR: 'frontL', left: 'right', right: 'left', backL: 'backR', backR: 'backL', armL: 'armR', armR: 'armL', legLF: 'legRF', legRF: 'legLF', legLB: 'legRB', legRB: 'legLB' }[src.arr.slot]; if (mirrorSlot) pc.arr.slot = mirrorSlot; else if (sl.kind === 'body' && pc.arr.ang) pc.arr.ang = -pc.arr.ang; }
    if (linked) pc.mirror = { src: pid, axis: axis };
    pc.graphics = []; pc.pins = [];
    if (pc.grade) { const g2 = {}; Object.keys(src.grade || {}).forEach(function (k) { const r = src.grade[k], o = {}; Object.keys(r).forEach(function (sz) { o[sz] = { dx: -(r[sz].dx || 0), dy: r[sz].dy || 0 }; }); g2['m' + k] = o; }); pc.grade = g2; }
  } else {
    const dx = bb.w + 40;
    pc.pts = src.pts.map(function (p) { const id = AT.uid('p'); map[p.id] = id; return { id: id, x: p.x + dx, y: p.y, c: p.c }; });
    pc.internals = (src.internals || []).map(function (il) { return Object.assign({}, il, { id: AT.uid('il'), pts: (il.pts || []).map(function (p) { return { x: p.x + dx, y: p.y, c: p.c }; }) }); });
    pc.graphics = (src.graphics || []).map(function (g) { return Object.assign({}, g, { id: AT.uid('g'), x: g.x + dx }); });
    pc.pins = [];
    if (src.grade) { const g2 = {}; Object.keys(src.grade).forEach(function (k) { if (map[k]) g2[map[k]] = src.grade[k]; }); pc.grade = g2; }
  }
  pc.notches = (src.notches || []).map(function (n) { return { id: AT.uid('n'), s: map[n.s] || n.s, t: mirror ? 1 - (n.t || 0) : n.t }; });
  if (mirror) pc.notches = []; // segment starts change under reflection; re-add as needed
  pc.order = Object.keys(doc.pieces).length;
  doc.pieces[pc.id] = pc;
  return pc;
};
// regenerate linked mirror pieces from their sources

/* ---------- seams, free edges, hems ---------- */
// id of the same edge on a linked mirror (point order is reversed, so the mirror segment starts at the source's end corner)
AT.mirrorSegId = function (src, sid) {
  const pts = src.pts, n = pts.length, i = pts.findIndex(function (p) { return p.id === sid; }); if (i < 0) return null;
  let j = (i + 1) % n, guard = 0; while (pts[j].c && guard++ < n) j = (j + 1) % n;
  return 'm' + pts[j].id;
};
AT.srcSegId = function (src, mid) {
  const pts = src.pts, n = pts.length, j = pts.findIndex(function (p) { return p.id === String(mid).slice(1); }); if (j < 0) return null;
  let i = (j - 1 + n) % n, guard = 0; while (pts[i].c && guard++ < n) i = (i - 1 + n) % n;
  return pts[i].id;
};
// stable seam numbers 1..N: by the first piece (pattern order) and position along its outline
AT.seamNumbers = function (doc) {
  const key = function (sw) {
    let best = Infinity;
    ['a', 'b'].forEach(function (k) { AT.asArray(sw[k]).forEach(function (sd) { const pc = doc.pieces[sd.p]; if (!pc || !sd.a) return; const i = pc.pts.findIndex(function (p) { return p.id === sd.a.s; }); const v = (pc.order || 0) * 1e4 + (i < 0 ? 9999 : i) + (sd.a.t || 0) * 0.99; if (v < best) best = v; }); });
    return best;
  };
  const list = Object.values(doc.sewing || {}).filter(Boolean).map(function (sw) { return { id: sw.id, k: key(sw) }; }).sort(function (a, b) { return a.k - b.k || (a.id < b.id ? -1 : 1); });
  const out = {}; list.forEach(function (e, i) { out[e.id] = i + 1; });
  return out;
};
// which parts of a piece's sewing line are sewn (with seam ids and partner pieces) and which are free
AT.edgeCoverage = function (doc, pc, ol, nums) {
  const T = ol.total, sides = [];
  Object.values(doc.sewing || {}).forEach(function (sw) {
    if (!sw) return;
    ['a', 'b'].forEach(function (k) {
      AT.asArray(sw[k]).forEach(function (sd) {
        if (sd.p !== pc.id) return;
        const r = AT.sideRange(ol, sd); if (!r) return;
        const s0 = (((r.dir > 0 ? r.Pa : r.Pa - r.L) % T) + T) % T;
        sides.push({ P0: s0, P1: s0 + r.L, sw: sw, n: nums ? nums[sw.id] : 0, partners: AT.asArray(sw[k === 'a' ? 'b' : 'a']).map(function (o) { return o.p; }) });
      });
    });
  });
  const iv = [];
  sides.forEach(function (s) { if (s.P1 <= T) iv.push([s.P0, s.P1]); else { iv.push([s.P0, T]); iv.push([0, s.P1 - T]); } });
  iv.sort(function (a, b) { return a[0] - b[0]; });
  const sewn = [];
  iv.forEach(function (x) { const m = sewn[sewn.length - 1]; if (m && x[0] <= m[1] + 0.5) m[1] = Math.max(m[1], x[1]); else sewn.push(x.slice()); });
  const free = []; let cur = 0;
  sewn.forEach(function (m) { if (m[0] - cur > 0.5) free.push([cur, m[0]]); cur = Math.max(cur, m[1]); });
  if (T - cur > 0.5) free.push([cur, T]);
  if (free.length > 1 && free[0][0] < 0.5 && free[free.length - 1][1] > T - 0.5) { const last = free.pop(); free[0] = [last[0], T + free[0][1]]; }
  // free parts split per segment (a free run can cover a hem and a plain finished edge)
  const freeParts = [];
  free.forEach(function (f) {
    for (let base = Math.floor(f[0] / T) * T; base < f[1]; base += T) {
      ol.segs.forEach(function (sg, k) {
        const a = Math.max(f[0], base + ol.start[k]), b = Math.min(f[1], base + ol.start[k] + sg.len);
        if (b - a > 0.5) freeParts.push({ P0: a, P1: b, k: k, sid: sg.id });
      });
    }
  });
  return { sides: sides, sewn: sewn, free: free, freeParts: freeParts, T: T };
};
// segment ids a sewing side touches (overlap > 0.5 mm)
AT.sideSegIds = function (ol, sd) {
  const r = AT.sideRange(ol, sd); if (!r) return [];
  const T = ol.total, s0 = (((r.dir > 0 ? r.Pa : r.Pa - r.L) % T) + T) % T, s1 = s0 + r.L, out = [];
  ol.segs.forEach(function (sg, k) {
    const a = ol.start[k], b = a + sg.len;
    const ov = Math.max(0, Math.min(b, s1) - Math.max(a, s0)) + Math.max(0, Math.min(b + T, s1) - Math.max(a + T, s0));
    if (ov > 0.5) out.push(sg.id);
  });
  return out;
};
// one allowance for a whole seam: every segment on both sides (linked mirrors are written through their source)
AT.setSeamSA = function (doc, swId, mm) {
  const sw = doc.sewing[swId]; if (!sw) return 0; let n = 0;
  ['a', 'b'].forEach(function (k) {
    AT.asArray(sw[k]).forEach(function (sd) {
      let pc = doc.pieces[sd.p]; if (!pc) return;
      const ids = AT.sideSegIds(AT.outline(pc.pts), sd);
      ids.forEach(function (sid) {
        let tp = pc, ts = sid;
        if (pc.mirror && doc.pieces[pc.mirror.src]) { const s2 = AT.srcSegId(doc.pieces[pc.mirror.src], sid); if (s2) { tp = doc.pieces[pc.mirror.src]; ts = s2; } }
        tp.saSeg = Object.assign({}, tp.saSeg || {}); tp.saSeg[ts] = mm; n++;
        if (tp !== pc) { pc.saSeg = Object.assign({}, pc.saSeg || {}); pc.saSeg[sid] = mm; }
      });
    });
  });
  AT.syncMirrors(doc);
  return n;
};
// allowance of a seam (the most common value over its segments)
AT.seamSA = function (doc, sw) {
  const cnt = {};
  ['a', 'b'].forEach(function (k) { AT.asArray(sw[k]).forEach(function (sd) { const pc = doc.pieces[sd.p]; if (!pc) return; AT.sideSegIds(AT.outline(pc.pts), sd).forEach(function (sid) { const v = (pc.saSeg && pc.saSeg[sid] != null) ? pc.saSeg[sid] : (pc.sa || 0); cnt[v] = (cnt[v] || 0) + 1; }); }); });
  let best = null; Object.keys(cnt).forEach(function (v) { if (best == null || cnt[v] > cnt[best]) best = v; });
  return best == null ? 10 : +best;
};
AT.defaultHem = function (doc, pc) {
  const bb = AT.bbox(AT.outline(pc.pts).poly), fab = doc.fabrics[pc.fabric] || {}, phys = fab.phys || {};
  if (/jersey|rib|fleece/.test(String(fab.weave || '') + String(fab.preset || ''))) return 20;
  if ((phys.density || 0) > 0 && phys.density < 200) return 25;        // light cloth: a deep hem drags and shows through
  return /^arm/.test((pc.arr || {}).slot || '') ? 30 : bb.h >= 1000 ? 40 : 30;
};
// suggest hems for free (unsewn) edges:
//  - bottom edges (mostly horizontal, lowest 12 % of the piece) -> hem
//  - open vertical edges (front openings: long, not a slit between two sewn edges) -> hem of the same width
//  - a piece with an open front edge is treated as unlined: its other free edges (neck, slits) get a 10 mm narrow hem
AT.detectHems = function (doc) {
  const res = {}, info = [], unlined = {};
  AT.asArray(doc.pieces).forEach(function (pc) {
    if (!pc || !pc.pts || pc.pts.length < 3 || pc.mirror) return;
    const ol = AT.outline(pc.pts), bb = AT.bbox(ol.poly);
    if (bb.h < 200) return;
    const cov = AT.edgeCoverage(doc, pc, ol, null), w = AT.defaultHem(doc, pc), hem = {}, n = ol.segs.length;
    const sewnFrac = function (k) { const s0 = ol.start[k], s1 = s0 + ol.segs[k].len; let v = 0; cov.sewn.forEach(function (m) { v += Math.max(0, Math.min(s1, m[1]) - Math.max(s0, m[0])); }); return v / (ol.segs[k].len || 1); };
    const free = ol.segs.map(function (sg, k) { return sg.len >= 40 && sewnFrac(k) <= 0.2; });
    ol.segs.forEach(function (sg, k) {
      if (!free[k]) return;
      const a = sg.poly[0], b = sg.poly[sg.poly.length - 1], mid = AT.segPoint(sg, 0.5), dx = Math.abs(b.x - a.x), dy = Math.abs(b.y - a.y);
      if (dy <= dx * 0.6 && Math.max(a.y, b.y, mid.y) >= bb.y1 - bb.h * 0.12) { hem[sg.id] = w; return; }
      const slit = sewnFrac((k - 1 + n) % n) > 0.5 && sewnFrac((k + 1) % n) > 0.5;
      if (dx <= dy * 0.35 && sg.len >= bb.h * 0.4 && !slit) { hem[sg.id] = w; unlined[pc.fabric] = 1; }
    });
    info.push({ pc: pc, ol: ol, free: free, hem: hem });
  });
  // a cloth with an open front edge is unlined: every other free edge of its pieces (neck, slits) gets a 10 mm narrow hem
  info.forEach(function (it) {
    if (unlined[it.pc.fabric]) it.ol.segs.forEach(function (sg, k) { if (it.free[k] && it.hem[sg.id] == null) it.hem[sg.id] = 10; });
    if (Object.keys(it.hem).length) res[it.pc.id] = it.hem;
  });
  return res;
};
AT.syncMirrors = function (doc) {
  Object.values(doc.pieces).forEach(function (pc) {
    if (!pc.mirror) return;
    const src = doc.pieces[pc.mirror.src]; if (!src) { delete pc.mirror; return; }
    const axis = pc.mirror.axis;
    const byId = {}; pc.pts.forEach(function (p) { byId[p.id] = p; });
    pc.pts = src.pts.slice().reverse().map(function (p) { return { id: 'm' + p.id, x: 2 * axis - p.x, y: p.y, c: p.c }; });
    pc.internals = (src.internals || []).map(function (il, i) { return Object.assign({}, il, { id: (pc.internals[i] && pc.internals[i].id) || AT.uid('il'), pts: (il.pts || []).map(function (p) { return { x: 2 * axis - p.x, y: p.y, c: p.c }; }) }); });
    if (src.saSeg && Object.keys(src.saSeg).length) { const sm = {}; Object.keys(src.saSeg).forEach(function (k) { const m = AT.mirrorSegId(src, k); if (m && src.saSeg[k] != null) sm[m] = src.saSeg[k]; }); pc.saSeg = sm; } else delete pc.saSeg;
    if (src.hem && Object.keys(src.hem).length) { const hm = {}; Object.keys(src.hem).forEach(function (k) { const m = AT.mirrorSegId(src, k); if (m && src.hem[k] > 0) hm[m] = src.hem[k]; }); pc.hem = hm; } else delete pc.hem;
    if (src.grade) { const g2 = {}; Object.keys(src.grade).forEach(function (k) { const r = src.grade[k], o = {}; Object.keys(r || {}).forEach(function (sz) { o[sz] = { dx: -((r[sz] && r[sz].dx) || 0), dy: (r[sz] && r[sz].dy) || 0 }; }); g2['m' + k] = o; }); pc.grade = g2; } else delete pc.grade;
  });
};

/* ---------- Labadeh (لباده): long Shia clerical over-robe ----------
   Ankle length; edge-to-edge front without buttons (collar hook + hidden hooks to just below the waist);
   close, interfaced stand collar; long set-in sleeves; one-piece skirt with no hem vents;
   pocket-access slits in both side seams so the qaba / trouser pockets stay reachable. */
AT.labadehShapes = function (m, TP, opts) {
  opts = opts || {};
  const H = m.height, ease = opts.ease == null ? 130 : opts.ease;
  const W = (m.bust + ease) / 4, sh = m.shoulder / 2 + 20, nk = m.neck / (2 * Math.PI) + 9;
  const fnd = Math.round(nk * 1.1), bnd = 22, drop = 44, ah = Math.round(H * 0.118), yUA = drop + ah;
  const top = TP.shoulderY(nk) + 10;
  const len = opts.length || Math.round(top - H * 0.068);
  const yW = Math.round(top - TP.ys.waist), yH = Math.round(top - TP.ys.hip);
  const wW = (m.waist + 160) / 4, hW = Math.max(W - 4, (m.hip + 170) / 4), hemW = hW + (len - yH) * 0.16;
  const sideX = function (y) { return y <= yH ? wW + (hW - wW) * Math.sqrt(AT.clamp((y - yW) / Math.max(1, yH - yW), 0, 1)) : hW + (hemW - hW) * (y - yH) / (len - yH); };
  const yT = yW + 40, yB = yT + 175; // pocket-access slit
  const FB = [AT.P('hps', nk, 0), AT.P('n1', nk, fnd * 0.55, 1), AT.P('n2', nk * 0.55, fnd, 1), AT.P('cfN', 0, fnd),
    AT.P('cfH', 0, len), AT.P('hc', hemW * 0.5, len, 1), AT.P('hm', hemW, len - 8),
    AT.P('slB', sideX(yB), yB), AT.P('slT', sideX(yT), yT), AT.P('wc', wW - 6, yW, 1),
    AT.P('ua', W, yUA), AT.P('a2', W - 16, drop + ah * 0.95, 1), AT.P('a1', sh - 8, drop + ah * 0.52, 1), AT.P('sh', sh, drop)];
  const BK = [AT.P('hpsR', nk, 0), AT.P('bnR', nk * 0.5, bnd, 1), AT.P('cbk', 0, bnd), AT.P('bnL', -nk * 0.5, bnd, 1), AT.P('hpsL', -nk, 0),
    AT.P('shL', -sh, drop), AT.P('ahL1', -sh + 8, drop + ah * 0.52, 1), AT.P('ahL2', -W + 16, drop + ah * 0.95, 1), AT.P('uaL', -W, yUA),
    AT.P('wL', -(wW - 6), yW, 1), AT.P('slTL', -sideX(yT), yT), AT.P('slBL', -sideX(yB), yB), AT.P('hmL', -hemW, len - 8),
    AT.P('bh', 0, len + 8, 1), AT.P('hmR', hemW, len - 8), AT.P('slBR', sideX(yB), yB), AT.P('slTR', sideX(yT), yT), AT.P('wR', wW - 6, yW, 1),
    AT.P('uaR', W, yUA), AT.P('ahR2', W - 16, drop + ah * 0.95, 1), AT.P('ahR1', sh - 8, drop + ah * 0.52, 1), AT.P('shR', sh, drop)];
  const segLen = function (pts, id) { const ol = AT.outline(pts); return ol.segs[ol.byId[id]].len; };
  // long set-in sleeve: solve the bicep width so the cap matches front + back armhole plus a little ease
  const AH = segLen(FB, 'ua') + segLen(BK, 'shL');
  const capH = Math.round(H * 0.064), capEase = 18, sLen = m.arm + 5 - capH * 1.065, hwHem = Math.max(140, (m.upperArm || 300) * 0.5);
  const capPts = function (hw) { return [AT.P('uaR', hw, capH), AT.P('cR', hw * 0.55, -capH * 0.42, 1), AT.P('cL', -hw * 0.55, -capH * 0.42, 1), AT.P('uaL', -hw, capH), AT.P('hemL', -hwHem, capH + sLen), AT.P('hemR', hwHem, capH + sLen)]; };
  let hw = AH * 0.4;
  for (let it = 0; it < 30; it++) hw *= (AH + capEase) / segLen(capPts(hw), 'uaR');
  const SL = capPts(hw);
  // stand collar: an annular-sector band whose lower edge equals the neckline
  const neckLen = 2 * segLen(FB, 'hps') + segLen(BK, 'hpsR') + segLen(BK, 'cbk');
  const ch = 40, ce = 12, cd = 8;
  const collar = function (L) { return [AT.P('tl', cd, -ce), AT.P('tc', L / 2, ce, 1), AT.P('tr', L - cd, -ce), AT.P('br', L, ch - ce), AT.P('bc', L / 2, ch + ce, 1), AT.P('bl', 0, ch - ce)]; };
  let L = neckLen;
  for (let it = 0; it < 30; it++) L *= neckLen / segLen(collar(L), 'br');
  return { FB: FB, BK: BK, SL: SL, CO: collar(L), dims: { W: W, nk: nk, fnd: fnd, bnd: bnd, top: top, len: len, yW: yW, yH: yH, yT: yT, yB: yB, hemW: hemW, hw: hw, capH: capH, sLen: sLen, ch: ch, ce: ce, neckLen: neckLen, AH: AH } };
};
AT.draftLabadeh = function (doc, opts) {
  opts = opts || {};
  const m = doc.avatar, TP = opts.TP || AT.torsoProfile(m);
  const S = AT.labadehShapes(m, TP, opts), D = S.dims;
  const mirrorOf = function (pts, axis) { return pts.slice().reverse().map(function (p) { return AT.P('m' + p.id, 2 * axis - p.x, p.y, p.c); }); };
  const bbOf = function (pts) { return AT.bbox(AT.outline(pts).poly); };
  const ringR = function (bb, h, off) { // same radius the body arrangement uses (AT.slotFrame)
    let R = 0; const cy = bb.cy;
    for (let y = h - (bb.y1 - cy); y <= h + (cy - bb.y0); y += 10) { const e = TP.torso.eval(AT.clamp(y, TP.torso.y0, TP.torso.y1)); R = Math.max(R, Math.max(e[0], e[1] + Math.abs(e[2]))); }
    return (R || 160) + off;
  };
  // 2D layout: right front | left front | back | sleeves, collar below the sleeves
  const ox = D.hemW + 80, axis = ox - 40;
  const fbPts = AT.translatePts(S.FB, ox, 0), faPts = mirrorOf(fbPts, axis);
  const bx = ox + 2 * D.hemW + 120, bkPts = AT.translatePts(S.BK, bx, 0);
  const sx = bx + D.hemW + 150 + D.hw;
  const slPts = AT.translatePts(S.SL, sx, 0), srPts = AT.translatePts(S.SL, sx + 2 * D.hw + 100, 0);
  const coPts = AT.translatePts(S.CO, sx - D.hw, D.capH + D.sLen + 160);
  const offB = 75, bbF = bbOf(fbPts), hF = D.top - bbF.cy;
  const angF = (bbF.cx - ox) / ringR(bbF, hF, offB) / AT.deg;
  const bbB = bbOf(bkPts), bbS = bbOf(slPts), bbC = bbOf(coPts);
  const FB = AT.addPiece(doc, AT.newPiece('جلو چپ', fbPts, { pd: 26, arr: { slot: 'front', off: offB, h: hF, ang: Math.round(angF * 10) / 10 } }));
  const FA = AT.addPiece(doc, AT.newPiece('جلو راست', faPts, { pd: 26, arr: { slot: 'front', off: offB, h: hF, ang: -Math.round(angF * 10) / 10 }, mirror: { src: FB.id, axis: axis } }));
  const BK = AT.addPiece(doc, AT.newPiece('پشت', bkPts, { pd: 26, arr: { slot: 'back', off: offB, h: D.top - bbB.cy } }));
  const tS = 15 + (bbS.cy - bbS.y0);
  const SL = AT.addPiece(doc, AT.newPiece('آستین چپ', slPts, { pd: 20, arr: { slot: 'armL', off: 55, t: tS }, notches: [{ id: AT.uid('n'), s: 'uaR', t: 0.5 }] }));
  const SR = AT.addPiece(doc, AT.newPiece('آستین راست', srPts, { pd: 20, arr: { slot: 'armR', off: 55, t: tS }, notches: [{ id: AT.uid('n'), s: 'uaR', t: 0.5 }] }));
  const CO = AT.addPiece(doc, AT.newPiece('یقه', coPts, { pd: 10, strengthen: true, arr: { slot: 'neck', ang: 180, off: 12, h: D.top - D.bnd + (coPts[3].y + D.ce) - bbC.cy }, notches: [{ id: AT.uid('n'), s: 'br', t: 0.5 }] }));
  const side = AT.side;
  // shoulders and side seams (upper and lower parts; the slit between them stays open)
  AT.addSew(doc, side(FA, 'mhps'), side(BK, 'shR', true));
  AT.addSew(doc, side(FB, 'sh', true), side(BK, 'hpsL'));
  AT.addSew(doc, side(FA, 'mua'), side(BK, 'slTR', true));
  AT.addSew(doc, side(FA, 'mslB'), side(BK, 'hmR', true));
  AT.addSew(doc, side(FB, 'slT', true), side(BK, 'uaL'));
  AT.addSew(doc, side(FB, 'hm', true), side(BK, 'slBL'));
  // sleeves: underarm seams and set-in caps (M:N to front + back armholes)
  AT.addSew(doc, side(SL, 'uaL'), side(SL, 'hemR', true));
  AT.addSew(doc, side(SR, 'uaL'), side(SR, 'hemR', true));
  AT.addSew(doc, side(SL, 'uaR', true), [side(FB, 'ua'), side(BK, 'shL')]);
  AT.addSew(doc, side(SR, 'uaR'), [side(FA, 'msh', true), side(BK, 'uaR', true)]);
  // stand collar onto the neckline, its ends hooked at centre front
  AT.addSew(doc, side(CO, 'br'), [side(FA, 'mcfN'), side(BK, 'hpsR'), side(BK, 'cbk'), side(FB, 'hps')]);
  AT.addSew(doc, side(CO, 'tr'), side(CO, 'bl', true));
  // hidden hooks: front edges meet edge to edge from the collar to just below the waist
  const f = AT.clamp((D.yW + 20 - D.fnd) / (D.len - D.fnd), 0.05, 0.95);
  AT.addSew(doc, side(FB, 'cfN', false, 0, f), side(FA, 'mcfH', true, 1 - f, 1));
  // fabric: worsted wool (fastoni); collar interfaced via Strengthen
  doc.fabrics.fab_main = Object.assign(doc.fabrics.fab_main || {}, AT.makeFabric('wool', { id: 'fab_main', name: 'فاستونی پشمی', color: '#57544f', weave: 'twill', content: 'Worsted wool' }));
  doc.colorways = { cw_main: { id: 'cw_main', name: 'طوسی ذغالی', order: 0, over: {} } };
  [['cw_brown', 'قهوه‌ای', '#4b3c31'], ['cw_camel', 'شتری', '#9c8466'], ['cw_cream', 'شیری تابستانی', '#d8cfbe'], ['cw_navy', 'سرمه‌ای', '#2b3242']].forEach(function (c, i) {
    doc.colorways[c[0]] = { id: c[0], name: c[1], order: i + 1, over: { fab_main: { color: c[2] } } };
  });
  doc.activeColorway = 'cw_main';
  doc.stitches = doc.stitches || {};
  [[FB, 'cfN'], [FA, 'mcfH'], [CO, 'tl']].forEach(function (s) { const id = AT.uid('st'); doc.stitches[id] = { id: id, p: s[0].id, seg: s[1], off: 5, len: 2.6, gap: 1.4, w: 0.7, color: '#3a3834', rows: 1, rowGap: 6 }; });
  doc.poms = doc.poms || {};
  const pm = function (name, refs) { const id = AT.uid('pm'); doc.poms[id] = { id: id, name: name, refs: refs }; };
  pm('Front edge (collar to hem)', [{ p: FB.id, s: 'cfN' }]);
  pm('Neckline / collar', [{ p: CO.id, s: 'br' }]);
  pm('Pocket slit', [{ p: FB.id, s: 'slB' }]);
  pm('Front hem (half)', [{ p: FB.id, s: 'cfH' }]);
  pm('Sleeve cap', [{ p: SL.id, s: 'uaR' }]);
  pm('Sleeve opening', [{ p: SL.id, s: 'hemL' }]);
  // grading: re-draft for each size of the chart and store per-point offsets from the base size
  const chart = AT.SIZE_CHARTS[m.sex === 'male' ? 'male' : 'female'];
  const baseSpec = chart.find(function (c) { return c.name === 'M'; }) || chart[Math.floor(chart.length / 2)];
  const shapesFor = function (spec) { const mm = Object.assign({}, m, spec); delete mm.name; return AT.labadehShapes(mm, AT.torsoProfile(mm), opts); };
  const base = shapesFor(baseSpec);
  const grades = { FB: {}, BK: {}, SL: {}, CO: {} };
  chart.forEach(function (c) {
    if (c.name === baseSpec.name) return;
    const s = shapesFor(c);
    Object.keys(grades).forEach(function (k) {
      s[k].forEach(function (p, i) { const b = base[k][i]; const g = grades[k][p.id] = grades[k][p.id] || {}; g[c.name] = { dx: Math.round((p.x - b.x) * 10) / 10, dy: Math.round((p.y - b.y) * 10) / 10 }; });
    });
  });
  FB.grade = grades.FB; BK.grade = grades.BK; SL.grade = grades.SL; SR.grade = AT.clone(grades.SL); CO.grade = grades.CO;
  FA.grade = {}; Object.keys(grades.FB).forEach(function (id) { const o = {}; Object.keys(grades.FB[id]).forEach(function (sz) { o[sz] = { dx: -grades.FB[id][sz].dx, dy: grades.FB[id][sz].dy }; }); FA.grade['m' + id] = o; });
  doc.sizes = chart.map(function (c) { return c.name; }); doc.baseSize = baseSpec.name; doc.activeSize = baseSpec.name;
  doc.techpack = Object.assign({ season: 'All season', notes: 'Labadeh: ankle-length clerical over-robe. Edge-to-edge front without buttons, closed by a hook at the collar and hidden hooks to just below the waist. Stand collar, shoulders and chest interfaced. Long set-in sleeves. One-piece skirt with no hem vents; pocket-access slits in both side seams. Fully lined (lining not modelled).' }, doc.techpack || {});
  doc.meta.name = doc.meta.name === 'Untitled garment' ? 'لباده' : doc.meta.name;
  return doc;
};
AT.TEMPLATES.splice(AT.TEMPLATES.length - 1, 0, { id: 'labadeh', name: 'Labadeh', desc: 'Shia clerical over-robe: stand collar, long sleeves, ankle length', fn: 'draftLabadeh', sex: 'male' });

/* ---------- Aba (عبا): sleeveless open clerical cloak ----------
   Cut like the traditional aba/bisht: one wide body whose sides are straight (the fold of the cloth),
   a long straight shoulder line, an open front with no closure, and an arm slit at the top of each side.
   Worn over the qaba / labadeh, slightly shorter so the robe shows at the hem. */
AT.abaShapes = function (m, TP, opts) {
  opts = opts || {};
  const H = m.height;
  const nkL = m.neck / (2 * Math.PI) + 9;                 // labadeh neckline, used as the shared reference
  const top = TP.shoulderY(nkL) + 10;
  const len = opts.length || (Math.round(top - H * 0.068) - 60);
  const nk = nkL + 35, bnd = opts.backNeck || 70, e0 = 45, fnd = opts.frontNeck || 110; // neck opening wraps loosely around the robe's collar so the aba sits on the shoulders
  const sh = m.shoulder / 2 + 25, drop = 48;
  // kimono-style long shoulder line sloping with the arm, so each arm passes out through its slit and holds the aba on
  const W = opts.halfWidth || Math.round(m.shoulder / 2 + m.arm * 0.62);
  const sT = drop + (W - sh) * Math.tan((opts.slope || 40) * AT.deg);
  const slit = opts.slit || 360, slB = sT + slit;
  const FB = [AT.P('hps', nk, 0), AT.P('n1', nk * 0.92, fnd * 0.62, 1), AT.P('n2', e0 + (nk - e0) * 0.35, fnd, 1), AT.P('fN', e0, fnd),
    AT.P('fH', e0, len), AT.P('hc', (e0 + W) * 0.5, len, 1), AT.P('sH', W, len - 10),
    AT.P('slB', W, slB), AT.P('sT', W, sT), AT.P('shp', sh, drop)];
  const BK = [AT.P('hpsR', nk, 0), AT.P('bnR', nk * 0.5, bnd, 1), AT.P('cbk', 0, bnd), AT.P('bnL', -nk * 0.5, bnd, 1), AT.P('hpsL', -nk, 0),
    AT.P('shpL', -sh, drop), AT.P('sTL', -W, sT), AT.P('slBL', -W, slB), AT.P('sHL', -W, len - 10), AT.P('bh', 0, len + 10, 1),
    AT.P('sHR', W, len - 10), AT.P('slBR', W, slB), AT.P('sTR', W, sT), AT.P('shpR', sh, drop)];
  return { FB: FB, BK: BK, dims: { top: top, len: len, nk: nk, W: W, sT: sT, slB: slB, e0: e0, fnd: fnd, bnd: bnd } };
};
AT.draftAba = function (doc, opts) {
  opts = opts || {};
  const m = doc.avatar, TP = opts.TP || AT.torsoProfile(m);
  const S = AT.abaShapes(m, TP, opts), D = S.dims;
  const mirrorOf = function (pts, axis) { return pts.slice().reverse().map(function (p) { return AT.P('m' + p.id, 2 * axis - p.x, p.y, p.c); }); };
  const bbOf = function (pts) { return AT.bbox(AT.outline(pts).poly); };
  // place to the right of whatever is already on the board
  let x0 = 0; Object.values(doc.pieces || {}).forEach(function (pc) { AT.asArray(pc.pts).forEach(function (p) { x0 = Math.max(x0, p.x); }); });
  if (Object.keys(doc.pieces || {}).length) x0 += 300;
  const ox = x0 + D.W + 40, axis = ox - 20 - D.e0;   // fronts meet the axis with a 40 mm gap
  const fbS = AT.translatePts(S.FB, ox - D.e0, 0), faS = mirrorOf(fbS, axis);
  const bx = ox - D.e0 + D.W + 160 + D.W, bkS = AT.translatePts(S.BK, bx, 0);
  // flat panels in front of and behind the A-posed arms; the two fronts sit side by side (shift), the arms pass between front and back
  const offB = opts.off || 180, bbF = bbOf(fbS), bbB = bbOf(bkS);
  const h = D.top + 50 - bbF.cy, hB = D.top + 50 - bbB.cy;
  const shF = Math.round(bbF.cx - (ox - D.e0));
  const fabId = opts.fabric || 'fab_aba';
  const FB = AT.addPiece(doc, AT.newPiece('عبا · جلو چپ', fbS, { fabric: fabId, pd: 34, layer: 1, arr: { slot: 'front', off: offB, h: h, flat: true, shift: shF } }));
  const FA = AT.addPiece(doc, AT.newPiece('عبا · جلو راست', faS, { fabric: fabId, pd: 34, layer: 1, arr: { slot: 'front', off: offB, h: h, flat: true, shift: -shF }, mirror: { src: FB.id, axis: axis } }));
  const BK = AT.addPiece(doc, AT.newPiece('عبا · پشت', bkS, { fabric: fabId, pd: 34, layer: 1, arr: { slot: 'back', off: Math.round(offB * 0.65), h: hB, flat: true } }));
  const side = AT.side, sew = [];
  // long shoulder lines (neck to shoulder point, then out to the arm slit)
  sew.push(AT.addSew(doc, side(FB, 'shp', true), side(BK, 'hpsL')));
  sew.push(AT.addSew(doc, side(FB, 'sT', true), side(BK, 'shpL')));
  sew.push(AT.addSew(doc, side(FA, 'mhps'), side(BK, 'shpR', true)));
  sew.push(AT.addSew(doc, side(FA, 'mshp'), side(BK, 'sTR', true)));
  // sides closed from the slit to the hem (the fold of the cloth); slits stay open for the arms
  sew.push(AT.addSew(doc, side(FB, 'sH', true), side(BK, 'slBL')));
  sew.push(AT.addSew(doc, side(FA, 'mslB'), side(BK, 'sHR', true)));
  if (!doc.fabrics[fabId]) {
    const ph = Object.assign(AT.clone(AT.FABRIC_PRESETS.wool.phys), { stretchWeft: 62, stretchWarp: 66, shear: 40, bendWeft: 22, bendWarp: 24, buckling: 55, density: 170, thickness: 0.45, damping: 30, friction: 60 });
    doc.fabrics[fabId] = AT.makeFabric('wool', { id: fabId, name: 'پشم عبایی', color: '#1f1e1c', weave: 'twill', sheen: 0.14, content: 'Light wool (aba cloth)', phys: ph });
  }
  // colorways: black by default, dark brown with the brown robe, translucent khachieh with the summer robe
  Object.values(doc.colorways || {}).forEach(function (cw) {
    cw.over = cw.over || {};
    if (cw.id === 'cw_brown') cw.over[fabId] = { color: '#2e241d' };
    else if (cw.id === 'cw_cream') cw.over[fabId] = { color: '#c9b48e', opacity: 0.88 };
  });
  doc.stitches = doc.stitches || {};
  [[FB, 'fN'], [FA, 'mfH'], [FB, 'hps'], [FA, 'mfN'], [BK, 'hpsR'], [BK, 'cbk']].forEach(function (s) { const id = AT.uid('st'); doc.stitches[id] = { id: id, p: s[0].id, seg: s[1], off: 8, len: 2.4, gap: 1.2, w: 0.8, color: '#4a4136', rows: 2, rowGap: 5 }; });
  doc.poms = doc.poms || {};
  const pm = function (name, refs) { const id = AT.uid('pm'); doc.poms[id] = { id: id, name: name, refs: refs }; };
  pm('Aba front edge', [{ p: FB.id, s: 'fN' }]);
  pm('Aba arm slit', [{ p: FB.id, s: 'slB' }]);
  pm('Aba shoulder line', [{ p: FB.id, s: 'shp' }, { p: FB.id, s: 'sT' }]);
  // grading from the size chart, as for the labadeh
  const chart = AT.SIZE_CHARTS[m.sex === 'male' ? 'male' : 'female'];
  const sizes = AT.asArray(doc.sizes).length ? AT.asArray(doc.sizes) : chart.map(function (c) { return c.name; });
  const baseSpec = chart.find(function (c) { return c.name === (doc.baseSize || 'M'); }) || chart[Math.floor(chart.length / 2)];
  const shapesFor = function (spec) { const mm = Object.assign({}, m, spec); delete mm.name; return AT.abaShapes(mm, AT.torsoProfile(mm), { slit: opts.slit }); };
  const b0 = shapesFor(baseSpec), gFB = {}, gBK = {};
  chart.forEach(function (c) {
    if (c.name === baseSpec.name || sizes.indexOf(c.name) < 0) return;
    const s = shapesFor(c);
    [[s.FB, b0.FB, gFB], [s.BK, b0.BK, gBK]].forEach(function (z) { z[0].forEach(function (p, i) { const g = z[2][p.id] = z[2][p.id] || {}; g[c.name] = { dx: Math.round((p.x - z[1][i].x) * 10) / 10, dy: Math.round((p.y - z[1][i].y) * 10) / 10 }; }); });
  });
  FB.grade = gFB; BK.grade = gBK; FA.grade = {};
  Object.keys(gFB).forEach(function (id) { const o = {}; Object.keys(gFB[id]).forEach(function (sz) { o[sz] = { dx: -gFB[id][sz].dx, dy: gFB[id][sz].dy }; }); FA.grade['m' + id] = o; });
  if (!AT.asArray(doc.sizes).length) { doc.sizes = sizes; doc.baseSize = baseSpec.name; doc.activeSize = baseSpec.name; }
  doc.techpack = doc.techpack || {};
  const note = 'Aba: sleeveless open cloak in light wool, worn over the robe and 60 mm shorter. Straight sides (cloth fold) closed from the arm slits to the hem; 360 mm arm slits at the top of each side; long shoulder line sloping 40° with the arm; open front, no closure; narrow braid along the neck and front edges.';
  doc.techpack.notes = doc.techpack.notes ? (doc.techpack.notes.indexOf('Aba:') >= 0 ? doc.techpack.notes : doc.techpack.notes + '\n' + note) : note;
  if (doc.meta && doc.meta.name === 'Untitled garment') doc.meta.name = 'عبا';
  return { pieces: [FB.id, FA.id, BK.id], sewing: sew };
};
AT.TEMPLATES.splice(AT.TEMPLATES.length - 1, 0, { id: 'aba', name: 'Aba', desc: 'Sleeveless open clerical cloak with arm slits', fn: 'draftAba', sex: 'male' });

/* ---------- WebGPU cloth solver ----------
   Every simulation frame runs as compute shaders on the GPU: integration, avatar collision, graph-coloured
   (parallel Gauss-Seidel) XPBD stretch, sewing, bending and fasteners, vertex-triangle self-collision and
   collision between pieces and layers through a GPU spatial hash, skin-vertex vs cloth-triangle collision,
   grab, pins, per-piece gravity hold and kinetic energy. The CPU builds the simulation (meshes, constraints,
   arrangement) and receives the positions once per frame for drawing, picking and fit analysis.
   AT.step stays as the fallback when WebGPU is not available. */
AT.GPU = { state: 'idle', device: null, info: null, error: null, pipes: null, SCALE: 4096, HS: 65536, SKIN_HS: 65536 };
AT.GPU.WG = 128;
AT.GPU.HEADER = `
struct FP {
  h: f32, ih2: f32, gravity: f32, air: f32,
  maxSew: f32, vmax: f32, H0: f32, maxStep: f32,
  gx0: f32, gy0: f32, gz0: f32, gg: f32,
  gnx: u32, gny: u32, gnz: u32, N: u32,
  nTri: u32, nSkin: u32, HS: u32, cap: u32,
  cellC: f32, R: f32, entCap: u32, nSew: u32,
  skinC: f32, skinHS: u32, nPin: u32, nGrab: u32,
  tx: f32, ty: f32, tz: f32, hasP: u32,
  nPc: u32, nTriG: u32, layerOn: u32, hxMax: f32,
  edgeOn: u32, fricScale: f32, fricBand: f32, layerRepair: u32,
  repairMargin: f32, pad4: u32, fricMode: u32, muCap: f32,
};
struct Con { i: u32, j: u32, L: f32, r: f32, cm: f32, kind: u32, p0: u32, p1: u32 };
struct DP { off: u32, cnt: u32, kind: u32, flag: u32 };
const SCALE: f32 = 4096.0;
@group(0) @binding(0) var<uniform> F: FP;
`;
AT.GPU.FN_GRID = `
fn gsamp(p: vec3<f32>) -> vec4<f32> {
  let fx = (p.x - F.gx0) / F.gg; let fy = (p.y - F.gy0) / F.gg; let fz = (p.z - F.gz0) / F.gg;
  if (fx < 0.0 || fy < 0.0 || fz < 0.0 || fx >= f32(F.gnx - 1u) || fy >= f32(F.gny - 1u) || fz >= f32(F.gnz - 1u)) { return vec4<f32>(0.0, 0.0, 0.0, 1000.0); }
  let i = u32(fx); let j = u32(fy); let k = u32(fz);
  let u = fx - f32(i); let v = fy - f32(j); let w = fz - f32(k);
  let nx = F.gnx; let nxy = F.gnx * F.gny; let o = i + nx * j + nxy * k;
  let c000 = GRID[o]; let c100 = GRID[o + 1u]; let c010 = GRID[o + nx]; let c110 = GRID[o + nx + 1u];
  let c001 = GRID[o + nxy]; let c101 = GRID[o + nxy + 1u]; let c011 = GRID[o + nxy + nx]; let c111 = GRID[o + nxy + nx + 1u];
  let x00 = c000 + (c100 - c000) * u; let x10 = c010 + (c110 - c010) * u; let x01 = c001 + (c101 - c001) * u; let x11 = c011 + (c111 - c011) * u;
  let y0 = x00 + (x10 - x00) * v; let y1 = x01 + (x11 - x01) * v;
  let d = y0 + (y1 - y0) * w;
  let dx0 = (c100 - c000) + ((c110 - c010) - (c100 - c000)) * v; let dx1 = (c101 - c001) + ((c111 - c011) - (c101 - c001)) * v;
  return vec4<f32>((dx0 + (dx1 - dx0) * w) / F.gg, ((x10 - x00) + ((x11 - x01) - (x10 - x00)) * w) / F.gg, (y1 - y0) / F.gg, d);
}
`;
AT.GPU.FN_HASH = `
fn hk(i: i32, j: i32, k: i32) -> u32 { return ((bitcast<u32>(i) * 73856093u) ^ (bitcast<u32>(j) * 19349663u) ^ (bitcast<u32>(k) * 83492791u)) & (F.HS - 1u); }
fn hks(i: i32, j: i32, k: i32) -> u32 { return ((bitcast<u32>(i) * 73856093u) ^ (bitcast<u32>(j) * 19349663u) ^ (bitcast<u32>(k) * 83492791u)) & (F.skinHS - 1u); }
`;
AT.GPU.FN_CLOSEST = `
fn closestBary(p: vec3<f32>, a: vec3<f32>, b: vec3<f32>, c: vec3<f32>) -> vec4<f32> {
  let ab = b - a; let ac = c - a; let ap = p - a;
  let d1 = dot(ab, ap); let d2 = dot(ac, ap);
  if (d1 <= 0.0 && d2 <= 0.0) { return vec4<f32>(1.0, 0.0, 0.0, 1.0); }
  let bp = p - b; let d3 = dot(ab, bp); let d4 = dot(ac, bp);
  if (d3 >= 0.0 && d4 <= d3) { return vec4<f32>(0.0, 1.0, 0.0, 1.0); }
  let vc = d1 * d4 - d3 * d2;
  if (vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0) { let v = d1 / (d1 - d3); return vec4<f32>(1.0 - v, v, 0.0, 1.0); }
  let cp = p - c; let d5 = dot(ab, cp); let d6 = dot(ac, cp);
  if (d6 >= 0.0 && d5 <= d6) { return vec4<f32>(0.0, 0.0, 1.0, 1.0); }
  let vb = d5 * d2 - d1 * d6;
  if (vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0) { let w = d2 / (d2 - d6); return vec4<f32>(1.0 - w, 0.0, w, 1.0); }
  let va = d3 * d6 - d5 * d4;
  if (va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0) { let w = (d4 - d3) / ((d4 - d3) + (d5 - d6)); return vec4<f32>(0.0, 1.0 - w, w, 1.0); }
  let dn = 1.0 / (va + vb + vc); let v = vb * dn; let w = vc * dn;
  return vec4<f32>(1.0 - v - w, v, w, 0.0);
}
`;
AT.GPU.FN_DELTA = `
fn addD(v: u32, d: vec3<f32>) {
  atomicAdd(&DELTA[4u * v], i32(round(d.x * SCALE))); atomicAdd(&DELTA[4u * v + 1u], i32(round(d.y * SCALE)));
  atomicAdd(&DELTA[4u * v + 2u], i32(round(d.z * SCALE))); atomicAdd(&DELTA[4u * v + 3u], 1);
}
`;
const GID = '@builtin(global_invocation_id) gid: vec3<u32>';
AT.GPU.PASSES = {
  // ---- per-frame: seam gap reduction and gravity hold (global and per piece)
  gap: { binds: [['X', 'r', 'v4'], ['SW', 'r', 'u4'], ['PD', 'r', 'u4'], ['CTRL', 'w', 'au']], code: `
@compute @workgroup_size(128) fn main(${GID}) {
  let c = gid.x * 2u; if (c >= F.nSew) { return; }
  let s = SW[c]; let u = bitcast<f32>(s.w);
  let L = length((1.0 - u) * X[s.y].xyz + u * X[s.z].xyz - X[s.x].xyz);
  let fx = u32(min(L, 2000.0) * 16.0);
  atomicAdd(&CTRL[1], fx); atomicAdd(&CTRL[2], 1u);
  let pa = min(PD[2u * s.x].x, 63u); let pb = min(PD[2u * s.y].x, 63u);
  atomicAdd(&CTRL[4u + pa], fx); atomicAdd(&CTRL[68u + pa], 1u);
  if (pb != pa) { atomicAdd(&CTRL[4u + pb], fx); atomicAdd(&CTRL[68u + pb], 1u); }
}` },
  grav: { wg: 1, binds: [['CTRL', 'w', 'u1'], ['ST', 'w', 'f1']], code: `
@compute @workgroup_size(1) fn main(${GID}) {
  var gap = 0.0; if (CTRL[2] > 0u) { gap = f32(CTRL[1]) / 16.0 / f32(CTRL[2]); }
  let tg = clamp(1.0 - (gap - 6.0) / 30.0, 0.03, 1.0);
  var gsc = ST[0];
  if (gsc < 0.0 || tg < gsc) { gsc = tg; } else { gsc = gsc + (tg - gsc) * 0.04; }
  ST[0] = gsc; ST[1] = gap;
  for (var r = 0u; r < min(F.nPc, 64u); r = r + 1u) {
    var gp = 0.0; if (CTRL[68u + r] > 0u) { gp = f32(CTRL[4u + r]) / 16.0 / f32(CTRL[68u + r]); }
    let t2 = clamp(1.0 - (gp - 6.0) / 30.0, 0.03, 1.0);
    var g0 = ST[8u + r]; if (t2 < g0) { g0 = t2; } else { g0 = g0 + (t2 - g0) * 0.04; }
    ST[8u + r] = g0; ST[80u + r] = -F.gravity * min(gsc, g0);
    CTRL[4u + r] = 0u; CTRL[68u + r] = 0u;
  }
  CTRL[1] = 0u; CTRL[2] = 0u;
}` },
  // ---- pressure normals (only when a piece has pressure)
  nclear: { binds: [['NRM', 'w', 'i1']], code: `
@compute @workgroup_size(128) fn main(${GID}) { let i = gid.x; if (i >= F.N * 4u) { return; } NRM[i] = 0; }` },
  nacc: { binds: [['X', 'r', 'v4'], ['TRI', 'r', 'u4'], ['NRM', 'w', 'ai']], code: `
@compute @workgroup_size(128) fn main(${GID}) {
  let t = gid.x; if (t >= F.nTri) { return; }
  let tr = TRI[t]; let a = X[tr.x].xyz; let n = cross(X[tr.y].xyz - a, X[tr.z].xyz - a) * 64.0;
  let ni = vec3<i32>(round(n));
  for (var e = 0u; e < 3u; e = e + 1u) { let v = select(select(tr.z, tr.y, e == 1u), tr.x, e == 0u);
    atomicAdd(&NRM[4u * v], ni.x); atomicAdd(&NRM[4u * v + 1u], ni.y); atomicAdd(&NRM[4u * v + 2u], ni.z); }
}` },
  // ---- substep: integrate
  integrate: { binds: [['X', 'w', 'v4'], ['P', 'w', 'v4'], ['V', 'w', 'v4'], ['PP', 'r', 'v4'], ['PD', 'r', 'u4'], ['ST', 'r', 'f1'], ['NRM', 'r', 'i1']], code: `
@compute @workgroup_size(128) fn main(${GID}) {
  let q = gid.x; if (q >= F.N) { return; }
  let x = X[q];
  if (x.w == 0.0) { P[q] = vec4<f32>(x.xyz, 0.0); return; }
  var v = V[q].xyz;
  let dmp = max(0.0, 1.0 - (PP[q].x + F.air) * F.h);
  let pc = min(PD[2u * q].x, 63u);
  v = vec3<f32>(v.x * dmp, (v.y + ST[80u + pc] * F.h) * dmp, v.z * dmp);
  if (F.hasP != 0u) {
    let pr = bitcast<f32>(PD[2u * q + 1u].w);
    if (pr != 0.0) { let nn = vec3<f32>(f32(NRM[4u * q]), f32(NRM[4u * q + 1u]), f32(NRM[4u * q + 2u])); let L = length(nn); if (L > 0.0) { v = v + nn / L * (pr * 40.0 * F.h); } }
  }
  let sp = length(v); if (sp > F.vmax) { v = v * (F.vmax / sp); }
  P[q] = vec4<f32>(x.xyz, 0.0); V[q] = vec4<f32>(v, 0.0);
  X[q] = vec4<f32>(x.xyz + v * F.h, x.w);
}` },
  // ---- avatar (SDF) and floor collision with friction
  body: { grid: true, binds: [['X', 'w', 'v4'], ['P', 'r', 'v4'], ['PP', 'r', 'v4'], ['GRID', 'r', 'f1']], code: `
@compute @workgroup_size(128) fn main(${GID}) {
  let q = gid.x; if (q >= F.N) { return; }
  let x = X[q]; if (x.w == 0.0) { return; }
  let m = PP[q].y; let pr = P[q].xyz; var pos = x.xyz;
  let s = gsamp(pos);
  if (s.w < m) {
    let L0 = length(s.xyz); let n = s.xyz / select(L0, 1.0, L0 == 0.0);
    pos = pos + n * (m - s.w);
    let dd = pos - pr; let t = dd - dot(dd, n) * n;
    pos = pos - t * PP[q].z;
  }
  if (pos.y < 1.0 + m * 0.3) { pos = vec3<f32>(pr.x + (pos.x - pr.x) * 0.5, 1.0 + m * 0.3, pr.z + (pos.z - pr.z) * 0.5); }
  X[q] = vec4<f32>(pos, x.w);
}` },
  // ---- one colour of distance constraints: stretch (XPBD), bending (unilateral / fold), fasteners
  dist: { useD: true, binds: [['X', 'w', 'v4'], ['CON', 'r', 'con']], code: `
@compute @workgroup_size(128) fn main(${GID}) {
  if (gid.x >= D.cnt) { return; }
  let c = CON[D.off + gid.x];
  let xi = X[c.i]; let xj = X[c.j]; let wi = xi.w; let wj = xj.w; let ws = wi + wj;
  if (ws == 0.0) { return; }
  let d = xj.xyz - xi.xyz; let L = length(d);
  var dl = 0.0;
  if (c.kind == 0u) {
    if (L < 1e-9) { return; }
    let C = L - c.L; let al = c.r * select(1.0, c.cm, C < 0.0) * F.ih2;
    dl = -C / (ws + al) / L;
  } else if (c.kind == 1u) {
    if (L < 1e-9) { return; }
    let C = L - c.L; if (c.r >= 0.0 && C > 0.0) { return; }
    dl = -C / (ws + abs(c.r) * ws) / L;
  } else {
    if (L < 2.0) { return; }
    dl = -min(L - 2.0, F.maxSew) / L / ws;
  }
  X[c.i] = vec4<f32>(xi.xyz - wi * dl * d, wi);
  X[c.j] = vec4<f32>(xj.xyz + wj * dl * d, wj);
}` },
  // ---- one colour of sewing constraints (rate-limited while approaching, hard once closed)
  sew: { useD: true, binds: [['X', 'w', 'v4'], ['SW', 'r', 'u4']], code: `
@compute @workgroup_size(128) fn main(${GID}) {
  if (gid.x >= D.cnt) { return; }
  let s = SW[D.off + gid.x]; let u = bitcast<f32>(s.w); let u0 = 1.0 - u;
  let xa = X[s.x]; let x0 = X[s.y]; let x1 = X[s.z];
  let den = xa.w + u0 * u0 * x0.w + u * u * x1.w; if (den == 0.0) { return; }
  let d = u0 * x0.xyz + u * x1.xyz - xa.xyz; let L = length(d); if (L < 1e-6) { return; }
  var C = L; if (L > 10.0) { C = min(L, max(10.0, F.maxSew + L * 0.02)); }
  let s2 = C / L / den;
  X[s.x] = vec4<f32>(xa.xyz + xa.w * s2 * d, xa.w);
  X[s.y] = vec4<f32>(x0.xyz - x0.w * u0 * s2 * d, x0.w);
  X[s.z] = vec4<f32>(x1.xyz - x1.w * u * s2 * d, x1.w);
}` },
  grab: { binds: [['X', 'w', 'v4'], ['GR', 'r', 'v4']], code: `
@compute @workgroup_size(128) fn main(${GID}) {
  let e = gid.x; if (e >= F.nGrab) { return; }
  let g = GR[2u * e]; let q = bitcast<u32>(GR[2u * e + 1u].x);
  let x = X[q]; let tgt = vec3<f32>(F.tx, F.ty, F.tz) + g.xyz;
  X[q] = vec4<f32>(x.xyz + (tgt - x.xyz) * g.w, x.w);
}` },
  guard: { grid: true, binds: [['X', 'w', 'v4'], ['PP', 'r', 'v4'], ['GRID', 'r', 'f1']], code: `
@compute @workgroup_size(128) fn main(${GID}) {
  let q = gid.x; if (q >= F.N) { return; }
  let x = X[q]; if (x.w == 0.0) { return; }
  var pos = x.xyz; let s = gsamp(pos); let m2 = PP[q].y * 0.35;
  if (s.w < m2) { let L0 = length(s.xyz); pos = pos + s.xyz / select(L0, 1.0, L0 == 0.0) * (m2 - s.w); }
  if (pos.y < 1.0) { pos.y = 1.0; }
  X[q] = vec4<f32>(pos, x.w);
}` },
  // ---- skin vertices vs cloth triangles: keeps the cloth surface (not only its particles) clear of the body
  bodytri: { delta: true, hash: true, binds: [['X', 'r', 'v4'], ['TRI', 'r', 'u4'], ['PP', 'r', 'v4'], ['SKIN', 'r', 'v4'], ['SHEAD', 'r', 'i1'], ['SNEXT', 'r', 'i1'], ['DELTA', 'w', 'ai']], code: `
@compute @workgroup_size(128) fn main(${GID}) {
  let t = gid.x; if (t >= F.nTri) { return; }
  let tr = TRI[t]; let xa = X[tr.x]; let xb = X[tr.y]; let xc = X[tr.z];
  if (xa.w == 0.0 && xb.w == 0.0 && xc.w == 0.0) { return; }
  let C = F.skinC;
  let i0 = vec3<i32>(floor(min(min(xa.xyz, xb.xyz), xc.xyz) / C)); let i1 = vec3<i32>(floor(max(max(xa.xyz, xb.xyz), xc.xyz) / C));
  let nc = (i1 - i0) + vec3<i32>(1); if (nc.x * nc.y * nc.z > 64) { return; }
  let m = min(min(PP[tr.x].y, PP[tr.y].y), PP[tr.z].y) * 0.8;
  let e1 = xb.xyz - xa.xyz; let e2 = xc.xyz - xa.xyz;
  var best = 0.0; var bn = vec3<f32>(0.0); var bb = vec3<f32>(0.0);
  for (var i = i0.x; i <= i1.x; i = i + 1) { for (var j = i0.y; j <= i1.y; j = j + 1) { for (var k = i0.z; k <= i1.z; k = k + 1) {
    var v = SHEAD[hks(i, j, k)];
    var walk = 0;
    loop {
      if (v < 0 || walk > 4096) { break; }
      walk = walk + 1;
      let sp = SKIN[2u * u32(v)].xyz; let sn = SKIN[2u * u32(v) + 1u].xyz;
      let cv = vec3<i32>(floor(sp / C));
      if (cv.x == i && cv.y == j && cv.z == k) {
        let h3 = cross(sn, e2); let det = dot(e1, h3);
        if (abs(det) > 1e-6) {
          let inv = 1.0 / det; let s = sp - xa.xyz; let u = dot(s, h3) * inv;
          if (u >= -0.02 && u <= 1.02) {
            let qv = cross(s, e1); let vv = dot(sn, qv) * inv;
            if (vv >= -0.02 && u + vv <= 1.02) {
              let sd = dot(e2, qv) * inv;
              if (sd < m && sd >= -14.0 && m - sd > best) { best = m - sd; bn = sn; bb = vec3<f32>(max(0.0, 1.0 - u - vv), max(0.0, u), max(0.0, vv)); }
            }
          }
        }
      }
      v = SNEXT[u32(v)];
    }
  } } }
  if (best <= 0.0) { return; }
  let wa = select(0.0, bb.x, xa.w != 0.0); let wb = select(0.0, bb.y, xb.w != 0.0); let wc = select(0.0, bb.z, xc.w != 0.0);
  let den = wa * wa + wb * wb + wc * wc; if (den < 1e-6) { return; }
  let f = best / den;
  if (wa > 0.0) { addD(tr.x, bn * f * wa); }
  if (wb > 0.0) { addD(tr.y, bn * f * wb); }
  if (wc > 0.0) { addD(tr.z, bn * f * wc); }
}` },
  // ---- apply accumulated (averaged) corrections; flag 1 also shifts the previous position (no added velocity)
  apply: { useD: true, binds: [['X', 'w', 'v4'], ['P', 'w', 'v4'], ['DELTA', 'w', 'ai']], code: `
@compute @workgroup_size(128) fn main(${GID}) {
  let q = gid.x; if (q >= F.N) { return; }
  let c = atomicExchange(&DELTA[4u * q + 3u], 0);
  if (c == 0) { return; }
  let sx = atomicExchange(&DELTA[4u * q], 0); let sy = atomicExchange(&DELTA[4u * q + 1u], 0); let sz = atomicExchange(&DELTA[4u * q + 2u], 0);
  let d = vec3<f32>(f32(sx), f32(sy), f32(sz)) / (SCALE * f32(c));
  let x = X[q]; X[q] = vec4<f32>(x.xyz + d, x.w);
  if (D.flag == 1u) { let p = P[q]; P[q] = vec4<f32>(p.xyz + d, p.w); }
}` },
  pins: { binds: [['X', 'w', 'v4'], ['PIN', 'r', 'v4']], code: `
@compute @workgroup_size(128) fn main(${GID}) {
  let e = gid.x; if (e >= F.nPin) { return; }
  let pn = PIN[e]; let q = bitcast<u32>(pn.w); X[q] = vec4<f32>(pn.xyz, X[q].w);
}` },
  vel: { binds: [['X', 'r', 'v4'], ['P', 'r', 'v4'], ['V', 'w', 'v4']], code: `
@compute @workgroup_size(128) fn main(${GID}) {
  let q = gid.x; if (q >= F.N) { return; }
  V[q] = vec4<f32>((X[q].xyz - P[q].xyz) / F.h, 0.0);
}` },
  // ---- GPU spatial hash of cloth triangles (counting sort)
  hclear: { binds: [['HC', 'w', 'u1'], ['CTRL', 'w', 'u1']], code: `
@compute @workgroup_size(128) fn main(${GID}) { let i = gid.x; if (i < F.HS) { HC[i] = 0u; } if (i == 0u) { CTRL[0] = 0u; } }` },
  hcount: { hash: true, binds: [['X', 'r', 'v4'], ['TRI', 'r', 'u4'], ['HC', 'w', 'au']], code: `
@compute @workgroup_size(128) fn main(${GID}) {
  let t = gid.x; if (t >= F.nTriG) { return; }
  let tr = TRI[t]; let a = X[tr.x].xyz; let b = X[tr.y].xyz; let c = X[tr.z].xyz;
  let i0 = vec3<i32>(floor((min(min(a, b), c) - vec3<f32>(F.R)) / F.cellC)); let i1 = vec3<i32>(floor((max(max(a, b), c) + vec3<f32>(F.R)) / F.cellC));
  let nc = (i1 - i0) + vec3<i32>(1); if (nc.x * nc.y * nc.z > 64) { return; }
  for (var i = i0.x; i <= i1.x; i = i + 1) { for (var j = i0.y; j <= i1.y; j = j + 1) { for (var k = i0.z; k <= i1.z; k = k + 1) { atomicAdd(&HC[hk(i, j, k)], 1u); } } }
}` },
  scan1: { wg: 256, binds: [['HC', 'r', 'u1'], ['HASH', 'w', 'u1'], ['BS', 'w', 'u1']], code: `
var<workgroup> sh: array<u32, 256>;
@compute @workgroup_size(256) fn main(${GID}, @builtin(local_invocation_id) lid: vec3<u32>, @builtin(workgroup_id) wid: vec3<u32>) {
  let i = gid.x; var v = 0u; if (i < F.HS) { v = HC[i]; }
  sh[lid.x] = v; workgroupBarrier();
  for (var off = 1u; off < 256u; off = off * 2u) { var t = 0u; if (lid.x >= off) { t = sh[lid.x - off]; } workgroupBarrier(); sh[lid.x] = sh[lid.x] + t; workgroupBarrier(); }
  if (i < F.HS) { HASH[i] = sh[lid.x] - v; }
  if (lid.x == 255u) { BS[wid.x] = sh[255]; }
}` },
  scan2: { wg: 256, binds: [['BS', 'w', 'u1']], code: `
var<workgroup> sh: array<u32, 256>;
@compute @workgroup_size(256) fn main(${GID}, @builtin(local_invocation_id) lid: vec3<u32>) {
  let v = BS[lid.x]; sh[lid.x] = v; workgroupBarrier();
  for (var off = 1u; off < 256u; off = off * 2u) { var t = 0u; if (lid.x >= off) { t = sh[lid.x - off]; } workgroupBarrier(); sh[lid.x] = sh[lid.x] + t; workgroupBarrier(); }
  BS[lid.x] = sh[lid.x] - v;
  if (lid.x == 255u) { BS[256] = sh[255]; }
}` },
  scan3: { binds: [['HC', 'w', 'u1'], ['HASH', 'w', 'u1'], ['BS', 'r', 'u1']], code: `
@compute @workgroup_size(128) fn main(${GID}) {
  let i = gid.x; if (i >= F.HS) { return; }
  let s = HASH[i] + BS[i / 256u]; HASH[i] = s; HC[i] = s;
  if (i == F.HS - 1u) { HASH[F.HS] = BS[256]; }
}` },
  scatter: { hash: true, binds: [['X', 'r', 'v4'], ['TRI', 'r', 'u4'], ['HC', 'w', 'au'], ['HASH', 'w', 'u1']], code: `
@compute @workgroup_size(128) fn main(${GID}) {
  let t = gid.x; if (t >= F.nTriG) { return; }
  let tr = TRI[t]; let a = X[tr.x].xyz; let b = X[tr.y].xyz; let c = X[tr.z].xyz;
  let i0 = vec3<i32>(floor((min(min(a, b), c) - vec3<f32>(F.R)) / F.cellC)); let i1 = vec3<i32>(floor((max(max(a, b), c) + vec3<f32>(F.R)) / F.cellC));
  let nc = (i1 - i0) + vec3<i32>(1); if (nc.x * nc.y * nc.z > 64) { return; }
  for (var i = i0.x; i <= i1.x; i = i + 1) { for (var j = i0.y; j <= i1.y; j = j + 1) { for (var k = i0.z; k <= i1.z; k = k + 1) {
    let slot = atomicAdd(&HC[hk(i, j, k)], 1u); if (slot < F.entCap) { HASH[F.HS + 1u + slot] = t; }
  } } }
}` },
  // ---- vertex-triangle contact detection (self-collision included); side kept from the previous position,
  //      or fixed by layer order (higher layer outside along the body normal)
  detect: { grid: true, hash: true, closest: true, binds: [['X', 'r', 'v4'], ['P', 'r', 'v4'], ['PD', 'r', 'u4'], ['TRI', 'r', 'u4'], ['HASH', 'r', 'u1'], ['CONT', 'w', 'u4'], ['CTRL', 'w', 'au'], ['GRID', 'r', 'f1']], code: `
@compute @workgroup_size(128) fn main(${GID}) {
  let q = gid.x; if (q >= F.N) { return; }
  let pdq = PD[2u * q]; if ((pdq.z & 1u) == 0u) { return; }
  let tang = pdq.z >> 8u; let tangWant = select(0u, 1u, (pdq.z & 2u) != 0u);
  let xq = X[q]; let pos = xq.xyz; let wq = xq.w;
  let ci = vec3<i32>(floor(pos / F.cellC)); let h = hk(ci.x, ci.y, ci.z);
  let e0 = HASH[h]; let e1 = min(HASH[h + 1u], F.entCap);
  let rq = PD[2u * q + 1u]; let rxq = bitcast<f32>(rq.x); let ryq = bitcast<f32>(rq.y);
  let exq = 2.2 * select(bitcast<f32>(rq.z), 20.0, bitcast<f32>(rq.z) <= 0.0); let ex2 = exq * exq;
  let pq = pdq.x; let layq = pdq.y; let mq = pdq.w; let pprev = P[q].xyz;
  for (var e = e0; e < e1; e = e + 1u) {
    let t = HASH[F.HS + 1u + e]; let tr = TRI[t];
    if (tr.x == q || tr.y == q || tr.z == q) { continue; }
    let xa = X[tr.x]; let xb = X[tr.y]; let xc = X[tr.z];
    if (wq == 0.0 && xa.w == 0.0 && xb.w == 0.0 && xc.w == 0.0) { continue; }
    let pt = tr.w;
    if (pt == pq) {
      let ra = PD[2u * tr.x + 1u]; let rb = PD[2u * tr.y + 1u]; let rc = PD[2u * tr.z + 1u];
      let da = vec2<f32>(bitcast<f32>(ra.x) - rxq, bitcast<f32>(ra.y) - ryq); if (dot(da, da) < ex2) { continue; }
      let db = vec2<f32>(bitcast<f32>(rb.x) - rxq, bitcast<f32>(rb.y) - ryq); if (dot(db, db) < ex2) { continue; }
      let dc = vec2<f32>(bitcast<f32>(rc.x) - rxq, bitcast<f32>(rc.y) - ryq); if (dot(dc, dc) < ex2) { continue; }
    }
    if (mq != 0u && (mq & (1u << (pt & 31u))) != 0u && (PD[2u * tr.x].w != 0u || PD[2u * tr.y].w != 0u || PD[2u * tr.z].w != 0u)) { continue; }
    let e1v = xb.xyz - xa.xyz; let e2v = xc.xyz - xa.xyz;
    var n = cross(e1v, e2v); let nl = length(n); if (nl < 1e-9) { continue; }
    n = n / nl;
    let laya = PD[2u * tr.x].y;
    let layered = layq != laya;
    var bu = 0.0; var bv = 0.0; var bw = 0.0; var dist = 0.0; var sd = 0.0;
    if (layered && F.edgeOn != 0u) { // closest point on the triangle (edges and corners too): no gaps along ridges between layers
      let cb = closestBary(pos, xa.xyz, xb.xyz, xc.xyz); bu = cb.x; bv = cb.y; bw = cb.z;
      let cpt = bu * xa.xyz + bv * xb.xyz + bw * xc.xyz; dist = length(pos - cpt); sd = dot(pos - cpt, n);
    } else { // same layer: face region only (stable for folds of one garment)
      let r = pos - xa.xyz; sd = dot(r, n); dist = abs(sd);
      let d00 = dot(e1v, e1v); let d01 = dot(e1v, e2v); let d11 = dot(e2v, e2v); let d20 = dot(r, e1v); let d21 = dot(r, e2v);
      let den = d00 * d11 - d01 * d01; if (den < 1e-9) { continue; }
      bv = (d11 * d20 - d01 * d21) / den; bw = (d00 * d21 - d01 * d20) / den; bu = 1.0 - bv - bw;
      if (bu < -0.08 || bv < -0.08 || bw < -0.08) { continue; }
    }
    if (dist > F.R) { continue; }
    var side = 1u;
    if (layered && F.layerOn != 0u) {
      let gc = gsamp((xa.xyz + xb.xyz + xc.xyz) / 3.0);
      let outward = dot(gc.xyz, n) >= 0.0;
      if (layq > laya) { side = select(0u, 1u, outward); } else { side = select(1u, 0u, outward); }
    } else {
      let sp = dot(pprev - (bu * P[tr.x].xyz + bv * P[tr.y].xyz + bw * P[tr.z].xyz), n);
      let s0 = select(sd, sp, abs(sp) > 1e-6);
      side = select(0u, 1u, s0 >= 0.0);
      if (layered && F.layerRepair != 0u) {
        // repair only clear crossings: the outer layer is nearer the body than the inner surface it touches
        let cpt2 = bu * xa.xyz + bv * xb.xyz + bw * xc.xyz;
        let gq = gsamp(pos); let gt = gsamp(cpt2);
        let outerNearer = (gq.w) < (gt.w - F.repairMargin); let innerFarther = (gq.w) > (gt.w + F.repairMargin);
        let crossed = select(innerFarther, outerNearer, (layq > laya));
        let gl = length(gt.xyz);
        if (crossed && gq.w < 500.0 && gt.w < 500.0 && gl > 1e-6) {
          let ng = dot(gt.xyz / gl, n);
          if (abs(ng) > 0.35) { let outw = ng > 0.0; if (layq > laya) { side = select(0u, 1u, outw); } else { side = select(1u, 0u, outw); } }
        }
      }
    }
    // untangling: this vertex was found on the wrong side of this piece (it poked through); push it back
    var untg = 0u;
    if (tang != 0u && tang - 1u == pt) { side = tangWant; untg = 4u; }
    var hx = 0.0;
    if (layq != laya) { let pa = bitcast<f32>(PD[2u * tr.x + 1u].z); hx = 0.3 * max(select(bitcast<f32>(rq.z), 20.0, bitcast<f32>(rq.z) <= 0.0), pa); }
    if (dist > F.H0 + 2.0 + hx + (F.R - F.H0 - 2.0 - F.hxMax)) { continue; }
    let idx = atomicAdd(&CTRL[0], 1u);
    if (idx < F.cap) { CONT[idx] = vec4<u32>(q, t, side | select(0u, 2u, layered && F.edgeOn != 0u) | untg, bitcast<u32>(hx)); }
  }
}` },
  // ---- resolve contacts (every substep): keep the thickness on the wanted side, plus friction
  resolve: { delta: true, closest: true, binds: [['X', 'r', 'v4'], ['P', 'r', 'v4'], ['PP', 'r', 'v4'], ['CONT', 'r', 'u4'], ['CTRL', 'r', 'u1'], ['TRI', 'r', 'u4'], ['DELTA', 'w', 'ai']], code: `
@compute @workgroup_size(128) fn main(${GID}) {
  let e = gid.x; if (e >= min(CTRL[0], F.cap)) { return; }
  let ct = CONT[e]; let q = ct.x; let tr = TRI[ct.y]; let side = select(-1.0, 1.0, (ct.z & 1u) == 1u); let edgeMode = (ct.z & 2u) != 0u;
  let xq = X[q]; let xa = X[tr.x]; let xb = X[tr.y]; let xc = X[tr.z];
  let e1v = xb.xyz - xa.xyz; let e2v = xc.xyz - xa.xyz;
  var n = cross(e1v, e2v); let nl = length(n); if (nl < 1e-9) { return; }
  n = n / nl;
  var cb = vec4<f32>(0.0);
  if (edgeMode) { cb = closestBary(xq.xyz, xa.xyz, xb.xyz, xc.xyz); }
  else {
    let r = xq.xyz - xa.xyz;
    let d00 = dot(e1v, e1v); let d01 = dot(e1v, e2v); let d11 = dot(e2v, e2v); let d20 = dot(r, e1v); let d21 = dot(r, e2v);
    let den0 = d00 * d11 - d01 * d01; if (den0 < 1e-9) { return; }
    var pv = (d11 * d20 - d01 * d21) / den0; var pw = (d00 * d21 - d01 * d20) / den0; var pu = 1.0 - pv - pw;
    if (pu < -0.12 || pv < -0.12 || pw < -0.12) { return; }
    pu = max(pu, 0.0); pv = max(pv, 0.0); pw = max(pw, 0.0); let ps = pu + pv + pw;
    cb = vec4<f32>(pu / ps, pv / ps, pw / ps, 0.0);
  }
  let bu = cb.x; let bv = cb.y; let bw = cb.z;
  let h = F.H0 + 0.5 * (PP[q].w + (PP[tr.x].w + PP[tr.y].w + PP[tr.z].w) / 3.0) + bitcast<f32>(ct.w);
  let dv = xq.xyz - (bu * xa.xyz + bv * xb.xyz + bw * xc.xyz);
  let sN = side * dot(dv, n);
  var dirn = n * side; var sCur = sN;
  if (cb.w > 0.5 && sN >= 0.0) { let dl = length(dv); if (dl > 1e-6) { dirn = dv / dl; sCur = dl; } }
  if (sCur >= h) { return; } // not touching: no push and no friction (cloth that is merely close must stay free to slide apart)
  let wq = xq.w; let wa = xa.w * bu; let wb = xb.w * bv; let wc = xc.w * bw;
  let den = wq + wa * bu + wb * bv + wc * bw; if (den <= 0.0) { return; }
  let dn = min(h - sCur, select(F.maxStep, F.maxStep * 0.3, (ct.z & 4u) != 0u)); // untangling pushes gently
  let f = dirn * (dn / den);
  var dq = f * wq; var da = -f * wa; var db = -f * wb; var dc = -f * wc;
  // friction only while touching. fricMode 0: Coulomb (tangential correction bounded by mu x the normal correction);
  // fricMode 1: remove a fraction mu of this substep's relative sliding (capped by muCap)
  let mu = min(F.muCap, (PP[q].z + PP[tr.x].z) * 0.5 * F.fricScale);
  if (mu > 0.0) {
    var tv = (xq.xyz + dq - P[q].xyz) - (bu * (xa.xyz + da - P[tr.x].xyz) + bv * (xb.xyz + db - P[tr.y].xyz) + bw * (xc.xyz + dc - P[tr.z].xyz));
    tv = tv - dot(tv, dirn) * dirn;
    let tl = length(tv);
    if (tl > 1e-9) {
      var k = mu;
      if (F.fricMode != 1u) { var kc = 1.0; if (tl > mu * dn) { kc = min(1.0, 0.8 * mu * dn / tl); } k = select(kc, max(kc, 0.35 * mu), F.fricMode == 2u); }
      let f2 = k / den;
      dq = dq - tv * f2 * wq; da = da + tv * f2 * wa; db = db + tv * f2 * wb; dc = dc + tv * f2 * wc;
    }
  }
  if (wq > 0.0) { addD(q, dq); }
  if (wa > 0.0) { addD(tr.x, da); }
  if (wb > 0.0) { addD(tr.y, db); }
  if (wc > 0.0) { addD(tr.z, dc); }
}` },
  // ---- kinetic energy partial sums (auto-stop)
  energy: { binds: [['X', 'r', 'v4'], ['V', 'r', 'v4'], ['PART', 'w', 'v2']], code: `
var<workgroup> se: array<vec2<f32>, 128>;
@compute @workgroup_size(128) fn main(${GID}, @builtin(local_invocation_id) lid: vec3<u32>, @builtin(workgroup_id) wid: vec3<u32>) {
  let q = gid.x; var s = vec2<f32>(0.0);
  if (q < F.N) { if (X[q].w > 0.0) { let v = V[q].xyz; s = vec2<f32>(dot(v, v), 1.0); } }
  se[lid.x] = s; workgroupBarrier();
  for (var off = 64u; off > 0u; off = off / 2u) { if (lid.x < off) { se[lid.x] = se[lid.x] + se[lid.x + off]; } workgroupBarrier(); }
  if (lid.x == 0u) { PART[wid.x] = se[0]; }
}` }
};

AT.GPU.TYPES = { v4: 'array<vec4<f32>>', u4: 'array<vec4<u32>>', f1: 'array<f32>', u1: 'array<u32>', i1: 'array<i32>', au: 'array<atomic<u32>>', ai: 'array<atomic<i32>>', con: 'array<Con>', v2: 'array<vec2<f32>>' };
AT.GPU.init = function () {
  if (AT.GPU.promise) return AT.GPU.promise;
  AT.GPU.state = 'init';
  AT.GPU.promise = (async function () {
    try {
      if (typeof navigator === 'undefined' || !navigator.gpu) throw new Error('WebGPU is not available in this browser');
      const ad = await navigator.gpu.requestAdapter({ powerPreference: 'high-performance' });
      if (!ad) throw new Error('No WebGPU adapter');
      const info = ad.info || (ad.requestAdapterInfo ? await ad.requestAdapterInfo() : {}) || {};
      const want = { maxStorageBufferBindingSize: 1 << 28, maxBufferSize: 1 << 28 };
      const req = {}; Object.keys(want).forEach(function (k) { if (ad.limits[k]) req[k] = Math.min(ad.limits[k], Math.max(want[k], 0)); });
      const dev = await ad.requestDevice({ requiredLimits: req });
      dev.lost.then(function (e) { AT.GPU.state = 'lost'; AT.GPU.error = 'GPU device lost' + (e && e.message ? ': ' + e.message : ''); });
      dev.addEventListener && dev.addEventListener('uncapturederror', function (ev) { console.error('[Atelier WebGPU]', ev.error && ev.error.message); AT.GPU.lastError = ev.error && ev.error.message; });
      AT.GPU.device = dev;
      AT.GPU.info = { vendor: info.vendor || '', architecture: info.architecture || '', device: info.device || '', description: info.description || '', fallback: !!(ad.isFallbackAdapter || info.isFallbackAdapter) };
      AT.GPU.pipes = await AT.GPU.buildPipes(dev);
      AT.GPU.state = 'ready';
    } catch (e) {
      AT.GPU.state = 'failed'; AT.GPU.error = (e && e.message) || String(e);
      if (typeof console !== 'undefined') console.warn('[Atelier] WebGPU unavailable, using the CPU solver:', AT.GPU.error);
    }
    return AT.GPU.state;
  })();
  return AT.GPU.promise;
};
AT.GPU.label = function () {
  const G = AT.GPU;
  if (G.state === 'ready') { const i = G.info || {}; return 'WebGPU · ' + ([i.vendor, i.architecture].filter(Boolean).join(' ') || 'GPU') + (i.fallback ? ' (software)' : ''); }
  if (G.state === 'init' || G.state === 'idle') return 'Starting WebGPU…';
  return 'CPU solver' + (G.error ? ' · ' + G.error : '');
};
AT.GPU.buildPipes = async function (dev) {
  const G = AT.GPU, out = {};
  const names = Object.keys(G.PASSES);
  for (let pi = 0; pi < names.length; pi++) {
    const name = names[pi], ps = G.PASSES[name];
    let code = G.HEADER;
    const entries = [{ binding: 0, visibility: GPUShaderStage.COMPUTE, buffer: { type: 'uniform' } }];
    ps.binds.forEach(function (b, i) {
      code += '@group(0) @binding(' + (i + 1) + ') var<storage, ' + (b[1] === 'w' ? 'read_write' : 'read') + '> ' + b[0] + ': ' + G.TYPES[b[2]] + ';\n';
      entries.push({ binding: i + 1, visibility: GPUShaderStage.COMPUTE, buffer: { type: b[1] === 'w' ? 'storage' : 'read-only-storage' } });
    });
    if (ps.useD) code += '@group(1) @binding(0) var<uniform> D: DP;\n';
    if (ps.grid) code += G.FN_GRID;
    if (ps.hash) code += G.FN_HASH;
    if (ps.delta) code += G.FN_DELTA;
    if (ps.closest) code += G.FN_CLOSEST;
    code += ps.code;
    const bgl = dev.createBindGroupLayout({ entries: entries });
    const layouts = [bgl];
    if (ps.useD) layouts.push(G.dLayout || (G.dLayout = dev.createBindGroupLayout({ entries: [{ binding: 0, visibility: GPUShaderStage.COMPUTE, buffer: { type: 'uniform', hasDynamicOffset: true, minBindingSize: 16 } }] })));
    const module = dev.createShaderModule({ code: code, label: 'atelier-' + name });
    let pipe;
    try { pipe = await dev.createComputePipelineAsync({ layout: dev.createPipelineLayout({ bindGroupLayouts: layouts }), compute: { module: module, entryPoint: 'main' }, label: 'atelier-' + name }); }
    catch (e) {
      let msg = e.message;
      try { const ci = await module.getCompilationInfo(); msg += ' | ' + ci.messages.map(function (m) { return m.lineNum + ':' + m.linePos + ' ' + m.message; }).join(' ; '); } catch (e2) { }
      throw new Error('WGSL pass ' + name + ': ' + msg);
    }
    out[name] = { name: name, pipe: pipe, bgl: bgl, binds: ps.binds, useD: !!ps.useD, wg: ps.wg || G.WG };
  }
  return out;
};
// greedy graph colouring: constraints in one colour share no vertex, so a colour runs fully in parallel
AT.GPU.colorize = function (n, N, vertsOf) {
  const used = new Uint32Array(N * 4), col = new Int16Array(n), counts = [];
  const vs = [0, 0, 0];
  for (let c = 0; c < n; c++) {
    const k = vertsOf(c, vs);
    let m0 = 0, m1 = 0, m2 = 0, m3 = 0;
    for (let e = 0; e < k; e++) { const v = vs[e] * 4; m0 |= used[v]; m1 |= used[v + 1]; m2 |= used[v + 2]; m3 |= used[v + 3]; }
    let cc = 127;
    for (let b = 0; b < 128; b++) { const m = b < 32 ? m0 : b < 64 ? m1 : b < 96 ? m2 : m3; if (!(m & (1 << (b & 31)))) { cc = b; break; } }
    col[c] = cc; counts[cc] = (counts[cc] || 0) + 1;
    for (let e = 0; e < k; e++) { const v = vs[e] * 4 + (cc >> 5); used[v] |= 1 << (cc & 31); }
  }
  const start = [], ranges = []; let o = 0;
  for (let b = 0; b < counts.length; b++) { start[b] = o; if (counts[b]) ranges.push([o, counts[b]]); o += counts[b] || 0; }
  const order = new Int32Array(n), fill = start.slice();
  for (let c = 0; c < n; c++) order[fill[col[c]]++] = c;
  // colour 127 is the overflow bucket: it may contain conflicts, so run it one constraint at a time
  const ov = counts[127] ? [start[127], counts[127]] : null;
  if (ov) { ranges.pop(); for (let i = 0; i < ov[1]; i++) ranges.push([ov[0] + i, 1]); }
  return { order: order, ranges: ranges };
};
AT.GPU.attach = function (sim) {
  const G = AT.GPU, dev = G.device;
  if (G.state !== 'ready' || !dev || !sim || !sim.N) return null;
  if (sim._g) return sim._g;
  const N = sim.N, recs = sim.recs, P = sim.params;
  const mk = function (name, data, extraUsage, size) {
    const sz = Math.max(16, Math.ceil((size || data.byteLength) / 16) * 16);
    const b = dev.createBuffer({ size: sz, usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST | GPUBufferUsage.COPY_SRC | (extraUsage || 0), label: 'atelier-' + name });
    if (data) dev.queue.writeBuffer(b, 0, data.buffer ? data : new Uint8Array(data), 0);
    return b;
  };
  const g = { sim: sim, buf: {}, bg: {}, busy: false, frames: 0, ms: 0, alive: true };
  // particles
  const X = new Float32Array(N * 4), Pp = new Float32Array(N * 4), V = new Float32Array(N * 4), PP = new Float32Array(N * 4);
  const PDb = new ArrayBuffer(N * 32), PDu = new Uint32Array(PDb), PDf = new Float32Array(PDb);
  for (let q = 0; q < N; q++) {
    X[4 * q] = sim.x[3 * q]; X[4 * q + 1] = sim.x[3 * q + 1]; X[4 * q + 2] = sim.x[3 * q + 2]; X[4 * q + 3] = sim.w[q];
    Pp[4 * q] = sim.p[3 * q]; Pp[4 * q + 1] = sim.p[3 * q + 1]; Pp[4 * q + 2] = sim.p[3 * q + 2];
    V[4 * q] = sim.v[3 * q]; V[4 * q + 1] = sim.v[3 * q + 1]; V[4 * q + 2] = sim.v[3 * q + 2];
    PP[4 * q] = sim.damp[q]; PP[4 * q + 1] = sim.margin[q]; PP[4 * q + 2] = sim.fric[q]; PP[4 * q + 3] = sim.thickOf ? sim.thickOf[q] : 0.5;
  }
  const po = sim.pieceOf, SW = sim.SW, pm = new Uint32Array(N);
  for (let c = 0; c < SW.n; c++) { const qa = SW.a[c], q0 = SW.b0[c], q1 = SW.b1[c], ba = 1 << (po[qa] & 31), bb = 1 << (po[q0] & 31); pm[qa] |= bb | ba; pm[q0] |= ba | bb; pm[q1] |= ba | bb; }
  recs.forEach(function (r) {
    for (let i = 0; i < r.n; i++) {
      const q = r.off + i;
      PDu[8 * q] = po[q]; PDu[8 * q + 1] = sim.layerOf ? sim.layerOf[q] : 0; PDu[8 * q + 2] = sim.active[q] ? 1 : 0; PDu[8 * q + 3] = pm[q];
      PDf[8 * q + 4] = r.mesh.xy[2 * i]; PDf[8 * q + 5] = r.mesh.xy[2 * i + 1]; PDf[8 * q + 6] = sim.pdOf ? sim.pdOf[q] : 20; PDf[8 * q + 7] = sim.pressure[q] || 0;
    }
  });
  // triangles of active pieces
  const T = []; let eSum = 0, eN = 0;
  recs.forEach(function (r, ri) {
    if (r.pc.state === 'off') return;
    const tr = r.mesh.tris, xy = r.mesh.xy;
    for (let i = 0; i < tr.length; i += 3) {
      T.push(tr[i] + r.off, tr[i + 1] + r.off, tr[i + 2] + r.off, ri);
      eSum += Math.hypot(xy[2 * tr[i]] - xy[2 * tr[i + 1]], xy[2 * tr[i] + 1] - xy[2 * tr[i + 1] + 1]); eN++;
    }
  });
  const nTri = T.length / 4, edge = eN ? eSum / eN : 20;
  // distance constraints in colour order: stretch, bending, fasteners
  const S = sim.S, B = sim.B, FA = sim.FA;
  const groups = {};
  const conBuf = new ArrayBuffer(Math.max(1, S.n + B.n + FA.n) * 32), cU = new Uint32Array(conBuf), cF = new Float32Array(conBuf);
  let co = 0;
  const addGroup = function (key, n, vi, vj, fill) {
    const cz = AT.GPU.colorize(n, N, function (c, vs) { vs[0] = vi(c); vs[1] = vj(c); return 2; });
    const base = co;
    for (let k = 0; k < n; k++) { fill(cz.order[k], co); co++; }
    groups[key] = cz.ranges.map(function (r) { return [base + r[0], r[1]]; });
  };
  addGroup('stretch', S.n, function (c) { return S.i[c]; }, function (c) { return S.j[c]; }, function (c, o) { cU[8 * o] = S.i[c]; cU[8 * o + 1] = S.j[c]; cF[8 * o + 2] = S.L[c]; cF[8 * o + 3] = S.r[c]; cF[8 * o + 4] = S.cm[c]; cU[8 * o + 5] = 0; });
  addGroup('bend', B.n, function (c) { return B.i[c]; }, function (c) { return B.j[c]; }, function (c, o) { cU[8 * o] = B.i[c]; cU[8 * o + 1] = B.j[c]; cF[8 * o + 2] = B.L[c]; cF[8 * o + 3] = B.r[c]; cF[8 * o + 4] = 1; cU[8 * o + 5] = 1; });
  addGroup('fast', FA.n, function (c) { return FA.a[c]; }, function (c) { return FA.b[c]; }, function (c, o) { cU[8 * o] = FA.a[c]; cU[8 * o + 1] = FA.b[c]; cF[8 * o + 2] = 2; cU[8 * o + 5] = 2; });
  // sewing: natural order for the gap reduction, coloured copy for the solver
  const swNat = new ArrayBuffer(Math.max(1, SW.n) * 16), swNU = new Uint32Array(swNat), swNF = new Float32Array(swNat);
  for (let c = 0; c < SW.n; c++) { swNU[4 * c] = SW.a[c]; swNU[4 * c + 1] = SW.b0[c]; swNU[4 * c + 2] = SW.b1[c]; swNF[4 * c + 3] = SW.u[c]; }
  const swz = AT.GPU.colorize(SW.n, N, function (c, vs) { vs[0] = SW.a[c]; vs[1] = SW.b0[c]; vs[2] = SW.b1[c]; return 3; });
  const swCol = new ArrayBuffer(Math.max(1, SW.n) * 16), swCU = new Uint32Array(swCol), swCF = new Float32Array(swCol);
  for (let k = 0; k < SW.n; k++) { const c = swz.order[k]; swCU[4 * k] = SW.a[c]; swCU[4 * k + 1] = SW.b0[c]; swCU[4 * k + 2] = SW.b1[c]; swCF[4 * k + 3] = SW.u[c]; }
  groups.sew = swz.ranges;
  // dynamic-offset table of dispatch descriptors (256-byte stride)
  const dlist = [[0, 0, 0, 0], [0, 0, 0, 1]];
  const dIndex = {};
  ['stretch', 'bend', 'fast', 'sew'].forEach(function (k) { dIndex[k] = groups[k].map(function (r) { dlist.push([r[0], r[1], 0, 0]); return dlist.length - 1; }); });
  const dArr = new Uint32Array(dlist.length * 64);
  dlist.forEach(function (d, i) { dArr.set(d, i * 64); });
  const dBuf = dev.createBuffer({ size: dArr.byteLength, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST, label: 'atelier-D' });
  dev.queue.writeBuffer(dBuf, 0, dArr);
  g.dbg = dev.createBindGroup({ layout: G.dLayout, entries: [{ binding: 0, resource: { buffer: dBuf, size: 16 } }] });
  g.groups = groups; g.dIndex = dIndex; g.dBuf = dBuf;
  // avatar SDF grid
  const grid = sim.av.grid;
  // skin hash (static): skin vertices of the MakeHuman body, cell 24 mm
  let skinN = 0, SK = new Float32Array(8), SH = new Int32Array(G.SKIN_HS).fill(-1), SN = new Int32Array(1);
  const skinC = 24;
  if (sim.bt) {
    skinN = sim.bt.nb; SK = new Float32Array(skinN * 8); SN = new Int32Array(Math.max(1, skinN)).fill(-1);
    const Pz = sim.bt.P, Nz = sim.bt.Nn;
    const hks = function (i, j, k) { return ((Math.imul(i, 73856093) ^ Math.imul(j, 19349663) ^ Math.imul(k, 83492791)) >>> 0) & (G.SKIN_HS - 1); };
    for (let v = 0; v < skinN; v++) {
      SK[8 * v] = Pz[3 * v]; SK[8 * v + 1] = Pz[3 * v + 1]; SK[8 * v + 2] = Pz[3 * v + 2]; SK[8 * v + 4] = Nz[3 * v]; SK[8 * v + 5] = Nz[3 * v + 1]; SK[8 * v + 6] = Nz[3 * v + 2];
      const h = hks(Math.floor(Pz[3 * v] / skinC), Math.floor(Pz[3 * v + 1] / skinC), Math.floor(Pz[3 * v + 2] / skinC));
      SN[v] = SH[h]; SH[h] = v;
    }
  }
  // pins
  const PIN = new Float32Array(Math.max(1, sim.pins.length) * 4), PINu = new Uint32Array(PIN.buffer);
  for (let e = 0; e < sim.pins.length; e++) { PIN[4 * e] = sim.pinPos[3 * e]; PIN[4 * e + 1] = sim.pinPos[3 * e + 1]; PIN[4 * e + 2] = sim.pinPos[3 * e + 2]; PINu[4 * e + 3] = sim.pins[e]; }
  const nPc = recs.length;
  const ST = new Float32Array(160); ST[0] = sim.gScale == null ? -1 : sim.gScale; for (let r = 0; r < 64; r++) { ST[8 + r] = sim.gP && sim.gP[r] != null ? sim.gP[r] : 1; ST[80 + r] = -P.gravity; }
  const cap = Math.max(65536, N * 24), entCap = Math.max(131072, nTri * 48);
  const nWG = Math.ceil(N / G.WG);
  const bf = g.buf;
  bf.X = mk('X', X); bf.P = mk('P', Pp); bf.V = mk('V', V); bf.PP = mk('PP', PP); bf.PD = mk('PD', PDu); g.PDu = PDu;
  bf.TRI = mk('TRI', new Uint32Array(T.length ? T : [0, 0, 0, 0]));
  bf.CON = mk('CON', cU); bf.SW = mk('SWn', swNU); bf.SWC = mk('SWc', swCU);
  bf.GRID = mk('GRID', grid.data instanceof Float32Array ? grid.data : Float32Array.from(grid.data));
  bf.SKIN = mk('SKIN', SK); bf.SHEAD = mk('SHEAD', SH); bf.SNEXT = mk('SNEXT', SN);
  bf.PIN = mk('PIN', PIN); bf.GR = mk('GR', null, 0, 4096 * 32); g.grabCap = 4096;
  bf.ST = mk('ST', ST); bf.CTRL = mk('CTRL', new Uint32Array(136));
  bf.NRM = mk('NRM', null, 0, N * 16); bf.DELTA = mk('DELTA', null, 0, N * 16);
  bf.HC = mk('HC', null, 0, G.HS * 4); bf.HASH = mk('HASH', null, 0, (G.HS + 1 + entCap) * 4); bf.BS = mk('BS', null, 0, 260 * 4);
  bf.CONT = mk('CONT', null, 0, cap * 16); bf.PART = mk('PART', null, 0, nWG * 8);
  g.U = dev.createBuffer({ size: 192, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST, label: 'atelier-F' });
  g.Fbuf = new ArrayBuffer(192); g.Ff = new Float32Array(g.Fbuf); g.Fu = new Uint32Array(g.Fbuf);
  const Fu = g.Fu, Ff = g.Ff;
  Ff[6] = P.clothThickness == null ? 2.5 : P.clothThickness; Ff[7] = Math.max(2, edge * 0.25);
  Ff[8] = grid.x0; Ff[9] = grid.y0; Ff[10] = grid.z0; Ff[11] = grid.g; Fu[12] = grid.nx; Fu[13] = grid.ny; Fu[14] = grid.nz; Fu[15] = N;
  Fu[16] = nTri; Fu[17] = skinN; Fu[18] = G.HS; Fu[19] = cap;
  let lmin = 99, lmax = -99, pdMax = 0; for (let q = 0; q < N; q++) { const L = sim.layerOf ? sim.layerOf[q] : 0; if (L < lmin) lmin = L; if (L > lmax) lmax = L; if (sim.pdOf && sim.pdOf[q] > pdMax) pdMax = sim.pdOf[q]; }
  const hxMax = lmax > lmin ? 0.3 * (pdMax || 20) : 0;
  Ff[20] = AT.clamp(edge * 1.1, 10, 60); Ff[21] = Ff[6] + 2 + hxMax + (P.ccSlack == null ? 8 : P.ccSlack); Fu[22] = entCap; Fu[23] = SW.n; Ff[35] = hxMax;
  Ff[24] = skinC; Fu[25] = G.SKIN_HS; Fu[26] = sim.pins.length; Fu[27] = 0;
  Fu[31] = sim.pressure.some(function (a) { return a !== 0; }) ? 1 : 0;
  Fu[32] = nPc; Fu[33] = nTri; Fu[34] = P.ccLayer ? 1 : 0;
  Fu[36] = P.ccEdge ? 1 : 0; Ff[37] = P.ccFric == null ? 2 : P.ccFric; Ff[38] = P.ccBand == null ? 1.5 : P.ccBand;
  Fu[39] = P.ccRepair === false ? 0 : 1; Ff[40] = P.ccRepairMargin == null ? 8 : P.ccRepairMargin;
  Fu[42] = P.ccFricMode == null ? 1 : P.ccFricMode; Ff[43] = P.ccMuCap == null ? 0.8 : P.ccMuCap;
  // bind groups (one per pass); the sewing solver binds the coloured copy under the name SW
  Object.keys(G.pipes).forEach(function (name) {
    const pp = G.pipes[name];
    const entries = [{ binding: 0, resource: { buffer: g.U } }];
    pp.binds.forEach(function (b, i) { const key = (name === 'sew' && b[0] === 'SW') ? 'SWC' : b[0]; entries.push({ binding: i + 1, resource: { buffer: bf[key] } }); });
    g.bg[name] = dev.createBindGroup({ layout: pp.bgl, entries: entries });
  });
  // readback: positions + state + energy partials, double-buffered
  g.rbX = N * 16; g.rbST = 160 * 4; g.rbPART = nWG * 8; g.nWG = nWG;
  const rbSize = Math.ceil((g.rbX + g.rbST + g.rbPART) / 16) * 16;
  g.stage = [0, 1].map(function (i) { return dev.createBuffer({ size: rbSize, usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST, label: 'atelier-readback' + i }); });
  g.si = 0; g.N = N; g.nTri = nTri; g.nPc = nPc; g.skinN = skinN; g.cap = cap; g.entCap = entCap; g.edge = edge;
  sim._g = g;
  return g;
};
AT.GPU.release = function (sim) {
  AT.untangleRelease(sim);
  const g = sim && sim._g; if (!g) return;
  g.alive = false; sim._g = null;
  const kill = function () { Object.keys(g.buf).forEach(function (k) { try { g.buf[k].destroy(); } catch (e) { } }); try { g.U.destroy(); g.dBuf.destroy(); } catch (e) { } g.stage.forEach(function (s) { try { s.destroy(); } catch (e) { } }); };
  if (g.busy && g.pending) g.pending.then(kill, kill); else kill();
};
// untangler results: bit0 active, bit1 wanted side (1 = along the triangle normal), bits 8+ = tangled piece + 1
AT.GPU.setTangles = function (sim, tang) {
  const g = sim._g; if (!g || !g.PDu || !AT.GPU.device) return;
  const P = g.PDu, N = g.N; let n = 0;
  for (let q = 0; q < N; q++) { const a = P[8 * q + 2] & 1; P[8 * q + 2] = a | (tang[q] & ~1); if (tang[q]) n++; }
  g.tangled = n;
  try { AT.GPU.device.queue.writeBuffer(g.buf.PD, 0, P); } catch (e) { }
};
// upload CPU-side positions (after a CPU edit such as dragging a piece or a NaN restore)
AT.GPU.upload = function (sim, resetVel) {
  const g = sim._g; if (!g) return;
  const N = sim.N, X = new Float32Array(N * 4), Pp = new Float32Array(N * 4);
  for (let q = 0; q < N; q++) { X[4 * q] = sim.x[3 * q]; X[4 * q + 1] = sim.x[3 * q + 1]; X[4 * q + 2] = sim.x[3 * q + 2]; X[4 * q + 3] = sim.w[q]; Pp[4 * q] = sim.p[3 * q]; Pp[4 * q + 1] = sim.p[3 * q + 1]; Pp[4 * q + 2] = sim.p[3 * q + 2]; }
  const q = AT.GPU.device.queue;
  q.writeBuffer(g.buf.X, 0, X); q.writeBuffer(g.buf.P, 0, Pp);
  if (resetVel) q.writeBuffer(g.buf.V, 0, new Float32Array(N * 4));
};
AT.GPU.encodeFrame = function (sim, dt) {
  const G = AT.GPU, g = sim._g, dev = G.device, P = sim.params, pp = G.pipes;
  const n = P.substeps, h = (dt || 1 / 60) / n, Ff = g.Ff, Fu = g.Fu;
  Ff[0] = h; Ff[1] = 1 / (h * h); Ff[2] = P.gravity; Ff[3] = P.air; Ff[4] = P.sewSpeed * h; Ff[5] = 6000;
  Fu[26] = sim.pins.length;
  // grab (mouse drag in 3D)
  const gb = sim.grab;
  if (gb && gb.idx.length) {
    const m = Math.min(gb.idx.length, g.grabCap), A = new Float32Array(m * 8), Au = new Uint32Array(A.buffer);
    for (let e = 0; e < m; e++) { A[8 * e] = gb.rel[3 * e]; A[8 * e + 1] = gb.rel[3 * e + 1]; A[8 * e + 2] = gb.rel[3 * e + 2]; A[8 * e + 3] = gb.wt[e]; Au[8 * e + 4] = gb.idx[e]; }
    const an = AT.grabAnchor(sim, (dt || 1 / 60) * Math.max(1, Math.min(6, P.framesPerStep | 0 || 1)));
    dev.queue.writeBuffer(g.buf.GR, 0, A); Fu[27] = m; Ff[28] = an[0]; Ff[29] = an[1]; Ff[30] = an[2];
  } else Fu[27] = 0;
  dev.queue.writeBuffer(g.U, 0, g.Fbuf);
  const enc = dev.createCommandEncoder({ label: 'atelier-frame' });
  const ps = enc.beginComputePass({ label: 'atelier-sim' });
  const run = function (name, count, di) {
    if (count <= 0) return;
    const p = pp[name]; ps.setPipeline(p.pipe); ps.setBindGroup(0, g.bg[name]);
    if (p.useD) ps.setBindGroup(1, g.dbg, [(di || 0) * 256]);
    ps.dispatchWorkgroups(Math.ceil(count / p.wg));
  };
  const runD = function (name, key) { const R = g.groups[key], I = g.dIndex[key]; for (let k = 0; k < R.length; k++) run(name, R[k][1], I[k]); };
  const N = g.N, sc = P.selfCollision !== false, ke = Math.max(1, Math.min(n, P.ccEveryGPU || 2)), K = Math.max(1, Math.min(6, P.framesPerStep | 0 || 1));
  for (let f = 0; f < K; f++) {
  if (sim.SW.n) run('gap', Math.ceil(sim.SW.n / 2));
  run('grav', 1);
  if (Fu[31]) { run('nclear', N * 4); run('nacc', g.nTri); }
  for (let s = 0; s < n; s++) {
    run('integrate', N); run('body', N);
    for (let it = 0; it < P.iterations; it++) { runD('dist', 'stretch'); runD('sew', 'sew'); }
    runD('dist', 'bend'); runD('dist', 'fast');
    if (sc && g.nTri) {
      if (s % ke === ke - 1 || (g.frames === 0 && f === 0 && s === 0)) {
        run('hclear', G.HS); run('hcount', g.nTri); run('scan1', G.HS); run('scan2', 256); run('scan3', G.HS); run('scatter', g.nTri); run('detect', N);
      }
      for (let ci = 0; ci < (P.ccIters || 2); ci++) { run('resolve', g.cap); run('apply', N, 0); }
    }
    if (Fu[27]) run('grab', Fu[27]);
    run('guard', N);
    if (g.skinN && (s === n - 1 || s === (n >> 1) - 1)) { run('bodytri', g.nTri); run('apply', N, 1); }
    if (sim.pins.length) run('pins', sim.pins.length);
    run('vel', N);
  }
  }
  run('energy', N);
  ps.end();
  const st = g.stage[g.si]; g.si ^= 1;
  enc.copyBufferToBuffer(g.buf.X, 0, st, 0, g.rbX);
  enc.copyBufferToBuffer(g.buf.ST, 0, st, g.rbX, g.rbST);
  enc.copyBufferToBuffer(g.buf.PART, 0, st, g.rbX + g.rbST, g.rbPART);
  dev.queue.submit([enc.finish()]);
  return st;
};
// one simulation frame on the GPU; returns a promise that resolves when positions are back in sim.x
AT.GPU.step = function (sim, dt) {
  const G = AT.GPU;
  if (G.state !== 'ready') return null;
  const g = sim._g || G.attach(sim);
  if (!g || g.busy) return null;
  if (sim._dirtyX) { G.upload(sim, true); sim._dirtyX = false; }
  const t0 = (typeof performance !== 'undefined' ? performance.now() : Date.now());
  let st;
  try { st = G.encodeFrame(sim, dt); } catch (e) { G.state = 'failed'; G.error = e.message; return null; }
  g.busy = true;
  g.pending = st.mapAsync(GPUMapMode.READ).then(function () {
    if (!g.alive) { st.unmap(); return; }
    if (G.lastError) { st.unmap(); G.state = 'failed'; G.error = 'GPU error: ' + G.lastError; return; }
    const all = st.getMappedRange(), N = g.N;
    const X = new Float32Array(all, 0, N * 4), ST = new Float32Array(all, g.rbX, 160), PA = new Float32Array(all, g.rbX + g.rbST, g.nWG * 2);
    let bad = false; const x = sim.x;
    for (let q = 0; q < N; q++) { const a = X[4 * q], b = X[4 * q + 1], c = X[4 * q + 2]; if (!(a === a && b === b && c === c)) { bad = true; break; } x[3 * q] = a; x[3 * q + 1] = b; x[3 * q + 2] = c; }
    let e = 0, cnt = 0; for (let i = 0; i < g.nWG; i++) { e += PA[2 * i]; cnt += PA[2 * i + 1]; }
    sim.gScale = ST[0]; sim.sewGap = ST[1];
    if (!sim.gP || sim.gP.length !== g.nPc) sim.gP = new Float32Array(g.nPc);
    for (let r = 0; r < g.nPc; r++) sim.gP[r] = ST[8 + Math.min(r, 63)];
    st.unmap();
    if (bad) { x.set(sim.good); sim.p.set(sim.good); G.upload(sim, true); sim.energy = 0; }
    else { sim.energy = cnt ? Math.sqrt(e / cnt) : 0; if ((sim.frames & 15) === 0) sim.good.set(x); }
    const K = Math.max(1, Math.min(6, sim.params.framesPerStep | 0 || 1)); sim.frames += K; sim.time += (dt || 1 / 60) * K; g.frames += K;
    if (!bad) AT.untangleTick(sim);
    const t1 = (typeof performance !== 'undefined' ? performance.now() : Date.now());
    g.ms = g.ms ? g.ms * 0.9 + (t1 - t0) * 0.1 : (t1 - t0);
  }).catch(function (err) { G.error = err && err.message; }).then(function () { g.busy = false; });
  return g.pending;
};

/* ---------- untangling: global intersection analysis on the CPU (Web Worker) ----------
   Contacts only stop new crossings. Cloth that is already through other cloth would be held on the wrong
   side (the two pieces look welded at a point). Every few frames this finds mesh edges that pierce cloth
   triangles, splits each piece along those edges and marks the smaller region (the part that poked through).
   The GPU contact pass then pushes the marked vertices back to the wanted side. Self-contained: it also
   runs inside a Blob worker via toString(). */
AT.untangleCore = function (S, X) {
  const N = S.N, T = S.T, nT = T.length / 4, cs = S.cell, inv = 1 / cs;
  const cells = new Map();
  const key = function (i, j, k) { return (i * 73856093) ^ (j * 19349663) ^ (k * 83492791); };
  for (let t = 0; t < nT; t++) {
    const a = T[4 * t], b = T[4 * t + 1], c = T[4 * t + 2];
    const x0 = Math.floor(Math.min(X[3 * a], X[3 * b], X[3 * c]) * inv), x1 = Math.floor(Math.max(X[3 * a], X[3 * b], X[3 * c]) * inv);
    const y0 = Math.floor(Math.min(X[3 * a + 1], X[3 * b + 1], X[3 * c + 1]) * inv), y1 = Math.floor(Math.max(X[3 * a + 1], X[3 * b + 1], X[3 * c + 1]) * inv);
    const z0 = Math.floor(Math.min(X[3 * a + 2], X[3 * b + 2], X[3 * c + 2]) * inv), z1 = Math.floor(Math.max(X[3 * a + 2], X[3 * b + 2], X[3 * c + 2]) * inv);
    if ((x1 - x0 + 1) * (y1 - y0 + 1) * (z1 - z0 + 1) > 64) continue;
    for (let i = x0; i <= x1; i++) for (let j = y0; j <= y1; j++) for (let k = z0; k <= z1; k++) { const h = key(i, j, k); let l = cells.get(h); if (!l) { l = []; cells.set(h, l); } l.push(t); }
  }
  const stamp = new Int32Array(nT).fill(-1);
  const E = S.E, nE = E.length / 2, cr = []; // crossings: [u, v, t, sign of u]
  for (let e = 0; e < nE; e++) {
    const u = E[2 * e], v = E[2 * e + 1];
    const ux = X[3 * u], uy = X[3 * u + 1], uz = X[3 * u + 2], vx = X[3 * v], vy = X[3 * v + 1], vz = X[3 * v + 2];
    const x0 = Math.floor(Math.min(ux, vx) * inv), x1 = Math.floor(Math.max(ux, vx) * inv), y0 = Math.floor(Math.min(uy, vy) * inv), y1 = Math.floor(Math.max(uy, vy) * inv), z0 = Math.floor(Math.min(uz, vz) * inv), z1 = Math.floor(Math.max(uz, vz) * inv);
    if ((x1 - x0 + 1) * (y1 - y0 + 1) * (z1 - z0 + 1) > 27) continue;
    const pu = S.piece[u], ex2 = 4.84 * S.pd[u] * S.pd[u];
    for (let i = x0; i <= x1; i++) for (let j = y0; j <= y1; j++) for (let k = z0; k <= z1; k++) {
      const l = cells.get(key(i, j, k)); if (!l) continue;
      for (let m = 0; m < l.length; m++) {
        const t = l[m]; if (stamp[t] === e) continue; stamp[t] = e;
        const a = T[4 * t], b = T[4 * t + 1], c = T[4 * t + 2];
        if (a === u || b === u || c === u || a === v || b === v || c === v) continue;
        if (T[4 * t + 3] === pu) {
          const dxa = S.xy[2 * a] - S.xy[2 * u], dya = S.xy[2 * a + 1] - S.xy[2 * u + 1]; if (dxa * dxa + dya * dya < ex2) continue;
          const dxb = S.xy[2 * b] - S.xy[2 * u], dyb = S.xy[2 * b + 1] - S.xy[2 * u + 1]; if (dxb * dxb + dyb * dyb < ex2) continue;
          const dxc = S.xy[2 * c] - S.xy[2 * u], dyc = S.xy[2 * c + 1] - S.xy[2 * u + 1]; if (dxc * dxc + dyc * dyc < ex2) continue;
        }
        if ((S.sewn[u] || S.sewn[v]) && (S.sewn[a] || S.sewn[b] || S.sewn[c])) continue;
        const ax = X[3 * a], ay = X[3 * a + 1], az = X[3 * a + 2];
        const e1x = X[3 * b] - ax, e1y = X[3 * b + 1] - ay, e1z = X[3 * b + 2] - az, e2x = X[3 * c] - ax, e2y = X[3 * c + 1] - ay, e2z = X[3 * c + 2] - az;
        const nx = e1y * e2z - e1z * e2y, ny = e1z * e2x - e1x * e2z, nz = e1x * e2y - e1y * e2x;
        const du = (ux - ax) * nx + (uy - ay) * ny + (uz - az) * nz, dv = (vx - ax) * nx + (vy - ay) * ny + (vz - az) * nz;
        if (du === 0 || dv === 0 || (du > 0) === (dv > 0)) continue;
        const s = du / (du - dv), px = ux + (vx - ux) * s - ax, py = uy + (vy - uy) * s - ay, pz = uz + (vz - uz) * s - az;
        const d00 = e1x * e1x + e1y * e1y + e1z * e1z, d01 = e1x * e2x + e1y * e2y + e1z * e2z, d11 = e2x * e2x + e2y * e2y + e2z * e2z;
        const d20 = px * e1x + py * e1y + pz * e1z, d21 = px * e2x + py * e2y + pz * e2z, den = d00 * d11 - d01 * d01; if (den <= 0) continue;
        const bv = (d11 * d20 - d01 * d21) / den, bw = (d00 * d21 - d01 * d20) / den;
        if (bv < 1e-4 || bw < 1e-4 || bv + bw > 1 - 1e-4) continue;
        cr.push(u, v, t, du > 0 ? 1 : -1);
      }
    }
  }
  // group crossings by (edge piece, triangle piece); split the edge piece along its crossing edges
  const groups = new Map();
  for (let i = 0; i < cr.length; i += 4) { const A = S.piece[cr[i]], B = T[4 * cr[i + 2] + 3], gk = A * 4096 + B; let gl = groups.get(gk); if (!gl) { gl = []; groups.set(gk, gl); } gl.push(i); }
  const tang = new Int32Array(N), best = new Int32Array(N).fill(1 << 30), vis = new Int32Array(N).fill(-1);
  let pass = 0, marked = 0, ambiguous = 0, resolved = 0;
  const cap = S.cap || 250;
  groups.forEach(function (gl, gk) {
    const B = gk % 4096, cut = new Set();
    gl.forEach(function (i) { const u = cr[i], v = cr[i + 1]; cut.add(Math.min(u, v) * N + Math.max(u, v)); });
    // separate tangle spots: crossing edges that share a vertex belong to the same intersection curve
    const par = gl.map(function (_, k) { return k; }), find = function (k) { while (par[k] !== k) { par[k] = par[par[k]]; k = par[k]; } return k; };
    const byV = new Map();
    gl.forEach(function (i, k) { [cr[i], cr[i + 1]].forEach(function (x) { const o = byV.get(x); if (o == null) byV.set(x, k); else { const ra = find(o), rb = find(k); if (ra !== rb) par[ra] = rb; } }); });
    const clusters = new Map(); gl.forEach(function (i, k) { const r = find(k); let c = clusters.get(r); if (!c) { c = []; clusters.set(r, c); } c.push(i); });
    clusters.forEach(function (cl) {
      const seedP = [], seedN = [];
      cl.forEach(function (i) { const u = cr[i], v = cr[i + 1]; if (cr[i + 3] > 0) { seedP.push(u); seedN.push(v); } else { seedN.push(u); seedP.push(v); } });
      const fill = function (seeds, other) {
        const id = ++pass, q = [], out = []; let merged = false;
        const otherSet = new Set(other);
        for (let i = 0; i < seeds.length; i++) { const x = seeds[i]; if (vis[x] !== id && !otherSet.has(x)) { vis[x] = id; q.push(x); } }
        for (let h = 0; h < q.length; h++) {
          const x = q[h]; out.push(x); if (out.length > cap) return { set: out, capped: true, merged: merged };
          for (let k = S.adjStart[x]; k < S.adjStart[x + 1]; k++) {
            const w = S.adj[k]; if (vis[w] === id) continue;
            if (cut.has(Math.min(x, w) * N + Math.max(x, w))) continue;
            if (otherSet.has(w)) { merged = true; continue; }
            vis[w] = id; q.push(w);
          }
        }
        return { set: out, capped: false, merged: merged };
      };
      const rp = fill(seedP, seedN), rn = fill(seedN, seedP);
      if (rp.merged || rn.merged || (rp.capped && rn.capped)) { ambiguous++; return; }
      let poked, want;
      if (rp.capped) { poked = rn; want = 1; } else if (rn.capped) { poked = rp; want = 0; }
      else if (rp.set.length <= 0.6 * rn.set.length) { poked = rp; want = 0; } else if (rn.set.length <= 0.6 * rp.set.length) { poked = rn; want = 1; }
      else { ambiguous++; return; }
      resolved++;
      const code = ((B + 1) << 8) | (want << 1), sz = poked.set.length;
      poked.set.forEach(function (x) { if (sz < best[x]) { best[x] = sz; if (!tang[x]) marked++; tang[x] = code; } });
    });
  });
  return { tang: tang, crossings: cr.length / 4, groups: groups.size, resolved: resolved, ambiguous: ambiguous, marked: marked };
};
// per-sim static data for the untangler
AT.untangleSetup = function (sim) {
  const N = sim.N, T = [], E = [], seen = new Set(), nb = []; for (let q = 0; q < N; q++) nb.push([]);
  const xy = new Float32Array(N * 2), pd = new Float32Array(N), piece = new Int32Array(N);
  let eSum = 0, eN = 0;
  sim.recs.forEach(function (r, ri) {
    for (let i = 0; i < r.n; i++) { const q = r.off + i; xy[2 * q] = r.mesh.xy[2 * i]; xy[2 * q + 1] = r.mesh.xy[2 * i + 1]; pd[q] = sim.pdOf ? sim.pdOf[q] : 20; piece[q] = ri; }
    if (r.pc.state === 'off') return;
    const tr = r.mesh.tris;
    for (let i = 0; i < tr.length; i += 3) {
      const a = tr[i] + r.off, b = tr[i + 1] + r.off, c = tr[i + 2] + r.off; T.push(a, b, c, ri);
      [[a, b], [b, c], [c, a]].forEach(function (p) { const lo = Math.min(p[0], p[1]), hi = Math.max(p[0], p[1]), k = lo * N + hi; if (!seen.has(k)) { seen.add(k); E.push(lo, hi); nb[lo].push(hi); nb[hi].push(lo); eSum += Math.hypot(xy[2 * lo] - xy[2 * hi], xy[2 * lo + 1] - xy[2 * hi + 1]); eN++; } });
    }
  });
  const adjStart = new Int32Array(N + 1); for (let q = 0; q < N; q++) adjStart[q + 1] = adjStart[q] + nb[q].length;
  const adj = new Int32Array(adjStart[N]); for (let q = 0; q < N; q++) adj.set(nb[q], adjStart[q]);
  const sewn = new Uint8Array(N); for (let q = 0; q < N; q++) sewn[q] = sim.sewn[q] ? 1 : 0;
  return { N: N, T: Uint32Array.from(T), E: Uint32Array.from(E), adjStart: adjStart, adj: adj, piece: piece, xy: xy, pd: pd, sewn: sewn, cell: Math.max(8, (eN ? eSum / eN : 20) * 1.2), cap: 250 };
};
// worker lifecycle: post positions every few frames while idle; results become per-vertex tangle codes on the GPU
AT.untangleTick = function (sim) {
  if (!sim || !sim._g || (sim.params && sim.params.untangle === false)) return;
  if (typeof Worker === 'undefined' || typeof Blob === 'undefined') return;
  let U = sim._ut;
  if (!U) {
    try {
      const code = 'const AT = {}; AT.untangleCore = ' + AT.untangleCore.toString() + ';\nlet S = null; self.onmessage = function (e) { if (e.data.setup) { S = e.data.setup; return; } const r = AT.untangleCore(S, e.data.X); self.postMessage({ id: e.data.id, tang: r.tang, stats: { crossings: r.crossings, resolved: r.resolved, ambiguous: r.ambiguous, marked: r.marked } }, [r.tang.buffer]); };';
      const url = URL.createObjectURL(new Blob([code], { type: 'text/javascript' }));
      const w = new Worker(url); URL.revokeObjectURL(url);
      U = sim._ut = { w: w, busy: false, last: -1e9, id: 0, stats: null };
      w.postMessage({ setup: AT.untangleSetup(sim) });
      w.onmessage = function (e) { U.busy = false; U.stats = e.data.stats; U.tot = U.tot || { runs: 0, resolved: 0, ambiguous: 0, marked: 0 }; U.tot.runs++; U.tot.resolved += e.data.stats.resolved; U.tot.ambiguous += e.data.stats.ambiguous; U.tot.marked += e.data.stats.marked; if (sim._g && sim._g.alive !== false) AT.GPU.setTangles(sim, e.data.tang); };
      w.onerror = function () { U.busy = false; U.dead = true; };
    } catch (err) { sim._ut = { dead: true }; return; }
  }
  if (U.dead || U.busy || sim.frames - U.last < 6) return;
  U.busy = true; U.last = sim.frames;
  const X = new Float32Array(sim.x);
  U.w.postMessage({ id: ++U.id, X: X }, [X.buffer]);
};
AT.untangleRelease = function (sim) { if (sim && sim._ut && sim._ut.w) { try { sim._ut.w.terminate(); } catch (e) { } } if (sim) sim._ut = null; };

// the app calls this once per animation frame: GPU when available, CPU otherwise
AT.simStep = function (sim, dt) {
  const G = AT.GPU;
  if (G.state === 'idle' && typeof navigator !== 'undefined' && navigator.gpu) G.init();
  if (G.state === 'ready' && sim.N) { sim.backend = 'gpu'; G.step(sim, dt); return; }
  if (G.state === 'init') return; // wait for the GPU instead of starting on the CPU
  sim.backend = 'cpu';
  const K = Math.max(1, Math.min(6, sim.params.framesPerStep | 0 || 1));
  for (let f = 0; f < K; f++) AT.step(sim, dt);
};

// diagnostics: contact count and hash fill of the last detection
AT.GPU.debugCounts = async function (sim) {
  const g = sim && sim._g, dev = AT.GPU.device; if (!g) return null;
  const st = dev.createBuffer({ size: 32, usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST });
  const enc = dev.createCommandEncoder();
  enc.copyBufferToBuffer(g.buf.CTRL, 0, st, 0, 16); enc.copyBufferToBuffer(g.buf.HASH, AT.GPU.HS * 4, st, 16, 4);
  dev.queue.submit([enc.finish()]); await st.mapAsync(GPUMapMode.READ);
  const u = new Uint32Array(st.getMappedRange().slice(0)); st.unmap(); st.destroy();
  return { contacts: u[0], contactCap: g.cap, hashEntries: u[4], hashCap: g.entCap, triangles: g.nTri, particles: g.N };
};


/* ============================================================
   ATELIER UI BASE — loaders, styling, textures, file IO
   ============================================================ */
AT.scripts = {};
AT.loadScript = function (url, globalName) {
  if (globalName && window[globalName]) return Promise.resolve(window[globalName]);
  if (AT.scripts[url]) return AT.scripts[url];
  AT.scripts[url] = new Promise(function (res, rej) {
    const s = document.createElement('script'); s.src = url; s.async = true;
    s.onload = function () { res(globalName ? window[globalName] : true); };
    s.onerror = function () { delete AT.scripts[url]; rej(new Error('Failed to load ' + url)); };
    document.head.appendChild(s);
  });
  return AT.scripts[url];
};
AT.LIBS = {
  three: ['https://cdnjs.cloudflare.com/ajax/libs/three.js/r128/three.min.js', 'THREE'],
  delaunator: ['https://cdn.jsdelivr.net/npm/delaunator@5.0.1/delaunator.min.js', 'Delaunator'],
  jspdf: ['https://cdnjs.cloudflare.com/ajax/libs/jspdf/2.5.1/jspdf.umd.min.js', 'jspdf']
};
AT.isEmpty = function (v) { return v === undefined || v === null || (typeof v === 'object' && (Array.isArray(v) ? v.length === 0 : Object.keys(v).every(function (k) { return AT.isEmpty(v[k]); }))); };
AT.stable = function (v) {
  if (v === null || v === undefined) return 'null';
  if (typeof v !== 'object') return JSON.stringify(typeof v === 'number' && !isFinite(v) ? null : v);
  if (Array.isArray(v)) return '[' + v.map(AT.stable).join(',') + ']';
  return '{' + Object.keys(v).filter(function (k) { return !AT.isEmpty(v[k]); }).sort().map(function (k) { return JSON.stringify(k) + ':' + AT.stable(v[k]); }).join(',') + '}';
};
AT.errors = [];
AT.reportError = function (where, e) {
  const msg = where + ': ' + (e && e.message ? e.message : String(e));
  if (AT.errors.length < 20 && AT.errors[AT.errors.length - 1] !== msg) { AT.errors.push(msg); try { console.error('[Atelier] ' + msg, e); } catch (x) {} }
};
AT.resolveAsset = function (o) {
  if (!o) return null;
  if (o.path && AT.assetBase) { const b = AT.assetBase.endsWith('/') ? AT.assetBase : AT.assetBase + '/'; return b + (AT.encodePath ? AT.encodePath(o.path) : o.path); }
  return o.url || null;
};
AT.clean = function (v) { return v == null ? null : JSON.parse(JSON.stringify(v, function (k, x) { return typeof x === 'number' && !isFinite(x) ? 0 : x; })); };
AT.fmt = function (mm, units, digits) {
  if (mm == null || isNaN(mm)) return '–';
  if (units === 'in') return (mm / 25.4).toFixed(digits == null ? 2 : digits) + '″';
  if (units === 'mm') return mm.toFixed(digits == null ? 1 : digits);
  return (mm / 10).toFixed(digits == null ? 1 : digits);
};
AT.unitLabel = function (u) { return u === 'in' ? 'in' : u === 'mm' ? 'mm' : 'cm'; };
AT.toMM = function (v, units) { const n = parseFloat(v); if (isNaN(n)) return null; return units === 'in' ? n * 25.4 : units === 'mm' ? n : n * 10; };
AT.fromMM = function (mm, units) { return units === 'in' ? mm / 25.4 : units === 'mm' ? mm : mm / 10; };
AT.download = function (name, data, mime) {
  const blob = data instanceof Blob ? data : new Blob([data], { type: mime || 'application/octet-stream' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a'); a.href = url; a.download = name; document.body.appendChild(a); a.click();
  setTimeout(function () { URL.revokeObjectURL(url); a.remove(); }, 1500);
};
AT.slug = function (s) { return String(s || 'garment').trim().replace(/[^a-z0-9]+/gi, '-').replace(/^-|-$/g, '').toLowerCase() || 'garment'; };

/* ---------- icons (24px grid, 1.5 stroke) ---------- */
AT.ICONS = {
  select3d: 'M5 3l12 6.5-5.2 1.3L9.5 16z M12 12l5 5',
  pin: 'M12 21v-6 M8 3h8l-1 6 3 3H6l3-3z',
  transform: 'M4 4h6v6H4z M14 14h6v6h-6z M10 7h4l-1.5-1.5 M14 7l-1.5 1.5 M7 10v4l-1.5-1.5 M7 14l1.5-1.5',
  edit: 'M5 19l3-1 10-10-2-2L6 16z M14 6l2 2 M4 4h3v3H4z',
  curvature: 'M4 18C8 6 16 6 20 18 M12 9.5v-5 M10 6.5l2-2 2 2',
  curvept: 'M4 18C8 6 16 6 20 18 M12 9.5m-2 0a2 2 0 1 0 4 0a2 2 0 1 0-4 0',
  addpoint: 'M4 18L20 6 M12 12m-2.2 0a2.2 2.2 0 1 0 4.4 0a2.2 2.2 0 1 0-4.4 0 M17 17h4 M19 15v4',
  polygon: 'M5 18l2-11 8-3 4 8-6 7z',
  rect: 'M5 6h14v12H5z',
  ellipse: 'M12 12m-8 0a8 6 0 1 0 16 0a8 6 0 1 0-16 0',
  internal: 'M4 5h16v14H4z M8 15l4-6 4 6',
  notch: 'M4 6h16v13H4z M12 6v4',
  segsew: 'M4 6h6v12H4z M14 6h6v12h-6z M10 9h4 M10 12h4 M10 15h4',
  freesew: 'M4 6h6v12H4z M14 6h6v12h-6z M10 10c1.5 1 2.5 1 4 0 M10 14c1.5 1 2.5 1 4 0',
  editsew: 'M4 6h6v12H4z M14 6h6v12h-6z M10 12h4 M18 4l2 2-5 5h-2v-2z',
  stitch: 'M4 6h16v12H4z M7 10h2 M11 10h2 M15 10h2 M7 14h2 M11 14h2 M15 14h2',
  button: 'M12 12m-7 0a7 7 0 1 0 14 0a7 7 0 1 0-14 0 M10 10.5h.01 M14 10.5h.01 M10 13.5h.01 M14 13.5h.01',
  hole: 'M5 12a3 3 0 0 1 3-3h8a3 3 0 0 1 0 6H8a3 3 0 0 1-3-3z M8 12h8',
  fasten: 'M8 12m-4 0a4 4 0 1 0 8 0a4 4 0 1 0-8 0 M13 12h7 M17 9l3 3-3 3',
  graphic: 'M4 5h16v14H4z M4 16l5-5 4 4 2-2 5 5 M15 9h.01',
  comment: 'M5 5h14v10H10l-4 4v-4H5z',
  measure: 'M3 15l12-12 6 6-12 12z M7 11l2 2 M10 8l2 2 M13 5l2 2',
  simulate: 'M8 5v14l11-7z',
  pause: 'M7 5h3v14H7z M14 5h3v14h-3z',
  avatar: 'M12 6m-2.5 0a2.5 2.5 0 1 0 5 0a2.5 2.5 0 1 0-5 0 M7 21l1.5-8-2.5-2 3-2.5h6l3 2.5-2.5 2L17 21',
  arrpts: 'M12 4v16 M4 12h16 M12 12m-3 0a3 3 0 1 0 6 0a3 3 0 1 0-6 0',
  wire: 'M4 4l16 16 M4 12l8 8 M12 4l8 8 M4 4h16v16H4z',
  fit: 'M12 3c-3 4-6 6-6 10a6 6 0 0 0 12 0c0-4-3-6-6-10z',
  seams: 'M5 19L19 5 M8 19l11-11 M5 16L16 5',
  xray: 'M12 12m-8 0a8 8 0 1 0 16 0a8 8 0 1 0-16 0 M12 4v16',
  grid: 'M4 4h16v16H4z M4 9.3h16 M4 14.7h16 M9.3 4v16 M14.7 4v16',
  sa: 'M6 6h12v12H6z M3 3h18v18H3z',
  len: 'M4 17h16 M4 14v6 M20 14v6 M8 11h8 M12 4v7',
  texture2d: 'M4 4h16v16H4z M4 10l6-6 M4 16l12-12 M8 20l12-12 M14 20l6-6',
  camera: 'M4 8h4l2-3h4l2 3h4v11H4z M12 13m-3.5 0a3.5 3.5 0 1 0 7 0a3.5 3.5 0 1 0-7 0',
  video: 'M4 7h11v10H4z M15 10l5-3v10l-5-3',
  undo: 'M9 14L4 9l5-5 M4 9h10a6 6 0 0 1 0 12h-3',
  redo: 'M15 14l5-5-5-5 M20 9H10a6 6 0 0 0 0 12h3',
  trash: 'M5 7h14 M9 7V4h6v3 M7 7l1 13h8l1-13',
  plus: 'M12 5v14 M5 12h14',
  close: 'M6 6l12 12 M18 6L6 18',
  library: 'M5 4v16 M9 4v16 M13 4l4 16 M17 4l3 12',
  chevron: 'M9 6l6 6-6 6',
  down: 'M6 9l6 6 6-6',
  eye: 'M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12z M12 12m-3 0a3 3 0 1 0 6 0a3 3 0 1 0-6 0',
  eyeoff: 'M3 3l18 18 M10.6 5.1A9.7 9.7 0 0 1 12 5c6 0 10 7 10 7a17 17 0 0 1-3.2 3.9 M6.6 6.6C3.9 8.3 2 12 2 12s4 7 10 7a9.6 9.6 0 0 0 5.4-1.6',
  lock: 'M6 11h12v9H6z M8.5 11V8a3.5 3.5 0 0 1 7 0v3',
  snow: 'M12 3v18 M4.2 7.5l15.6 9 M4.2 16.5l15.6-9',
  mirror: 'M12 3v18 M9 7L4 12l5 5z M15 7l5 5-5 5z',
  flip: 'M12 3v18 M8 6H5v12h3 M16 6h3v12h-3',
  focus: 'M4 9V4h5 M15 4h5v5 M20 15v5h-5 M9 20H4v-5',
  download: 'M12 4v11 M7 10l5 5 5-5 M5 20h14',
  upload: 'M12 20V9 M7 14l5-5 5 5 M5 4h14',
  layers: 'M12 4l8 4-8 4-8-4z M4 12l8 4 8-4 M4 16l8 4 8-4',
  ruler: 'M4 16L16 4l4 4L8 20z M8 12l1.5 1.5 M11 9l1.5 1.5',
  palette: 'M12 3a9 9 0 1 0 0 18c1 0 1.5-.8 1.5-1.6 0-1.2-1-1.4-1-2.4 0-.8.7-1.5 1.6-1.5H16a5 5 0 0 0 5-5c0-4.3-4-7.5-9-7.5z M7.5 11.5h.01 M10 7.5h.01 M14.5 7.5h.01',
  doc: 'M6 3h9l4 4v14H6z M14 3v5h5 M9 13h7 M9 17h5',
  users: 'M9 8m-3 0a3 3 0 1 0 6 0a3 3 0 1 0-6 0 M3 20c0-3.3 2.7-6 6-6s6 2.7 6 6 M17 11a3 3 0 0 0 0-6 M21 20c0-2.8-1.7-5-4-5.7',
  history: 'M3 12a9 9 0 1 0 3-6.7 M3 4v5h5 M12 7v5l3 2',
  settings: 'M12 12m-3 0a3 3 0 1 0 6 0a3 3 0 1 0-6 0 M12 2v3 M12 19v3 M4.9 4.9l2.1 2.1 M17 17l2.1 2.1 M2 12h3 M19 12h3 M4.9 19.1L7 17 M17 7l2.1-2.1',
  split: 'M4 4h16v16H4z M12 4v16',
  only3d: 'M12 3l8 4.5v9L12 21l-8-4.5v-9z M12 12l8-4.5 M12 12v9 M12 12L4 7.5',
  only2d: 'M5 5h14v14H5z M9 9h6v6H9z',
  rotate: 'M20 12a8 8 0 1 1-2.3-5.7 M20 4v4h-4',
  dims: 'M4 8h16 M4 5v6 M20 5v6 M4 8l2.6-1.8 M4 8l2.6 1.8 M20 8l-2.6-1.8 M20 8l-2.6 1.8 M7 14h10v6H7z',
  pdf: 'M7 3h7l4 4v14H7z M14 3v4h4 M10 12v5 M10 17l-2-2 M10 17l2-2',
  dot: 'M12 12m-1 0a1 1 0 1 0 2 0a1 1 0 1 0-2 0'
};
AT.Icon = function (props) {
  const d = AT.ICONS[props.name] || AT.ICONS.dot;
  const s = props.size || 18;
  return React.createElement('svg', { width: s, height: s, viewBox: '0 0 24 24', fill: 'none', stroke: 'currentColor', strokeWidth: props.sw || 1.5, strokeLinecap: 'round', strokeLinejoin: 'round', style: props.style, 'aria-hidden': true },
    d.split(' M').map(function (seg, i) { return React.createElement('path', { key: i, d: (i ? 'M' : '') + seg }); }));
};

/* ---------- procedural fabric textures ---------- */
AT.WEAVES = [['plain', 'Plain weave'], ['twill', 'Twill'], ['denim', 'Denim twill'], ['satin', 'Satin'], ['jersey', 'Jersey knit'], ['rib', 'Rib knit'], ['herringbone', 'Herringbone'], ['corduroy', 'Corduroy'], ['check', 'Gingham check'], ['stripe', 'Stripe'], ['plaid', 'Plaid / tartan'], ['quilt', 'Quilted'], ['leather', 'Leather grain'], ['fleece', 'Fleece'], ['none', 'Solid']];
AT.PRINT_WEAVES = { check: 1, stripe: 1, plaid: 1, quilt: 1 };
AT.rng = function (seed) { let s = seed >>> 0 || 1; return function () { s ^= s << 13; s ^= s >>> 17; s ^= s << 5; return ((s >>> 0) % 100000) / 100000; }; };
AT.shade = function (hex, f) {
  const c = parseInt((hex || '#888888').slice(1), 16), r = (c >> 16) & 255, g = (c >> 8) & 255, b = c & 255;
  const m = function (v) { return Math.round(AT.clamp(f >= 0 ? v + (255 - v) * f : v * (1 + f), 0, 255)); };
  return 'rgb(' + m(r) + ',' + m(g) + ',' + m(b) + ')';
};
AT.fabricTile = function (fab, size, bump) {
  size = size || 256;
  const c = document.createElement('canvas'); c.width = c.height = size;
  const g = c.getContext('2d');
  const base = bump ? '#808080' : (fab.color || '#cccccc'), col2 = bump ? '#a0a0a0' : (fab.color2 || '#ffffff');
  const dk = function (f) { return bump ? 'rgba(0,0,0,' + (Math.abs(f) * 1.6) + ')' : AT.shade(base, f); };
  const S = size, rnd = AT.rng(7 + (fab.weave || '').length * 131);
  g.fillStyle = base; g.fillRect(0, 0, S, S);
  const lines = function (n, ang, color, w) {
    g.save(); g.strokeStyle = color; g.lineWidth = w;
    for (let i = -n; i < 2 * n; i++) {
      const o = i * S / n; g.beginPath();
      if (ang === 'v') { g.moveTo(o, 0); g.lineTo(o, S); }
      else if (ang === 'h') { g.moveTo(0, o); g.lineTo(S, o); }
      else if (ang === 'd') { g.moveTo(o, 0); g.lineTo(o + S, S); }
      else { g.moveTo(o, S); g.lineTo(o + S, 0); }
      g.stroke();
    }
    g.restore();
  };
  const noise = function (amt, n, sz) { for (let i = 0; i < n; i++) { g.fillStyle = rnd() > 0.5 ? 'rgba(255,255,255,' + amt * rnd() + ')' : 'rgba(0,0,0,' + amt * rnd() + ')'; g.fillRect(rnd() * S, rnd() * S, sz, sz); } };
  switch (fab.weave) {
    case 'plain': lines(16, 'v', 'rgba(0,0,0,0.07)', S / 64); lines(16, 'h', 'rgba(255,255,255,0.07)', S / 64); noise(0.05, 600, 2); break;
    case 'twill': lines(12, 'd', 'rgba(0,0,0,0.13)', S / 30); lines(12, 'd', 'rgba(255,255,255,0.05)', S / 90); noise(0.04, 500, 2); break;
    case 'denim':
      lines(14, 'd', bump ? 'rgba(255,255,255,0.4)' : 'rgba(255,255,255,0.22)', S / 34); lines(14, 'd', 'rgba(0,0,0,0.18)', S / 70);
      noise(0.12, 1800, 2); break;
    case 'satin':
      { const gr = g.createLinearGradient(0, 0, S, S); gr.addColorStop(0, 'rgba(255,255,255,0.07)'); gr.addColorStop(0.5, 'rgba(255,255,255,0)'); gr.addColorStop(1, 'rgba(255,255,255,0.07)'); g.fillStyle = gr; g.fillRect(0, 0, S, S); }
      lines(32, 'h', 'rgba(0,0,0,0.035)', 1); break;
    case 'jersey':
      g.strokeStyle = 'rgba(0,0,0,0.16)'; g.lineWidth = S / 64;
      for (let cx = 0; cx < 8; cx++) for (let cy = 0; cy < 8; cy++) { const x0 = cx * S / 8, y0 = cy * S / 8; g.beginPath(); g.moveTo(x0 + S / 32, y0 + S / 40); g.lineTo(x0 + S / 16, y0 + S / 10); g.lineTo(x0 + S * 3 / 32, y0 + S / 40); g.stroke(); }
      noise(0.04, 400, 2); break;
    case 'rib': for (let i = 0; i < 8; i++) { const gr = g.createLinearGradient(i * S / 8, 0, (i + 1) * S / 8, 0); gr.addColorStop(0, 'rgba(0,0,0,0.18)'); gr.addColorStop(0.5, 'rgba(255,255,255,0.1)'); gr.addColorStop(1, 'rgba(0,0,0,0.18)'); g.fillStyle = gr; g.fillRect(i * S / 8, 0, S / 8, S); } break;
    case 'herringbone':
      g.lineWidth = S / 26;
      for (let b = 0; b < 4; b++) { g.strokeStyle = 'rgba(0,0,0,0.14)'; for (let i = -4; i < 8; i++) { g.beginPath(); const x0 = b * S / 4; if (b % 2) { g.moveTo(x0, i * S / 4); g.lineTo(x0 + S / 4, i * S / 4 + S / 4); } else { g.moveTo(x0, i * S / 4 + S / 4); g.lineTo(x0 + S / 4, i * S / 4); } g.stroke(); } }
      noise(0.04, 500, 2); break;
    case 'corduroy': for (let i = 0; i < 6; i++) { const gr = g.createLinearGradient(i * S / 6, 0, (i + 1) * S / 6, 0); gr.addColorStop(0, 'rgba(0,0,0,0.3)'); gr.addColorStop(0.35, 'rgba(255,255,255,0.12)'); gr.addColorStop(0.65, 'rgba(255,255,255,0.05)'); gr.addColorStop(1, 'rgba(0,0,0,0.3)'); g.fillStyle = gr; g.fillRect(i * S / 6, 0, S / 6, S); } noise(0.05, 600, 2); break;
    case 'check':
      g.fillStyle = bump ? 'rgba(0,0,0,0)' : col2; g.globalAlpha = 0.55; g.fillRect(0, 0, S / 2, S); g.fillRect(0, 0, S, S / 2); g.globalAlpha = 0.45; g.fillRect(0, 0, S / 2, S / 2); g.globalAlpha = 1;
      lines(24, 'd', 'rgba(0,0,0,0.05)', 1); break;
    case 'stripe': g.fillStyle = col2; g.fillRect(0, 0, S * 0.3, S); lines(32, 'h', 'rgba(0,0,0,0.04)', 1); break;
    case 'plaid':
      g.globalAlpha = 0.5; g.fillStyle = col2; g.fillRect(S * 0.1, 0, S * 0.18, S); g.fillRect(0, S * 0.1, S, S * 0.18);
      g.fillStyle = dk(-0.35); g.fillRect(S * 0.55, 0, S * 0.3, S); g.fillRect(0, S * 0.55, S, S * 0.3);
      g.globalAlpha = 0.8; g.fillStyle = bump ? '#999' : '#f2e6c8'; g.fillRect(S * 0.42, 0, S * 0.02, S); g.fillRect(0, S * 0.42, S, S * 0.02); g.globalAlpha = 1;
      lines(28, 'd', 'rgba(0,0,0,0.06)', 1); break;
    case 'quilt': g.strokeStyle = bump ? 'rgba(0,0,0,0.6)' : 'rgba(0,0,0,0.25)'; g.lineWidth = S / 40; g.setLineDash([S / 30, S / 50]); g.beginPath(); g.moveTo(0, 0); g.lineTo(S, S); g.moveTo(S, 0); g.lineTo(0, S); g.stroke(); g.setLineDash([]);
      { const gr = g.createRadialGradient(S / 2, S / 4, 0, S / 2, S / 4, S / 3); gr.addColorStop(0, 'rgba(255,255,255,0.14)'); gr.addColorStop(1, 'rgba(255,255,255,0)'); g.fillStyle = gr; g.fillRect(0, 0, S, S); } break;
    case 'leather': for (let i = 0; i < 900; i++) { g.fillStyle = 'rgba(0,0,0,' + (0.05 + rnd() * 0.08) + ')'; g.beginPath(); g.arc(rnd() * S, rnd() * S, 1 + rnd() * 3, 0, 6.3); g.fill(); } noise(0.06, 800, 1); break;
    case 'fleece': noise(0.12, 4000, 1.5); break;
    default: noise(0.02, 200, 2);
  }
  return c;
};
AT.fabricKey = function (fab) { return [fab.color, fab.color2, fab.weave, fab.texture && fab.texture.path].join('|'); };

/* ---------- export: DXF (AAMA-style), SVG, OBJ ---------- */
AT.exportDXF = function (doc) {
  const out = [];
  const w = function (c, v) { out.push(String(c)); out.push(String(v)); };
  const poly = function (pts, layer, closed) {
    if (pts.length < 2) return;
    w(0, 'POLYLINE'); w(8, layer); w(66, 1); w(70, closed ? 1 : 0);
    pts.forEach(function (p) { w(0, 'VERTEX'); w(8, layer); w(10, p.x.toFixed(3)); w(20, (-p.y).toFixed(3)); w(30, 0); });
    w(0, 'SEQEND'); w(8, layer);
  };
  const line = function (a, b, layer) { w(0, 'LINE'); w(8, layer); w(10, a.x.toFixed(3)); w(20, (-a.y).toFixed(3)); w(30, 0); w(11, b.x.toFixed(3)); w(21, (-b.y).toFixed(3)); w(31, 0); };
  const text = function (p, s, layer, h) { w(0, 'TEXT'); w(8, layer); w(10, p.x.toFixed(3)); w(20, (-p.y).toFixed(3)); w(30, 0); w(40, h || 6); w(1, s); };
  w(0, 'SECTION'); w(2, 'HEADER'); w(9, '$INSUNITS'); w(70, 4); w(9, '$MEASUREMENT'); w(70, 1); w(0, 'ENDSEC');
  w(0, 'SECTION'); w(2, 'BLOCKS');
  const pieces = AT.asArray(doc.pieces);
  const names = [];
  pieces.forEach(function (pc, i) {
    const pts = AT.gradedPts(pc, doc.activeSize, doc.baseSize), ol = AT.outline(pts);
    const name = AT.slug(pc.name).toUpperCase() + '_' + (i + 1);
    names.push(name);
    w(0, 'BLOCK'); w(8, 0); w(2, name); w(70, 0); w(10, 0); w(20, 0); w(30, 0); w(3, name);
    const cut = AT.cutOutline(pc, ol);
    poly(cut, 1, true);
    poly(ol.poly, 14, true);
    AT.asArray(pc.internals).forEach(function (il) { poly(AT.evalOpen(il.closed ? il.pts.concat([il.pts[0]]) : il.pts), 8, false); });
    const bb = AT.bbox(ol.poly), g = (pc.grain || 0) * AT.deg, L = bb.h * 0.35;
    line({ x: bb.cx - Math.sin(g) * -L, y: bb.cy - Math.cos(g) * L }, { x: bb.cx + Math.sin(g) * -L, y: bb.cy + Math.cos(g) * L }, 7);
    AT.asArray(pc.notches).forEach(function (n) {
      const k = ol.byId[n.s]; if (k == null) return;
      const p = AT.segPoint(ol.segs[k], n.t || 0), nx = -p.ty, ny = p.tx, sa = pc.sa || 0;
      line({ x: p.x, y: p.y }, { x: p.x - nx * (sa + 3) * (AT.area(ol.poly) > 0 ? -1 : 1), y: p.y - ny * (sa + 3) * (AT.area(ol.poly) > 0 ? -1 : 1) }, 4);
    });
    const fab = doc.fabrics[pc.fabric];
    text({ x: bb.cx - 40, y: bb.cy }, 'Piece Name: ' + pc.name, 1, 8);
    text({ x: bb.cx - 40, y: bb.cy + 12 }, 'Size: ' + (doc.activeSize || ''), 1, 6);
    text({ x: bb.cx - 40, y: bb.cy + 22 }, 'Material: ' + (fab ? fab.name : ''), 1, 6);
    text({ x: bb.cx - 40, y: bb.cy + 32 }, 'Quantity: ' + (pc.qty || 1), 1, 6);
    w(0, 'ENDBLK'); w(8, 0);
  });
  w(0, 'ENDSEC');
  w(0, 'SECTION'); w(2, 'ENTITIES');
  names.forEach(function (n) { w(0, 'INSERT'); w(8, 0); w(2, n); w(10, 0); w(20, 0); w(30, 0); });
  w(0, 'ENDSEC'); w(0, 'EOF');
  return out.join('\n');
};
AT.importDXF = function (text) {
  const L = text.split(/\r?\n/);
  const pairs = [];
  for (let i = 0; i + 1 < L.length; i += 2) pairs.push([parseInt(L[i].trim(), 10), L[i + 1].trim()]);
  const blocks = {}; let cur = null, ent = null; const loose = [];
  const target = function () { return cur ? cur.items : loose; };
  let inBlocks = false, inEntities = false;
  for (let i = 0; i < pairs.length; i++) {
    const c = pairs[i][0], v = pairs[i][1];
    if (c === 0) {
      if (ent && ent.type === 'LWPOLYLINE') { target().push(ent); ent = null; }
      if (v === 'SECTION') { const nm = pairs[i + 1] && pairs[i + 1][1]; inBlocks = nm === 'BLOCKS'; inEntities = nm === 'ENTITIES'; continue; }
      if (v === 'BLOCK') { cur = { name: '', items: [] }; ent = { type: 'BLOCK' }; continue; }
      if (v === 'ENDBLK') { if (cur) blocks[cur.name || ('B' + Object.keys(blocks).length)] = cur; cur = null; ent = null; continue; }
      if (v === 'POLYLINE') { ent = { type: 'POLYLINE', layer: '0', pts: [], closed: false }; continue; }
      if (v === 'VERTEX') { if (ent && ent.type === 'POLYLINE') { ent.vtx = { x: 0, y: 0 }; ent.pts.push(ent.vtx); } continue; }
      if (v === 'SEQEND') { if (ent && ent.type === 'POLYLINE') target().push(ent); ent = null; continue; }
      if (v === 'LWPOLYLINE') { ent = { type: 'LWPOLYLINE', layer: '0', pts: [], closed: false }; continue; }
      if (v === 'LINE') { ent = { type: 'LINE', layer: '0', a: { x: 0, y: 0 }, b: { x: 0, y: 0 } }; target().push(ent); continue; }
      if (v === 'TEXT') { ent = { type: 'TEXT', layer: '0', s: '' }; target().push(ent); continue; }
      if (ent && ent.type !== 'POLYLINE') ent = null;
      continue;
    }
    if (!ent) continue;
    if (ent.type === 'BLOCK' && c === 2 && cur) cur.name = v;
    if (c === 8) { if (ent.type === 'POLYLINE' && ent.vtx) { } else ent.layer = v; }
    if (ent.type === 'POLYLINE') { if (c === 70 && !ent.vtx) ent.closed = (parseInt(v, 10) & 1) === 1; if (ent.vtx && c === 10) ent.vtx.x = parseFloat(v); if (ent.vtx && c === 20) ent.vtx.y = -parseFloat(v); }
    if (ent.type === 'LWPOLYLINE') { if (c === 70) ent.closed = (parseInt(v, 10) & 1) === 1; if (c === 10) ent.pts.push({ x: parseFloat(v), y: 0 }); if (c === 20 && ent.pts.length) ent.pts[ent.pts.length - 1].y = -parseFloat(v); }
    if (ent.type === 'LINE') { if (c === 10) ent.a.x = parseFloat(v); if (c === 20) ent.a.y = -parseFloat(v); if (c === 11) ent.b.x = parseFloat(v); if (c === 21) ent.b.y = -parseFloat(v); }
    if (ent.type === 'TEXT' && c === 1) ent.s = v;
  }
  let groups = Object.keys(blocks).map(function (k) { return { name: k, items: blocks[k].items }; }).filter(function (g) { return g.items.some(function (e) { return e.pts && e.pts.length > 2; }); });
  if (!groups.length) {
    // no blocks: every closed polyline is a piece
    groups = loose.filter(function (e) { return e.pts && e.pts.length > 2 && (e.closed || Math.hypot(e.pts[0].x - e.pts[e.pts.length - 1].x, e.pts[0].y - e.pts[e.pts.length - 1].y) < 0.5); }).map(function (e, i) { return { name: 'Piece ' + (i + 1), items: [e] }; });
  }
  const simplify = function (pts, tol) {
    if (pts.length < 4) return pts;
    const keep = new Uint8Array(pts.length); keep[0] = keep[pts.length - 1] = 1;
    const rec = function (a, b) { let md = 0, mi = -1; for (let i = a + 1; i < b; i++) { const d = AT.distSeg(pts[i].x, pts[i].y, pts[a].x, pts[a].y, pts[b].x, pts[b].y).d; if (d > md) { md = d; mi = i; } } if (md > tol && mi > 0) { keep[mi] = 1; rec(a, mi); rec(mi, b); } };
    rec(0, pts.length - 1);
    return pts.filter(function (_, i) { return keep[i]; });
  };
  const pieces = [];
  groups.forEach(function (g) {
    const polys = g.items.filter(function (e) { return e.pts && e.pts.length > 2; });
    let net = polys.find(function (e) { return e.layer === '14'; }) || polys.find(function (e) { return e.layer === '1'; });
    if (!net) net = polys.slice().sort(function (a, b) { return Math.abs(AT.area(b.pts)) - Math.abs(AT.area(a.pts)); })[0];
    if (!net) return;
    let pts = net.pts.slice();
    if (Math.hypot(pts[0].x - pts[pts.length - 1].x, pts[0].y - pts[pts.length - 1].y) < 0.5) pts.pop();
    pts = simplify(pts.concat([pts[0]]), 0.4); pts.pop();
    const nm = (g.items.find(function (e) { return e.type === 'TEXT' && /piece name/i.test(e.s); }) || {}).s;
    const internals = polys.filter(function (e) { return e !== net && e.layer === '8'; }).map(function (e) { return { id: AT.uid('il'), pts: simplify(e.pts, 0.4).map(function (p) { return { x: p.x, y: p.y }; }), closed: !!e.closed }; });
    pieces.push({ name: nm ? nm.replace(/piece name:\s*/i, '') : g.name, pts: pts.map(function (p, i) { return { id: 'p' + i, x: p.x, y: p.y }; }), internals: internals });
  });
  return pieces;
};
AT.exportSVG = function (doc) {
  const pieces = AT.asArray(doc.pieces);
  let all = [];
  const items = pieces.map(function (pc) {
    const ol = AT.outline(AT.gradedPts(pc, doc.activeSize, doc.baseSize));
    const cut = AT.cutOutline(pc, ol);
    all = all.concat(cut);
    return { pc: pc, ol: ol, cut: cut };
  });
  const bb = AT.bbox(all.length ? all : [{ x: 0, y: 0 }, { x: 100, y: 100 }]);
  const pad = 20, P = function (pts) { return pts.map(function (p) { return (p.x - bb.x0 + pad).toFixed(2) + ',' + (p.y - bb.y0 + pad).toFixed(2); }).join(' '); };
  let s = '<svg xmlns="http://www.w3.org/2000/svg" width="' + (bb.w + 2 * pad).toFixed(1) + 'mm" height="' + (bb.h + 2 * pad).toFixed(1) + 'mm" viewBox="0 0 ' + (bb.w + 2 * pad).toFixed(1) + ' ' + (bb.h + 2 * pad).toFixed(1) + '">\n';
  items.forEach(function (it) {
    s += '<g id="' + AT.slug(it.pc.name) + '">';
    s += '<polygon points="' + P(it.cut) + '" fill="none" stroke="#000" stroke-width="0.5"/>';
    s += '<polygon points="' + P(it.ol.poly) + '" fill="none" stroke="#000" stroke-width="0.3" stroke-dasharray="3 2"/>';
    AT.asArray(it.pc.internals).forEach(function (il) { s += '<polyline points="' + P(AT.evalOpen(il.pts)) + '" fill="none" stroke="#555" stroke-width="0.3" stroke-dasharray="1 1"/>'; });
    const b = AT.bbox(it.ol.poly);
    s += '<text x="' + (b.cx - bb.x0 + pad).toFixed(1) + '" y="' + (b.cy - bb.y0 + pad).toFixed(1) + '" font-size="8" text-anchor="middle" font-family="Helvetica">' + String(it.pc.name).replace(/[<&]/g, '') + '</text>';
    s += '</g>\n';
  });
  return s + '</svg>';
};
AT.exportOBJ = function (doc, sim, avMesh, withAvatar) {
  let obj = '# Atelier export — units: metres\nmtllib garment.mtl\n', mtl = '';
  let vo = 1;
  const S = 0.001;
  let gb = { x0: Infinity, y0: Infinity, x1: -Infinity, y1: -Infinity };
  sim.recs.forEach(function (r) { for (let i = 0; i < r.n; i++) { const x = r.mesh.xy[2 * i], y = r.mesh.xy[2 * i + 1]; gb.x0 = Math.min(gb.x0, x); gb.y0 = Math.min(gb.y0, y); gb.x1 = Math.max(gb.x1, x); gb.y1 = Math.max(gb.y1, y); } });
  const span = Math.max(gb.x1 - gb.x0, gb.y1 - gb.y0) || 1;
  const mats = {};
  AT.computeNormals(sim);
  sim.recs.forEach(function (r) {
    if (r.pc.hide3d) return;
    const fab = AT.effFabric(doc, r.pc.fabric) || { name: 'fabric', color: '#cccccc' };
    const mn = AT.slug(fab.name) + '_' + (r.pc.fabric || 'x');
    if (!mats[mn]) { const c = parseInt((fab.color || '#cccccc').slice(1), 16); mats[mn] = 1; mtl += 'newmtl ' + mn + '\nKd ' + (((c >> 16) & 255) / 255).toFixed(4) + ' ' + (((c >> 8) & 255) / 255).toFixed(4) + ' ' + ((c & 255) / 255).toFixed(4) + '\nNs 10\nd ' + (fab.opacity || 1) + '\n\n'; }
    obj += 'o ' + AT.slug(r.pc.name) + '\nusemtl ' + mn + '\n';
    for (let i = 0; i < r.n; i++) { const k = (r.off + i) * 3; obj += 'v ' + (sim.x[k] * S).toFixed(5) + ' ' + (sim.x[k + 1] * S).toFixed(5) + ' ' + (sim.x[k + 2] * S).toFixed(5) + '\n'; }
    for (let i = 0; i < r.n; i++) obj += 'vt ' + ((r.mesh.xy[2 * i] - gb.x0) / span).toFixed(5) + ' ' + (1 - (r.mesh.xy[2 * i + 1] - gb.y0) / span).toFixed(5) + '\n';
    for (let i = 0; i < r.n; i++) { const k = (r.off + i) * 3; obj += 'vn ' + sim.nrm[k].toFixed(4) + ' ' + sim.nrm[k + 1].toFixed(4) + ' ' + sim.nrm[k + 2].toFixed(4) + '\n'; }
    const T = r.mesh.tris;
    for (let t = 0; t < T.length; t += 3) { const a = T[t] + vo, b = T[t + 1] + vo, c = T[t + 2] + vo; obj += 'f ' + a + '/' + a + '/' + a + ' ' + b + '/' + b + '/' + b + ' ' + c + '/' + c + '/' + c + '\n'; }
    vo += r.n;
  });
  if (withAvatar && avMesh) {
    obj += 'o avatar\nusemtl avatar\n'; mtl += 'newmtl avatar\nKd 0.8 0.78 0.75\n\n';
    const P = avMesh.pos, N = avMesh.nor, I = avMesh.idx, n = P.length / 3;
    for (let i = 0; i < n; i++) obj += 'v ' + (P[3 * i] * S).toFixed(5) + ' ' + (P[3 * i + 1] * S).toFixed(5) + ' ' + (P[3 * i + 2] * S).toFixed(5) + '\n';
    for (let i = 0; i < n; i++) obj += 'vn ' + N[3 * i].toFixed(4) + ' ' + N[3 * i + 1].toFixed(4) + ' ' + N[3 * i + 2].toFixed(4) + '\n';
    for (let t = 0; t < I.length; t += 3) { const a = I[t] + vo, b = I[t + 1] + vo, c = I[t + 2] + vo; obj += 'f ' + a + '//' + a + ' ' + b + '//' + b + ' ' + c + '//' + c + '\n'; }
  }
  return { obj: obj, mtl: mtl };
};


/* ============================================================
   3D GARMENT WINDOW (three.js r128)
   ============================================================ */
AT.FIT_COLORS = ['#2b4bd9', '#2f8de0', '#2fc2c9', '#3fcf7a', '#c9d93a', '#f2b33a', '#f06a32', '#d9262c'];
AT.fitColor = function (t) {
  t = AT.clamp(t, 0, 1) * (AT.FIT_COLORS.length - 1);
  const i = Math.floor(t), f = t - i, a = AT.FIT_COLORS[i], b = AT.FIT_COLORS[Math.min(i + 1, AT.FIT_COLORS.length - 1)];
  const pa = parseInt(a.slice(1), 16), pb = parseInt(b.slice(1), 16);
  return [(((pa >> 16) & 255) * (1 - f) + ((pb >> 16) & 255) * f) / 255, (((pa >> 8) & 255) * (1 - f) + ((pb >> 8) & 255) * f) / 255, ((pa & 255) * (1 - f) + (pb & 255) * f) / 255];
};
AT.Scene3D = class {
  constructor(host, cb) {
    const T = window.THREE; this.T = T; this.cb = cb; this.host = host;
    const r = new T.WebGLRenderer({ antialias: true, alpha: true, preserveDrawingBuffer: true, powerPreference: 'high-performance' });
    r.setPixelRatio(Math.min(2, window.devicePixelRatio || 1));
    r.outputEncoding = T.sRGBEncoding; r.toneMapping = T.ACESFilmicToneMapping; r.toneMappingExposure = 1.0;
    r.shadowMap.enabled = true; r.shadowMap.type = T.PCFSoftShadowMap;
    r.domElement.style.cssText = 'position:absolute;inset:0;width:100%;height:100%;display:block;outline:none;touch-action:none';
    host.appendChild(r.domElement); this.renderer = r; this.canvas = r.domElement;
    const sc = new T.Scene(); this.scene = sc;
    this.camera = new T.PerspectiveCamera(28, 1, 20, 40000);
    this.orb = { tx: 0, ty: 1000, tz: 0, dist: 4300, theta: 0, phi: 1.5 };
    sc.add(new T.HemisphereLight(0xffffff, 0x8a8478, 0.55));
    const key = new T.DirectionalLight(0xffffff, 1.05); key.position.set(1400, 3200, 2200); key.castShadow = true;
    key.shadow.mapSize.set(2048, 2048); const sc2 = key.shadow.camera; sc2.left = -1100; sc2.right = 1100; sc2.top = 2000; sc2.bottom = -200; sc2.near = 100; sc2.far = 8000; key.shadow.bias = -0.0004; key.shadow.radius = 4;
    sc.add(key); this.key = key;
    const fill = new T.DirectionalLight(0xdfe6ff, 0.42); fill.position.set(-2400, 1600, -900); sc.add(fill);
    const rim = new T.DirectionalLight(0xffffff, 0.35); rim.position.set(0, 1800, -3200); sc.add(rim);
    const floor = new T.Mesh(new T.CircleGeometry(1600, 64), new T.ShadowMaterial({ opacity: 0.16 }));
    floor.rotation.x = -Math.PI / 2; floor.receiveShadow = true; sc.add(floor);
    const ring = new T.Mesh(new T.RingGeometry(598, 600, 128), new T.MeshBasicMaterial({ color: 0x888888, transparent: true, opacity: 0.25 }));
    ring.rotation.x = -Math.PI / 2; ring.position.y = 0.5; sc.add(ring); this.ring = ring;
    this.avatarGroup = new T.Group(); this.garmentGroup = new T.Group(); this.overlay = new T.Group();
    sc.add(this.avatarGroup); sc.add(this.garmentGroup); sc.add(this.overlay);
    this.pieceMeshes = []; this.texCache = {}; this.decalCache = {}; this.imgCache = {};
    this.disp = { avatar: true, wire: false, fit: null, seams: true, arrPts: false, xray: false, pins: true, buttons: true, textures: true };
    this.raycaster = new T.Raycaster(); this.mouse = new T.Vector2();
    this.sel = {}; this.hoverRi = -1;
    this.ptr = null; this.alive = true; this.lastT = performance.now(); this.fps = 0; this._fc = 0; this._ft = performance.now();
    this._bind();
    this.ro = new ResizeObserver(() => this.resize()); this.ro.observe(host); this.resize();
    const loop = () => { if (!this.alive) return; try { this.frame(); } catch (e) { AT.reportError('3D frame', e); } this.raf = requestAnimationFrame(loop); };
    this.raf = requestAnimationFrame(loop);
  }
  dispose() {
    this.alive = false; cancelAnimationFrame(this.raf); this.ro.disconnect();
    this._unbind();
    Object.values(this.texCache).forEach((t) => { t.map && t.map.dispose(); t.bump && t.bump.dispose(); });
    this.renderer.dispose(); this.canvas.remove();
  }
  resize() {
    const w = Math.max(10, this.host.clientWidth), h = Math.max(10, this.host.clientHeight);
    this.renderer.setSize(w, h, false); this.camera.aspect = w / h; this.camera.updateProjectionMatrix(); this.w = w; this.h = h;
  }
  setTheme(dark) { this.dark = dark; this.ring.material.color.set(dark ? 0x999999 : 0x777777); }
  updateCamera() {
    const o = this.orb, sp = Math.sin(o.phi);
    this.camera.position.set(o.tx + o.dist * sp * Math.sin(o.theta), o.ty + o.dist * Math.cos(o.phi), o.tz + o.dist * sp * Math.cos(o.theta));
    this.camera.lookAt(o.tx, o.ty, o.tz);
  }
  view(name) {
    const o = this.orb, V = { front: [0, 1.5], back: [Math.PI, 1.5], left: [Math.PI / 2, 1.5], right: [-Math.PI / 2, 1.5], q3l: [Math.PI / 4, 1.42], q3r: [-Math.PI / 4, 1.42], top: [0, 0.05], bottom: [0, Math.PI - 0.05] };
    const v = V[name] || V.front; this.anim = { from: [o.theta, o.phi], to: [v[0], v[1]], t: 0 };
  }
  focus(box) {
    const o = this.orb;
    if (!box) { o.tx = 0; o.ty = this.avH ? this.avH * 0.55 : 1000; o.tz = 0; o.dist = this.avH ? this.avH * 2.55 : 4300; return; }
    o.tx = (box[0] + box[3]) / 2; o.ty = (box[1] + box[4]) / 2; o.tz = (box[2] + box[5]) / 2;
    const r = Math.max(box[3] - box[0], box[4] - box[1], box[5] - box[2]);
    o.dist = Math.max(500, r * 2.4);
  }
  // ---------- avatar ----------
  setAvatar(av, mesh, skin) {
    const T = this.T;
    this.av = av; this.avH = av.H;
    this.avatarGroup.clear();
    const g = new T.BufferGeometry();
    g.setAttribute('position', new T.BufferAttribute(mesh.pos, 3)); g.setAttribute('normal', new T.BufferAttribute(mesh.nor, 3)); g.setIndex(new T.BufferAttribute(mesh.idx, 1));
    const mat = new T.MeshStandardMaterial({ color: new T.Color(skin || '#cfc9c1'), roughness: 0.62, metalness: 0.0 });
    const m = new T.Mesh(g, mat); m.castShadow = true; m.receiveShadow = true; this.avatarMesh = m; this.avatarGroup.add(m);
    this.setArrangementPoints();
    this.setTapes(av.tapes || []);
    if (!this._focused) { this.focus(null); this._focused = true; }
    this.applyDisplay();
  }
  setSkin(c) { if (this.avatarMesh) this.avatarMesh.material.color.set(c); }
  setTapes(tapes) {
    const T = this.T;
    if (this.tapeGroup) { this.overlay.remove(this.tapeGroup); this.tapeGroup.children.forEach((l) => { l.geometry.dispose(); l.material.dispose(); }); }
    const g = new T.Group();
    (tapes || []).forEach((tp) => {
      if (!tp.pts || tp.pts.length < 2) return;
      const geo = new T.BufferGeometry().setFromPoints(tp.pts.map((p) => new T.Vector3(p[0], p[1], p[2])));
      const mat = new T.LineBasicMaterial({ color: 0xE8654A, transparent: true, opacity: 0.9, depthTest: false });
      const line = tp.closed ? new T.LineLoop(geo, mat) : new T.Line(geo, mat);
      line.userData.k = tp.k; line.renderOrder = 12; g.add(line);
    });
    this.tapeGroup = g; this.overlay.add(g); g.visible = !!this.disp.tapes;
    this.highlightTape(this._tapeKey);
  }
  highlightTape(k) {
    this._tapeKey = k;
    if (!this.tapeGroup) return;
    this.tapeGroup.children.forEach((l) => { const on = !k || l.userData.k === k; l.material.color.set(l.userData.k === k ? '#ffffff' : '#E8654A'); l.material.opacity = on ? 0.95 : 0.3; });
  }
  setArrangementPoints() {
    const T = this.T;
    if (this.arrMesh) { this.overlay.remove(this.arrMesh); }
    if (!this.av) return;
    const pts = AT.arrangementPoints(this.av); this.arrPts = pts;
    const im = new T.InstancedMesh(new T.SphereGeometry(13, 16, 12), new T.MeshBasicMaterial({ color: 0x3b82f6, transparent: true, opacity: 0.9, depthTest: false }), pts.length);
    const M = new T.Matrix4(); pts.forEach((p, i) => { M.makeTranslation(p.pos[0], p.pos[1], p.pos[2]); im.setMatrixAt(i, M); });
    im.renderOrder = 10; this.arrMesh = im; this.overlay.add(im); im.visible = this.disp.arrPts;
  }
  // ---------- garment ----------
  setSim(sim, doc) {
    const T = this.T;
    this.sim = sim; this.doc = doc;
    this.pieceMeshes.forEach((pm) => { pm.mesh.geometry.dispose(); if (pm.decal) { pm.decal.geometry.dispose(); pm.decal.material.dispose(); } pm.mesh.material.dispose(); });
    this.garmentGroup.clear(); this.pieceMeshes = [];
    if (!sim) return;
    sim.recs.forEach((r, ri) => {
      const g = new T.BufferGeometry();
      const pos = new T.BufferAttribute(sim.x.subarray(r.off * 3, (r.off + r.n) * 3), 3); pos.setUsage(T.DynamicDrawUsage);
      g.setAttribute('position', pos); g.setIndex(new T.BufferAttribute(r.mesh.tris, 1));
      g.setAttribute('uv', new T.BufferAttribute(new Float32Array(r.n * 2), 2));
      g.setAttribute('color', new T.BufferAttribute(new Float32Array(r.n * 3).fill(1), 3));
      g.computeVertexNormals();
      const mat = new T.MeshStandardMaterial({ side: T.DoubleSide, roughness: 0.85 });
      const mesh = new T.Mesh(g, mat); mesh.castShadow = true; mesh.receiveShadow = true; mesh.userData.ri = ri; mesh.frustumCulled = false;
      this.garmentGroup.add(mesh);
      this.pieceMeshes.push({ r: r, ri: ri, mesh: mesh, geo: g });
    });
    // sewing lines
    if (this.sewLines) this.overlay.remove(this.sewLines);
    const nL = sim.SW.n;
    const lg = new T.BufferGeometry();
    lg.setAttribute('position', new T.BufferAttribute(new Float32Array(Math.max(1, nL) * 6), 3));
    const lc = new Float32Array(Math.max(1, nL) * 6);
    for (let c = 0; c < nL; c++) { const sw = sim.SW.list[sim.SW.s[c]]; const col = new T.Color((sw && sw.color) || '#E8654A'); lc.set([col.r, col.g, col.b, col.r, col.g, col.b], c * 6); }
    lg.setAttribute('color', new T.BufferAttribute(lc, 3));
    this.sewLines = new T.LineSegments(lg, new T.LineBasicMaterial({ vertexColors: true, transparent: true, opacity: 0.85, depthTest: false }));
    this.sewLines.renderOrder = 5; this.sewLines.frustumCulled = false; this.overlay.add(this.sewLines);
    // pins
    if (this.pinMesh) this.overlay.remove(this.pinMesh);
    this.pinMesh = new T.InstancedMesh(new T.SphereGeometry(6, 10, 8), new T.MeshBasicMaterial({ color: 0xE8654A, depthTest: false }), Math.max(1, sim.pins.length));
    this.pinMesh.count = sim.pins.length; this.pinMesh.renderOrder = 11; this.overlay.add(this.pinMesh);
    this.refreshMaterials(doc);
    this.setButtons(doc);
    this.applyDisplay();
  }
  pieceUV(r, fab) {
    const pm = this.pieceMeshes.find((p) => p.r === r); if (!pm) return;
    const a = pm.geo.attributes.uv, xy = r.mesh.xy, g = (r.pc.grain || 0) * AT.deg, cg = Math.cos(g), sg = Math.sin(g);
    let scale = fab.scale || 10; if (fab.texture && fab.texture.w) scale = fab.texture.w;
    for (let i = 0; i < r.n; i++) { const x = xy[2 * i], y = xy[2 * i + 1]; a.array[2 * i] = (x * cg + y * sg) / scale; a.array[2 * i + 1] = -(-x * sg + y * cg) / scale; }
    a.needsUpdate = true;
  }
  fabricTex(fab) {
    const T = this.T, tu = AT.resolveAsset(fab.texture), key = AT.fabricKey(fab) + '|' + (tu && this.imgCache[tu] && this.imgCache[tu].complete ? 'img' : '');
    if (this.texCache[key]) return this.texCache[key];
    let map, bump;
    const turl = AT.resolveAsset(fab.texture);
    if (turl) {
      const img = this.imgCache[turl];
      if (img && img.complete && img.naturalWidth) { map = new T.Texture(img); map.needsUpdate = true; }
      else if (!img) { const im = new Image(); im.crossOrigin = 'anonymous'; im.onload = () => { this.texCache = {}; this.refreshMaterials(this.doc); }; im.src = turl; this.imgCache[turl] = im; }
    }
    if (!map) { map = new T.CanvasTexture(AT.fabricTile(fab, 256)); }
    bump = new T.CanvasTexture(AT.fabricTile(fab, 256, true));
    [map, bump].forEach((t) => { t.wrapS = t.wrapT = T.RepeatWrapping; t.anisotropy = 8; });
    map.encoding = T.sRGBEncoding;
    this.texCache[key] = { map: map, bump: bump };
    return this.texCache[key];
  }
  refreshMaterials(doc) {
    const T = this.T; this.doc = doc || this.doc; doc = this.doc;
    if (!doc) return;
    this.pieceMeshes.forEach((pm) => {
      const pc = doc.pieces[pm.r.id] || pm.r.pc;
      const fab = AT.effFabric(doc, pc.fabric) || AT.makeFabric('poplin');
      this.pieceUV(pm.r, fab);
      const tx = this.fabricTex(fab), m = pm.mesh.material;
      m.map = this.disp.textures ? tx.map : null; m.bumpMap = this.disp.textures ? tx.bump : null; m.bumpScale = 0.6;
      m.color.set(this.disp.textures ? '#ffffff' : (fab.color || '#cccccc'));
      
      m.roughness = fab.rough == null ? 0.85 : fab.rough; m.metalness = 0;
      const st = pc.state || 'active';
      m.transparent = (fab.opacity || 1) < 1 || st === 'off' || this.disp.xray;
      m.opacity = st === 'off' ? 0.3 : this.disp.xray ? 0.45 : (fab.opacity || 1);
      m.depthWrite = !m.transparent || m.opacity > 0.9;
      m.emissive.set(st === 'frozen' ? '#1e3a8a' : '#000000'); m.emissiveIntensity = st === 'frozen' ? 0.35 : 0;
      m.wireframe = this.disp.wire;
      m.needsUpdate = true;
      pm.mesh.visible = !pc.hide3d;
      this.buildDecal(pm, pc, doc);
    });
    this.applySelection();
  }
  buildDecal(pm, pc, doc) {
    const T = this.T;
    const stitches = Object.values(doc.stitches || {}).filter((s) => s && s.p === pc.id);
    const holes = Object.values(doc.holes || {}).filter((h) => h && h.p === pc.id);
    const graphics = AT.asArray(pc.graphics);
    const key = AT.stable([stitches, holes, graphics, pc.pts, doc.activeSize]) + (graphics.map((gr) => { const u = AT.resolveAsset(gr); return u && this.imgCache[u] && this.imgCache[u].complete ? 1 : 0; }).join(''));
    if (pm.decalKey === key) return;
    pm.decalKey = key;
    if (pm.decal) { pm.mesh.remove(pm.decal); pm.decal.geometry.dispose(); pm.decal.material.map && pm.decal.material.map.dispose(); pm.decal.material.dispose(); pm.decal = null; }
    if (!stitches.length && !holes.length && !graphics.length) return;
    const r = pm.r, xy = r.mesh.xy;
    const ol = r.mesh.ol, bb = AT.bbox(ol.poly), pad = 4;
    const ppm = Math.min(3, 1600 / Math.max(bb.w + 2 * pad, bb.h + 2 * pad));
    const cw = Math.ceil((bb.w + 2 * pad) * ppm), ch = Math.ceil((bb.h + 2 * pad) * ppm);
    const cv = document.createElement('canvas'); cv.width = cw; cv.height = ch;
    const g = cv.getContext('2d');
    const X = (x) => (x - bb.x0 + pad) * ppm, Y = (y) => (y - bb.y0 + pad) * ppm;
    graphics.forEach((gr) => {
      const gurl = AT.resolveAsset(gr);
      if (!gurl) return;
      let img = this.imgCache[gurl];
      if (!img) { img = new Image(); img.crossOrigin = 'anonymous'; img.onload = () => { pm.decalKey = null; this.buildDecal(pm, this.doc.pieces[pc.id] || pc, this.doc); }; img.src = gurl; this.imgCache[gurl] = img; }
      if (!img.complete || !img.naturalWidth) return;
      g.save(); g.globalAlpha = gr.opacity == null ? 1 : gr.opacity; g.translate(X(gr.x), Y(gr.y)); g.rotate((gr.rot || 0) * AT.deg);
      g.drawImage(img, -gr.w / 2 * ppm, -gr.h / 2 * ppm, gr.w * ppm, gr.h * ppm); g.restore();
    });
    stitches.forEach((st) => AT.drawStitch(g, ol, st, X, Y, ppm, false));
    holes.forEach((h) => {
      g.save(); g.translate(X(h.x), Y(h.y)); g.rotate((h.ang || 0) * AT.deg);
      const L = (h.len || 18) * ppm, W = Math.max(2, 3.2 * ppm);
      g.fillStyle = h.color || '#2a2a2a'; g.beginPath(); g.roundRect ? g.roundRect(-L / 2, -W / 2, L, W, W / 2) : g.rect(-L / 2, -W / 2, L, W); g.fill();
      g.fillStyle = 'rgba(0,0,0,0.65)'; g.fillRect(-L / 2 + W * 0.6, -W * 0.12, L - W * 1.2, W * 0.24); g.restore();
    });
    const tex = new T.CanvasTexture(cv); tex.encoding = T.sRGBEncoding; tex.anisotropy = 8;
    const geo = new T.BufferGeometry();
    geo.setAttribute('position', pm.geo.attributes.position); geo.setIndex(pm.geo.index);
    const uv = new Float32Array(r.n * 2);
    for (let i = 0; i < r.n; i++) { uv[2 * i] = X(xy[2 * i]) / cw; uv[2 * i + 1] = 1 - Y(xy[2 * i + 1]) / ch; }
    geo.setAttribute('uv', new T.BufferAttribute(uv, 2));
    pm.normalShared = geo;
    const mat = new T.MeshStandardMaterial({ map: tex, transparent: true, depthWrite: false, polygonOffset: true, polygonOffsetFactor: -2, polygonOffsetUnits: -2, side: T.DoubleSide, roughness: 0.8 });
    const dm = new T.Mesh(geo, mat); dm.frustumCulled = false; dm.renderOrder = 2;
    pm.decal = dm; pm.mesh.add(dm);
  }
  setButtons(doc) {
    const T = this.T;
    if (this.btnGroup) this.overlay.remove(this.btnGroup);
    this.btnGroup = new T.Group(); this.overlay.add(this.btnGroup);
    this.btnRefs = [];
    if (!this.sim) return;
    Object.values(doc.buttons || {}).forEach((b) => {
      if (!b) return;
      const ri = this.sim.recs.findIndex((r) => r.id === b.p); if (ri < 0) return;
      const r = this.sim.recs[ri], L = AT.locate(r.mesh, b.x, b.y); if (!L) return;
      const d = b.size || 15;
      let geo;
      if (b.shape === 'square') geo = new T.BoxGeometry(d, 2.6, d);
      else if (b.shape === 'shank') geo = new T.SphereGeometry(d / 2, 20, 12);
      else geo = new T.CylinderGeometry(d / 2, d / 2 * 0.94, Math.max(2.2, d * 0.16), 28);
      const mat = new T.MeshStandardMaterial({ color: new T.Color(b.color || '#2b2b2b'), roughness: b.finish === 'metal' ? 0.25 : 0.45, metalness: b.finish === 'metal' ? 0.9 : 0.05 });
      const mesh = new T.Mesh(geo, mat); mesh.castShadow = true;
      if (b.shape !== 'shank' && (b.holes || 4) > 0) {
        const hm = new T.MeshBasicMaterial({ color: 0x111111 });
        const nh = b.holes || 4;
        for (let k = 0; k < nh; k++) { const a = k / nh * Math.PI * 2 + Math.PI / 4; const hh = new T.Mesh(new T.CircleGeometry(d * 0.06, 10), hm); hh.rotation.x = -Math.PI / 2; hh.position.set(Math.cos(a) * d * 0.16, Math.max(1.2, d * 0.085), Math.sin(a) * d * 0.16); mesh.add(hh); }
      }
      this.btnGroup.add(mesh);
      this.btnRefs.push({ mesh: mesh, r: r, L: L, h: Math.max(1.6, d * 0.1) });
    });
  }
  applyDisplay() {
    const d = this.disp;
    if (this.avatarMesh) { this.avatarMesh.visible = d.avatar; this.avatarMesh.material.transparent = d.xray; this.avatarMesh.material.opacity = d.xray ? 0.35 : 1; }
    if (this.arrMesh) this.arrMesh.visible = d.arrPts;
    if (this.sewLines) this.sewLines.visible = d.seams;
    if (this.pinMesh) this.pinMesh.visible = d.pins;
    if (this.btnGroup) this.btnGroup.visible = d.buttons;
    if (this.tapeGroup) this.tapeGroup.visible = !!d.tapes;
  }
  setDisplay(patch) {
    const prevFit = this.disp.fit, prevTex = this.disp.textures, prevWire = this.disp.wire, prevX = this.disp.xray;
    Object.assign(this.disp, patch);
    if (prevFit !== this.disp.fit || prevTex !== this.disp.textures || prevWire !== this.disp.wire || prevX !== this.disp.xray) { this.applyFitMaterial(); this.refreshMaterials(this.doc); }
    this.applyDisplay();
  }
  applyFitMaterial() {
    const on = !!this.disp.fit;
    this.pieceMeshes.forEach((pm) => {
      const m = pm.mesh.material; m.vertexColors = on;
      if (on) { m.map = null; m.bumpMap = null; m.color.set('#ffffff'); }
      if (pm.decal) pm.decal.visible = !on;
      m.needsUpdate = true;
    });
    this._fitTick = 0;
  }
  updateFitColors() {
    if (!this.disp.fit || !this.sim) return;
    const mode = this.disp.fit, vals = AT.fitValues(this.sim, this.doc, mode);
    this.fitVals = vals;
    if (mode === 'fit') this.fitSummary = AT.fitSummary(vals, this.sim);
    this.pieceMeshes.forEach((pm) => {
      const c = pm.geo.attributes.color.array, r = pm.r;
      for (let i = 0; i < r.n; i++) {
        const v = vals[r.off + i];
        let col;
        if (mode === 'strain') col = AT.fitColor((v - 1) / 0.2);
        else if (mode === 'stress') col = AT.fitColor(v / 100);
        else if (mode === 'pressure') col = v ? [0.95, 0.25, 0.2] : [0.93, 0.93, 0.93];
        else col = v >= 1 ? [0.85, 0.15, 0.17] : v >= 0.6 ? [0.98, 0.8, 0.2] : [0.95, 0.95, 0.95];
        c[3 * i] = col[0]; c[3 * i + 1] = col[1]; c[3 * i + 2] = col[2];
      }
      pm.geo.attributes.color.needsUpdate = true;
    });
  }
  applySelection() {
    const S = this.sel || {};
    this.pieceMeshes.forEach((pm) => {
      const m = pm.mesh.material, st = (pm.r.pc && pm.r.pc.state) || 'active';
      if (st === 'frozen') return;
      if (S[pm.r.id]) { m.emissive.set('#E8654A'); m.emissiveIntensity = 0.22; }
      else if (pm.ri === this.hoverRi) { m.emissive.set('#E8654A'); m.emissiveIntensity = 0.08; }
      else { m.emissive.set('#000000'); m.emissiveIntensity = 0; }
    });
  }
  setSelection(ids) { const s = {}; (ids || []).forEach((i) => { s[i] = 1; }); this.sel = s; this.applySelection(); }
  // ---------- per-frame ----------
  frame() {
    const now = performance.now(), dt = Math.min(0.05, (now - this.lastT) / 1000); this.lastT = now;
    if (this.anim) {
      const a = this.anim; a.t = Math.min(1, a.t + dt * 3.2); const e = a.t < 0.5 ? 2 * a.t * a.t : 1 - Math.pow(-2 * a.t + 2, 2) / 2;
      let dth = a.to[0] - a.from[0]; while (dth > Math.PI) dth -= 2 * Math.PI; while (dth < -Math.PI) dth += 2 * Math.PI;
      this.orb.theta = a.from[0] + dth * e; this.orb.phi = a.from[1] + (a.to[1] - a.from[1]) * e; if (a.t >= 1) this.anim = null;
    }
    if (this.turntable) { this.orb.theta += dt * Math.PI * 2 / this.turntable.dur; }
    if (this.cb.tick) this.cb.tick(dt);
    const sim = this.sim;
    if (sim) {
      this.pieceMeshes.forEach((pm) => { pm.geo.attributes.position.needsUpdate = true; pm.geo.computeVertexNormals(); });
      if (this.sewLines && this.disp.seams) {
        const a = this.sewLines.geometry.attributes.position.array, x = sim.x, SW = sim.SW;
        for (let c = 0; c < SW.n; c++) {
          const ia = SW.a[c] * 3, b0 = SW.b0[c] * 3, b1 = SW.b1[c] * 3, u = SW.u[c], o = c * 6;
          const tx = (1 - u) * x[b0] + u * x[b1], ty = (1 - u) * x[b0 + 1] + u * x[b1 + 1], tz = (1 - u) * x[b0 + 2] + u * x[b1 + 2];
          const far = Math.abs(tx - x[ia]) + Math.abs(ty - x[ia + 1]) + Math.abs(tz - x[ia + 2]) > 6;
          a[o] = x[ia]; a[o + 1] = x[ia + 1]; a[o + 2] = x[ia + 2];
          a[o + 3] = far ? tx : x[ia]; a[o + 4] = far ? ty : x[ia + 1]; a[o + 5] = far ? tz : x[ia + 2];
        }
        this.sewLines.geometry.attributes.position.needsUpdate = true;
      }
      if (this.pinMesh && this.disp.pins) {
        const M = new this.T.Matrix4();
        for (let i = 0; i < sim.pins.length; i++) { const k = sim.pins[i] * 3; M.makeTranslation(sim.x[k], sim.x[k + 1], sim.x[k + 2]); this.pinMesh.setMatrixAt(i, M); }
        this.pinMesh.instanceMatrix.needsUpdate = true;
      }
      if (this.btnRefs && this.disp.buttons) this.btnRefs.forEach((b) => {
        const L = b.L, o = b.r.off, x = sim.x, A = (o + L.a) * 3, B = (o + L.b) * 3, C = (o + L.c) * 3;
        const px = L.l1 * x[A] + L.l2 * x[B] + L.l3 * x[C], py = L.l1 * x[A + 1] + L.l2 * x[B + 1] + L.l3 * x[C + 1], pz = L.l1 * x[A + 2] + L.l2 * x[B + 2] + L.l3 * x[C + 2];
        const e1 = [x[B] - x[A], x[B + 1] - x[A + 1], x[B + 2] - x[A + 2]], e2 = [x[C] - x[A], x[C + 1] - x[A + 1], x[C + 2] - x[A + 2]];
        let n = AT.norm(AT.cross(e1, e2));
        // orient outward (away from avatar centre axis)
        if (n[0] * px + n[2] * pz < 0) n = [-n[0], -n[1], -n[2]];
        b.mesh.position.set(px + n[0] * b.h, py + n[1] * b.h, pz + n[2] * b.h);
        b.mesh.quaternion.setFromUnitVectors(new this.T.Vector3(0, 1, 0), new this.T.Vector3(n[0], n[1], n[2]));
      });
      if (this.disp.fit && ((this._fitTick = (this._fitTick || 0) + 1) % 8 === 1)) this.updateFitColors();
    }
    this.updateCamera();
    this.renderer.render(this.scene, this.camera);
    this._fc++; if (now - this._ft > 500) { this.fps = this._fc * 1000 / (now - this._ft); this._fc = 0; this._ft = now; }
  }
  // ---------- picking ----------
  ndc(e) { const rc = this.canvas.getBoundingClientRect(); this.mouse.set(((e.clientX - rc.left) / rc.width) * 2 - 1, -((e.clientY - rc.top) / rc.height) * 2 + 1); return this.mouse; }
  pick(e) {
    this.raycaster.setFromCamera(this.ndc(e), this.camera);
    const meshes = this.pieceMeshes.filter((p) => p.mesh.visible).map((p) => { p.geo.computeBoundingSphere(); return p.mesh; });
    const hits = this.raycaster.intersectObjects(meshes, false);
    if (!hits.length) return null;
    const h = hits[0], ri = h.object.userData.ri, r = this.sim.recs[ri], f = h.face;
    const cand = [f.a, f.b, f.c];
    let best = cand[0], bd = Infinity;
    cand.forEach((v) => { const k = (r.off + v) * 3, d = Math.hypot(this.sim.x[k] - h.point.x, this.sim.x[k + 1] - h.point.y, this.sim.x[k + 2] - h.point.z); if (d < bd) { bd = d; best = v; } });
    return { ri: ri, r: r, v: best, q: r.off + best, point: h.point.clone(), x2: r.mesh.xy[2 * best], y2: r.mesh.xy[2 * best + 1] };
  }
  pickArrangement(e) {
    if (!this.arrMesh || !this.arrMesh.visible) return null;
    this.raycaster.setFromCamera(this.ndc(e), this.camera);
    const h = this.raycaster.intersectObject(this.arrMesh, false);
    return h.length ? this.arrPts[h[0].instanceId] : null;
  }
  planePoint(e, through) {
    const T = this.T; this.raycaster.setFromCamera(this.ndc(e), this.camera);
    const n = new T.Vector3(); this.camera.getWorldDirection(n);
    const pl = new T.Plane().setFromNormalAndCoplanarPoint(n, through);
    const out = new T.Vector3(); return this.raycaster.ray.intersectPlane(pl, out) ? out : null;
  }
  _bind() {
    const c = this.canvas;
    this._h = {
      down: (e) => this.onDown(e), move: (e) => this.onMove(e), up: (e) => this.onUp(e),
      wheel: (e) => { e.preventDefault(); e.stopPropagation(); const f = Math.exp((e.ctrlKey ? e.deltaY * 3 : e.deltaY) * 0.0012); this.orb.dist = AT.clamp(this.orb.dist * f, 300, 20000); },
      ctx: (e) => e.preventDefault()
    };
    c.addEventListener('pointerdown', this._h.down); window.addEventListener('pointermove', this._h.move); window.addEventListener('pointerup', this._h.up);
    c.addEventListener('wheel', this._h.wheel, { passive: false }); c.addEventListener('contextmenu', this._h.ctx);
  }
  _unbind() {
    const c = this.canvas;
    c.removeEventListener('pointerdown', this._h.down); window.removeEventListener('pointermove', this._h.move); window.removeEventListener('pointerup', this._h.up);
    c.removeEventListener('wheel', this._h.wheel); c.removeEventListener('contextmenu', this._h.ctx);
  }
  onDown(e) {
    e.stopPropagation();
    this.cb.focus && this.cb.focus();
    const tool = this.cb.tool ? this.cb.tool() : 'select';
    const T = this.T;
    this.ptr = { x: e.clientX, y: e.clientY, btn: e.button, mode: 'orbit', moved: false, shift: e.shiftKey, alt: e.altKey };
    if (e.button === 1 || (e.button === 2 && e.shiftKey)) { this.ptr.mode = 'pan'; return; }
    if (e.button === 2) { this.ptr.mode = 'orbit'; return; }
    if (e.altKey) { this.ptr.mode = 'orbit'; return; }
    const ap = this.pickArrangement(e);
    if (ap) { this.ptr.mode = 'none'; this.cb.onArrange && this.cb.onArrange(ap); return; }
    const hit = this.sim ? this.pick(e) : null;
    if (!hit) { this.ptr.mode = 'orbit'; this.ptr.empty = true; return; }
    if (tool === 'pin') { this.ptr.mode = 'none'; this.cb.onPin && this.cb.onPin(hit); return; }
    if (tool === 'comment') { this.ptr.mode = 'none'; this.cb.onComment && this.cb.onComment(hit); return; }
    this.cb.onSelect && this.cb.onSelect(hit.r.id, e.shiftKey);
    if (this.cb.simulating && this.cb.simulating()) {
      // grab cloth
      const sim = this.sim, r = hit.r, R = 45, idx = [], wt = [], rel = [];
      const c = [sim.x[hit.q * 3], sim.x[hit.q * 3 + 1], sim.x[hit.q * 3 + 2]];
      for (let i = 0; i < r.n; i++) { const k = (r.off + i) * 3, d = Math.hypot(sim.x[k] - c[0], sim.x[k + 1] - c[1], sim.x[k + 2] - c[2]); if (d < R) { idx.push(r.off + i); const f = 1 - d / R; wt.push(0.5 * f * f); rel.push(sim.x[k] - c[0], sim.x[k + 1] - c[1], sim.x[k + 2] - c[2]); } }
      sim.grab = { idx: idx, wt: wt, rel: rel, target: c.slice() };
      this.ptr.mode = 'grab'; this.ptr.anchor = new T.Vector3(c[0], c[1], c[2]);
    } else {
      this.ptr.mode = 'move'; this.ptr.ri = hit.ri; this.ptr.anchor = hit.point.clone(); this.ptr.last = hit.point.clone();
    }
  }
  onMove(e) {
    const p = this.ptr;
    if (!p) {
      if (e.target === this.canvas && this.sim && !this._hoverPending) {
        this._hoverPending = true;
        requestAnimationFrame(() => { this._hoverPending = false; const h = this.pick(e); const ri = h ? h.ri : -1; if (ri !== this.hoverRi) { this.hoverRi = ri; this.applySelection(); } });
      }
      return;
    }
    const dx = e.clientX - p.x, dy = e.clientY - p.y;
    if (Math.abs(dx) + Math.abs(dy) > 3) p.moved = true;
    if (p.mode === 'orbit') { this.orb.theta -= dx * 0.008; this.orb.phi = AT.clamp(this.orb.phi - dy * 0.008, 0.05, Math.PI - 0.05); this.anim = null; p.x = e.clientX; p.y = e.clientY; }
    else if (p.mode === 'pan') {
      const s = this.orb.dist * 0.0011, o = this.orb, th = o.theta;
      o.tx -= (Math.cos(th) * dx) * s; o.tz -= (-Math.sin(th) * dx) * s; o.ty += dy * s; p.x = e.clientX; p.y = e.clientY;
    } else if (p.mode === 'grab' && this.sim && this.sim.grab) {
      const q = this.planePoint(e, p.anchor); if (q) this.sim.grab.target = [q.x, q.y, q.z];
    } else if (p.mode === 'move' && this.sim) {
      const q = this.planePoint(e, p.last); if (q) { AT.translatePiece(this.sim, p.ri, q.x - p.last.x, q.y - p.last.y, q.z - p.last.z); p.last = q; }
    }
  }
  onUp(e) {
    const p = this.ptr; if (!p) return; this.ptr = null;
    if (p.mode === 'grab' && this.sim) this.sim.grab = null;
    if (p.mode === 'orbit' && p.btn === 2 && !p.moved) this.cb.onContext && this.cb.onContext(e, this.sim ? this.pick(e) : null);
    if (p.mode === 'orbit' && p.empty && !p.moved && p.btn === 0) this.cb.onSelect && this.cb.onSelect(null, e.shiftKey);
  }
  // ---------- capture ----------
  snapshot(scale, bg) {
    const r = this.renderer, w = this.w, h = this.h, s = scale || 2;
    r.setPixelRatio(s); r.setSize(w, h, false);
    const prev = this.renderer.getClearColor(new this.T.Color()), pa = this.renderer.getClearAlpha();
    if (bg) r.setClearColor(new this.T.Color(bg), 1);
    this.updateCamera(); r.render(this.scene, this.camera);
    const url = this.canvas.toDataURL('image/png');
    r.setClearColor(prev, pa); r.setPixelRatio(Math.min(2, window.devicePixelRatio || 1)); r.setSize(w, h, false);
    return url;
  }
  snapshotView(view, scale, bg) {
    const o = Object.assign({}, this.orb), V = { front: [0, 1.5], back: [Math.PI, 1.5], left: [Math.PI / 2, 1.5], right: [-Math.PI / 2, 1.5], q3l: [Math.PI / 4, 1.42] };
    const v = V[view] || V.front; this.anim = null; this.orb.theta = v[0]; this.orb.phi = v[1];
    const url = this.snapshot(scale, bg); Object.assign(this.orb, o); return url;
  }
  recordTurntable(seconds, onDone) {
    if (!window.MediaRecorder || !this.canvas.captureStream) { onDone && onDone(null, 'Video capture is not supported in this browser'); return; }
    const stream = this.canvas.captureStream(60);
    const mime = ['video/webm;codecs=vp9', 'video/webm;codecs=vp8', 'video/webm', 'video/mp4'].find((m) => MediaRecorder.isTypeSupported(m)) || '';
    const rec = new MediaRecorder(stream, mime ? { mimeType: mime, videoBitsPerSecond: 8e6 } : undefined), chunks = [];
    rec.ondataavailable = (ev) => { if (ev.data.size) chunks.push(ev.data); };
    rec.onstop = () => { this.turntable = null; onDone && onDone(new Blob(chunks, { type: mime || 'video/webm' })); };
    this.turntable = { dur: seconds }; rec.start();
    setTimeout(() => rec.stop(), seconds * 1000 + 50);
  }
};
AT.drawStitch = function (g, ol, st, X, Y, ppm, preview) {
  const segs = st.seg === '*' ? ol.segs.map((s) => s.id) : [st.seg];
  const rows = st.rows || 1;
  const sign = AT.area(ol.poly) > 0 ? 1 : -1;
  g.save(); g.strokeStyle = st.color || '#333'; g.lineCap = 'round';
  g.lineWidth = Math.max(preview ? 1 : 1.2, (st.w || 0.8) * ppm);
  g.setLineDash([(st.len || 3) * ppm, (st.gap || 1.5) * ppm]);
  segs.forEach((sid) => {
    const k = ol.byId[sid]; if (k == null) return;
    const sg = ol.segs[k];
    for (let rI = 0; rI < rows; rI++) {
      const off = (st.off || 3) + rI * (st.rowGap || 6);
      g.beginPath();
      for (let i = 0; i < sg.poly.length; i++) {
        const a = sg.poly[Math.max(0, i - 1)], b = sg.poly[Math.min(sg.poly.length - 1, i + 1)];
        let tx = b.x - a.x, ty = b.y - a.y; const L = Math.hypot(tx, ty) || 1; tx /= L; ty /= L;
        const nx = -ty * sign, ny = tx * sign;
        const px = sg.poly[i].x + nx * off, py = sg.poly[i].y + ny * off;
        if (i === 0) g.moveTo(X(px), Y(py)); else g.lineTo(X(px), Y(py));
      }
      g.stroke();
    }
  });
  g.restore();
};


/* ============================================================
   2D PATTERN WINDOW
   ============================================================ */
AT.TOOLS2D = {
  transform: { key: 'a', label: 'Transform Pattern', icon: 'transform' },
  edit: { key: 'z', label: 'Edit Pattern', icon: 'edit' },
  curvature: { key: 'c', label: 'Edit Curvature', icon: 'curvature' },
  curvept: { key: 'v', label: 'Edit Curve Point', icon: 'curvept' },
  addpoint: { key: 'x', label: 'Add Point / Split Line', icon: 'addpoint' },
  polygon: { key: 'h', label: 'Polygon', icon: 'polygon' },
  rect: { key: 's', label: 'Rectangle', icon: 'rect' },
  ellipse: { key: 'e', label: 'Ellipse', icon: 'ellipse' },
  internal: { key: 'g', label: 'Internal Polygon / Line', icon: 'internal' },
  intellipse: { key: 'r', label: 'Internal Ellipse', icon: 'ellipse' },
  notch: { key: '', label: 'Notch', icon: 'notch' },
  segsew: { key: 'n', label: 'Segment Sewing (Shift = M:N)', icon: 'segsew' },
  freesew: { key: 'm', label: 'Free Sewing', icon: 'freesew' },
  editsew: { key: 'b', label: 'Edit Sewing', icon: 'editsew' },
  stitch: { key: 'k', label: 'Segment Topstitch (Shift = whole outline)', icon: 'stitch' },
  editstitch: { key: 'j', label: 'Edit Topstitch', icon: 'stitch' },
  button: { key: '', label: 'Button', icon: 'button' },
  hole: { key: '', label: 'Buttonhole', icon: 'hole' },
  fasten: { key: '', label: 'Fasten Button', icon: 'fasten' },
  graphic: { key: 't', label: 'Graphic (place / edit)', icon: 'graphic' },
  comment: { key: '', label: 'Comment', icon: 'comment' },
  measure: { key: '', label: 'Measure', icon: 'measure' }
};
AT.SIZE_NEST_COLORS = ['#3B82F6', '#10B981', '#111111', '#F59E0B', '#EF4444', '#8B5CF6', '#EC4899', '#14B8A6'];
AT.Editor2D = class {
  constructor(host, app) {
    this.host = host; this.app = app;
    const c = document.createElement('canvas');
    c.style.cssText = 'position:absolute;inset:0;width:100%;height:100%;display:block;touch-action:none;outline:none';
    host.appendChild(c); this.canvas = c; this.g = c.getContext('2d');
    this.view = { ox: 0, oy: 0, s: 0.5 };
    this.cache = {}; this.tileCache = {}; this.imgCache = {};
    this.drag = null; this.draft = null; this.hover = null; this.sewDraft = null; this.measureLine = null;
    this.dirty = true; this.alive = true;
    this._bind();
    this.ro = new ResizeObserver(() => { this.resize(); }); this.ro.observe(host); this.resize();
    const loop = () => { if (!this.alive) return; if (this.dirty) { this.dirty = false; try { this.render(); } catch (e) { AT.reportError('2D render', e); } } this.raf = requestAnimationFrame(loop); };
    this.raf = requestAnimationFrame(loop);
    this._fitted = false;
  }
  dispose() { this.alive = false; cancelAnimationFrame(this.raf); this.ro.disconnect(); this._unbind(); this.canvas.remove(); }
  resize() { const d = window.devicePixelRatio || 1, w = this.host.clientWidth, h = this.host.clientHeight; this.canvas.width = Math.max(1, w * d); this.canvas.height = Math.max(1, h * d); this.w = w; this.h = h; this.dpr = d; this.dirty = true; if (!this._fitted && w > 50) { this.fit(); } }
  invalidate() { this.dirty = true; }
  W2S(x, y) { return [x * this.view.s + this.view.ox, y * this.view.s + this.view.oy]; }
  S2W(x, y) { return [(x - this.view.ox) / this.view.s, (y - this.view.oy) / this.view.s]; }
  doc() { return this.app.doc(); }
  pieces() { const d = this.doc(); return d ? AT.asArray(d.pieces).sort((a, b) => (a.order || 0) - (b.order || 0)) : []; }
  pts(pc) { const d = this.doc(); return AT.gradedPts(pc, d.activeSize, d.baseSize); }
  ol(pc) {
    const pts = this.pts(pc), key = AT.stable(pts);
    const c = this.cache[pc.id];
    if (c && c.key === key) return c.ol;
    const ol = AT.outline(pts); this.cache[pc.id] = { key: key, ol: ol }; return ol;
  }
  fit(ids) {
    const d = this.doc(); if (!d || !this.w) return;
    let all = [];
    this.pieces().forEach((pc) => { if (!ids || ids.indexOf(pc.id) >= 0) all = all.concat(this.ol(pc).poly); });
    if (!all.length) { this.view = { ox: this.w / 2, oy: this.h / 2, s: 0.5 }; this.dirty = true; return; }
    const bb = AT.bbox(all), pad = 40;
    const s = Math.min((this.w - pad * 2) / Math.max(bb.w, 10), (this.h - pad * 2) / Math.max(bb.h, 10));
    this.view.s = AT.clamp(s, 0.02, 20); this.view.ox = this.w / 2 - bb.cx * this.view.s; this.view.oy = this.h / 2 - bb.cy * this.view.s;
    this._fitted = true; this.dirty = true;
  }
  // ---------- hit testing ----------
  hit(sx, sy, opts) {
    opts = opts || {};
    const d = this.doc(); if (!d) return {};
    const [x, y] = this.S2W(sx, sy), tol = 7 / this.view.s, res = { x: x, y: y };
    const pcs = this.pieces().slice().reverse();
    // annotations first
    if (!opts.outlineOnly) {
      for (const c of Object.values(d.comments || {})) if (c && Math.hypot(c.x - x, c.y - y) < 10 / this.view.s + 6) { res.comment = c; return res; }
      for (const b of Object.values(d.buttons || {})) if (b && Math.hypot(b.x - x, b.y - y) < (b.size || 15) / 2 + tol * 0.5) { res.button = b; break; }
      for (const h of Object.values(d.holes || {})) if (h && Math.hypot(h.x - x, h.y - y) < (h.len || 18) / 2 + tol * 0.5) { res.hole = h; break; }
    }
    for (const pc of pcs) {
      const ol = this.ol(pc), pts = this.pts(pc);
      // points
      let bp = null, bd = tol;
      pts.forEach((p, i) => { const dd = Math.hypot(p.x - x, p.y - y); if (dd < bd) { bd = dd; bp = { pc: pc, p: p, i: i }; } });
      if (bp && !res.point) res.point = bp;
      const n = AT.nearestOnOutline(ol, x, y);
      if (n.d < tol && !res.seg) res.seg = { pc: pc, k: n.k, t: n.t, id: n.id, x: n.x, y: n.y, d: n.d };
      (pc.internals || []).forEach((il) => {
        const pl = AT.evalOpen(il.closed ? il.pts.concat([il.pts[0]]) : il.pts);
        for (let i = 0; i < pl.length - 1; i++) { if (AT.distSeg(x, y, pl[i].x, pl[i].y, pl[i + 1].x, pl[i + 1].y).d < tol && !res.internal) res.internal = { pc: pc, il: il }; }
      });
      if (!res.piece && AT.inside(ol.poly, x, y)) res.piece = pc;
      (pc.graphics || []).forEach((gr) => { if (!res.graphic && Math.abs(x - gr.x) < gr.w / 2 && Math.abs(y - gr.y) < gr.h / 2) res.graphic = { pc: pc, g: gr }; });
      if (res.point && res.seg && res.piece) break;
    }
    // sewing lines
    if (!opts.outlineOnly) {
      for (const sw of Object.values(d.sewing || {})) {
        let hitS = false;
        sw.a.concat(sw.b).forEach((sd) => {
          if (hitS) return; const pc = d.pieces[sd.p]; if (!pc) return;
          const pl = AT.sidePolyline(this.ol(pc), sd, 6 / this.view.s + 2);
          for (let i = 0; i < pl.length - 1; i++) if (AT.distSeg(x, y, pl[i].x, pl[i].y, pl[i + 1].x, pl[i + 1].y).d < tol) { hitS = true; break; }
        });
        if (hitS) { res.sewing = sw; break; }
      }
      for (const st of Object.values(d.stitches || {})) {
        const pc = d.pieces[st.p]; if (!pc) continue;
        const ol = this.ol(pc), segs = st.seg === '*' ? ol.segs : [ol.segs[ol.byId[st.seg]]].filter(Boolean);
        for (const sg of segs) { for (let i = 0; i < sg.poly.length - 1; i += 2) { const dd = AT.distSeg(x, y, sg.poly[i].x, sg.poly[i].y, sg.poly[i + 1].x, sg.poly[i + 1].y).d; if (Math.abs(dd - (st.off || 3)) < tol) { res.stitch = st; break; } } if (res.stitch) break; }
        if (res.stitch) break;
      }
    }
    return res;
  }
  // ---------- events ----------
  _bind() {
    const c = this.canvas;
    this._h = { down: (e) => this.onDown(e), move: (e) => this.onMove(e), up: (e) => this.onUp(e), wheel: (e) => this.onWheel(e), dbl: (e) => this.onDbl(e), ctx: (e) => e.preventDefault(), leave: () => { this.hover = null; this.dirty = true; } };
    c.addEventListener('pointerdown', this._h.down); window.addEventListener('pointermove', this._h.move); window.addEventListener('pointerup', this._h.up);
    c.addEventListener('wheel', this._h.wheel, { passive: false }); c.addEventListener('dblclick', this._h.dbl); c.addEventListener('contextmenu', this._h.ctx); c.addEventListener('pointerleave', this._h.leave);
  }
  _unbind() {
    const c = this.canvas;
    c.removeEventListener('pointerdown', this._h.down); window.removeEventListener('pointermove', this._h.move); window.removeEventListener('pointerup', this._h.up);
    c.removeEventListener('wheel', this._h.wheel); c.removeEventListener('dblclick', this._h.dbl); c.removeEventListener('contextmenu', this._h.ctx); c.removeEventListener('pointerleave', this._h.leave);
  }
  local(e) { const r = this.canvas.getBoundingClientRect(); return [e.clientX - r.left, e.clientY - r.top]; }
  onWheel(e) {
    e.preventDefault(); e.stopPropagation();
    const [sx, sy] = this.local(e);
    const mouseWheel = e.deltaMode === 1 || (Math.abs(e.deltaY) >= 40 && Math.abs(e.deltaX) < 1 && Number.isInteger(e.deltaY));
    if (e.ctrlKey || e.metaKey || mouseWheel) {
      const f = Math.exp(-(e.ctrlKey ? e.deltaY * 2.2 : e.deltaY) * 0.0016), s0 = this.view.s, s1 = AT.clamp(s0 * f, 0.02, 40);
      this.view.ox = sx - (sx - this.view.ox) * (s1 / s0); this.view.oy = sy - (sy - this.view.oy) * (s1 / s0); this.view.s = s1;
    } else { this.view.ox -= e.deltaX; this.view.oy -= e.deltaY; }
    this.dirty = true;
  }
  snap(x, y, e, excludeId) {
    const d = this.doc(), app = this.app;
    if (e && e.altKey) return [x, y];
    const tol = 8 / this.view.s;
    let best = null, bd = tol;
    this.pieces().forEach((pc) => this.pts(pc).forEach((p) => { if (p.id === excludeId || p.c) return; const dd = Math.hypot(p.x - x, p.y - y); if (dd < bd) { bd = dd; best = [p.x, p.y]; } }));
    if (best) { this.snapMark = best; return best; }
    this.snapMark = null;
    const gs = app.gridSnap ? app.gridSnap() : 0;
    if (gs) return [Math.round(x / gs) * gs, Math.round(y / gs) * gs];
    return [x, y];
  }
  onDown(e) {
    e.stopPropagation();
    this.app.focus && this.app.focus();
    try { this.canvas.setPointerCapture && this.canvas.setPointerCapture(e.pointerId); } catch (err) {}
    const [sx, sy] = this.local(e), [wx, wy] = this.S2W(sx, sy);
    const tool = this.app.tool(), d = this.doc(); if (!d) return;
    if (e.button === 1 || e.button === 2) { this.drag = { type: 'pan', sx: sx, sy: sy, ox: this.view.ox, oy: this.view.oy, btn: e.button, moved: false, e0: e }; return; }
    const H = this.hit(sx, sy);
    const A = this.app;
    this.down = { sx: sx, sy: sy, wx: wx, wy: wy };
    switch (tool) {
      case 'transform': {
        const rh = this.rotHandle();
        if (rh && Math.hypot(sx - rh[0], sy - rh[1]) < 10) { this.drag = { type: 'rotate', c: rh[2], a0: Math.atan2(wy - rh[2][1], wx - rh[2][0]), last: 0, moved: true }; return; }
        if (H.piece) {
          let ids = A.selPieces();
          if (e.shiftKey) { ids = ids.indexOf(H.piece.id) >= 0 ? ids.filter((i) => i !== H.piece.id) : ids.concat([H.piece.id]); A.setSel({ kind: 'pieces', ids: ids }); }
          else if (ids.indexOf(H.piece.id) < 0) { ids = [H.piece.id]; A.setSel({ kind: 'pieces', ids: ids }); }
          this.drag = { type: 'movePieces', ids: ids.filter((i) => !(d.pieces[i] && d.pieces[i].mirror)), last: [wx, wy], moved: false };
        } else { if (!e.shiftKey) A.setSel(null); this.drag = { type: 'box', x0: wx, y0: wy, x1: wx, y1: wy, mode: 'pieces' }; }
        return;
      }
      case 'edit': {
        if (H.point) {
          const pc = H.point.pc, pid = H.point.p.id;
          let sel = A.sel();
          let ptIds = sel && sel.kind === 'points' && sel.piece === pc.id ? sel.ids.slice() : [];
          if (e.shiftKey) ptIds = ptIds.indexOf(pid) >= 0 ? ptIds.filter((i) => i !== pid) : ptIds.concat([pid]);
          else if (ptIds.indexOf(pid) < 0) ptIds = [pid];
          A.setSel({ kind: 'points', piece: pc.id, ids: ptIds });
          this.drag = { type: 'movePoints', pc: pc.id, ids: ptIds, last: [wx, wy], start: [wx, wy], orig: AT.clone(this.pts(pc)), moved: false, anchor: H.point.p };
        } else if (H.seg) {
          A.setSel({ kind: 'segment', piece: H.seg.pc.id, seg: H.seg.id });
          const ol = this.ol(H.seg.pc), sg = ol.segs[H.seg.k], n = H.seg.pc.pts.length, ids = [];
          let j = sg.i0; ids.push(H.seg.pc.pts[j].id); j = (j + 1) % n; while (j !== sg.i1) { ids.push(H.seg.pc.pts[j].id); j = (j + 1) % n; } ids.push(H.seg.pc.pts[sg.i1].id);
          this.drag = { type: 'movePoints', pc: H.seg.pc.id, ids: ids, last: [wx, wy], start: [wx, wy], orig: AT.clone(this.pts(H.seg.pc)), moved: false };
        } else if (H.internal) { A.setSel({ kind: 'internal', piece: H.internal.pc.id, id: H.internal.il.id }); this.drag = { type: 'moveInternal', pc: H.internal.pc.id, id: H.internal.il.id, last: [wx, wy] }; }
        else if (H.piece) { A.setSel({ kind: 'pieces', ids: [H.piece.id] }); this.drag = { type: 'movePieces', ids: H.piece.mirror ? [] : [H.piece.id], last: [wx, wy] }; }
        else { A.setSel(null); this.drag = { type: 'box', x0: wx, y0: wy, x1: wx, y1: wy, mode: 'points' }; }
        return;
      }
      case 'curvature': if (H.seg) { this.drag = { type: 'curvature', pc: H.seg.pc.id, seg: H.seg.id, orig: AT.clone(H.seg.pc.pts) }; A.setSel({ kind: 'segment', piece: H.seg.pc.id, seg: H.seg.id }); } return;
      case 'curvept': {
        if (H.point && H.point.p.c) { if (e.altKey) { this.mutate(H.point.pc.id, (pc) => { const p = pc.pts.find((q) => q.id === H.point.p.id); if (p) delete p.c; }, 'Convert to corner'); return; } this.drag = { type: 'movePoints', pc: H.point.pc.id, ids: [H.point.p.id], last: [wx, wy], start: [wx, wy], orig: AT.clone(this.pts(H.point.pc)) }; A.setSel({ kind: 'points', piece: H.point.pc.id, ids: [H.point.p.id] }); return; }
        if (H.seg) { const pc = H.seg.pc; if (pc.mirror) return A.toast('Edit the source piece — this mirror is linked'); const old = AT.clone(pc.pts); const id = AT.addCurvePoint(pc, H.seg.id, H.seg.t, H.seg.x, H.seg.y); AT.remapRefs(d, pc.id, old); A.setSel({ kind: 'points', piece: pc.id, ids: [id] }); this.drag = { type: 'movePoints', pc: pc.id, ids: [id], last: [wx, wy], start: [wx, wy], orig: AT.clone(pc.pts), moved: true }; A.live([pc.id]); }
        return;
      }
      case 'addpoint': if (H.seg) { const pc = H.seg.pc; if (pc.mirror) return A.toast('Edit the source piece — this mirror is linked'); const old = AT.clone(pc.pts); const id = AT.splitSegment(pc, H.seg.id, H.seg.t); AT.remapRefs(d, pc.id, old); A.setSel({ kind: 'points', piece: pc.id, ids: [id] }); A.commit('Add point'); } return;
      case 'polygon': case 'internal': {
        const p = this.snap(wx, wy, e);
        if (!this.draft) {
          if (tool === 'internal') { const pc = H.piece || (H.seg && H.seg.pc); if (!pc) return A.toast('Click inside a pattern to start an internal line'); this.draft = { tool: tool, pc: pc.id, pts: [] }; }
          else this.draft = { tool: tool, pts: [] };
        }
        const dr = this.draft;
        if (dr.pts.length >= 2) { const f = dr.pts[0]; if (Math.hypot(this.W2S(f.x, f.y)[0] - sx, this.W2S(f.x, f.y)[1] - sy) < 9) { this.finishDraft(true); return; } }
        let q = { x: p[0], y: p[1] };
        if (e.shiftKey && dr.pts.length) { const l = dr.pts[dr.pts.length - 1], dx = q.x - l.x, dy = q.y - l.y, a = Math.round(Math.atan2(dy, dx) / (Math.PI / 4)) * (Math.PI / 4), L = Math.hypot(dx, dy); q = { x: l.x + Math.cos(a) * L, y: l.y + Math.sin(a) * L }; }
        if (e.altKey && dr.pts.length) q.c = 1;
        dr.pts.push(q); this.dirty = true; return;
      }
      case 'rect': case 'ellipse': case 'intellipse': {
        const p = this.snap(wx, wy, e);
        if (tool === 'intellipse') { const pc = H.piece; if (!pc) return A.toast('Drag inside a pattern'); this.drag = { type: 'shape', tool: tool, pc: pc.id, x0: p[0], y0: p[1], x1: p[0], y1: p[1] }; }
        else this.drag = { type: 'shape', tool: tool, x0: p[0], y0: p[1], x1: p[0], y1: p[1] };
        return;
      }
      case 'notch': if (H.seg) { this.mutate(H.seg.pc.id, (pc) => { pc.notches = (pc.notches || []).concat([{ id: AT.uid('n'), s: H.seg.id, t: H.seg.t }]); }, 'Add notch'); } return;
      case 'segsew': return this.sewClick(H, e, false);
      case 'freesew': return this.sewClick(H, e, true);
      case 'editsew': if (H.sewing) A.setSel({ kind: 'sewing', id: H.sewing.id }); else A.setSel(null); return;
      case 'stitch': if (H.seg) { const id = AT.uid('st'); d.stitches[id] = { id: id, p: H.seg.pc.id, seg: e.shiftKey ? '*' : H.seg.id, off: 4, len: 3, gap: 1.6, w: 0.9, color: '#3b3a36', rows: 1, rowGap: 6 }; A.setSel({ kind: 'stitch', id: id }); A.commit('Add topstitch'); } return;
      case 'editstitch': if (H.stitch) A.setSel({ kind: 'stitch', id: H.stitch.id }); else A.setSel(null); return;
      case 'button': {
        if (H.button) { A.setSel({ kind: 'button', id: H.button.id }); this.drag = { type: 'moveObj', coll: 'buttons', id: H.button.id, last: [wx, wy] }; return; }
        const pc = H.piece; if (!pc) return A.toast('Click inside a pattern to place a button');
        const id = AT.uid('bt'); d.buttons[id] = { id: id, p: pc.id, x: wx, y: wy, size: 15, color: '#2b2b2b', shape: 'round', holes: 4, finish: 'plastic' }; A.setSel({ kind: 'button', id: id }); A.commit('Add button'); return;
      }
      case 'hole': {
        if (H.hole) { A.setSel({ kind: 'hole', id: H.hole.id }); this.drag = { type: 'moveObj', coll: 'holes', id: H.hole.id, last: [wx, wy] }; return; }
        const pc = H.piece; if (!pc) return A.toast('Click inside a pattern to place a buttonhole');
        const id = AT.uid('bh'); d.holes[id] = { id: id, p: pc.id, x: wx, y: wy, len: 18, ang: 0, color: '#2a2a2a' }; A.setSel({ kind: 'hole', id: id }); A.commit('Add buttonhole'); return;
      }
      case 'fasten': {
        if (H.button) { this.fastenFrom = H.button.id; A.toast('Now click the buttonhole to fasten to'); this.dirty = true; return; }
        if (H.hole && this.fastenFrom) { d.buttons[this.fastenFrom].fasten = H.hole.id; this.fastenFrom = null; A.commit('Fasten button'); A.toast('Button fastened — simulate to close'); }
        return;
      }
      case 'graphic': {
        if (H.graphic) {
          A.setSel({ kind: 'graphic', piece: H.graphic.pc.id, id: H.graphic.g.id });
          const gr = H.graphic.g, corner = Math.abs(wx - gr.x) > gr.w / 2 - 8 / this.view.s && Math.abs(wy - gr.y) > gr.h / 2 - 8 / this.view.s;
          this.drag = corner ? { type: 'scaleGraphic', pc: H.graphic.pc.id, id: gr.id, ratio: gr.w / gr.h } : { type: 'moveGraphic', pc: H.graphic.pc.id, id: gr.id, last: [wx, wy] };
          return;
        }
        const pend = A.pendingGraphic && A.pendingGraphic();
        if (pend && H.piece) { const pc = H.piece, id = AT.uid('g'), w = 120; pc.graphics = (pc.graphics || []).concat([{ id: id, path: pend.path, name: pend.name, x: wx, y: wy, w: w, h: w / (pend.ratio || 1), rot: 0, opacity: 1 }]); A.setPendingGraphic(null); A.setSel({ kind: 'graphic', piece: pc.id, id: id }); A.commit('Place graphic'); return; }
        if (!pend) A.toast('Choose an image in the Graphic tab, then click a pattern');
        return;
      }
      case 'comment': { const pc = H.piece; if (!pc) return A.toast('Click on a pattern to pin a comment'); A.newComment(pc.id, wx, wy); return; }
      case 'measure': { const p = this.snap(wx, wy, e); this.measureLine = { a: p, b: p }; this.drag = { type: 'measure' }; return; }
    }
  }
  sewClick(H, e, free) {
    const A = this.app, d = this.doc();
    if (!H.seg) return;
    const sd = this.sewDraft || (this.sewDraft = { phase: 'a', a: [], b: [], pending: null });
    let side = null;
    if (!free) {
      const rev = H.seg.t > 0.5;
      side = Object.assign({ p: H.seg.pc.id }, AT.segSide(H.seg.id, rev));
    } else {
      if (!sd.pending) { sd.pending = { pc: H.seg.pc.id, s: H.seg.id, t: H.seg.t, x: H.seg.x, y: H.seg.y }; this.dirty = true; return; }
      if (sd.pending.pc !== H.seg.pc.id) { A.toast('Free sewing: both ends must be on the same pattern'); return; }
      const ol = this.ol(d.pieces[sd.pending.pc]);
      const Pa = AT.perimParam(ol, ol.byId[sd.pending.s], sd.pending.t), Pb = AT.perimParam(ol, H.seg.k, H.seg.t), T = ol.total;
      const fwd = ((Pb - Pa) % T + T) % T;
      let dir = fwd <= T / 2 ? 1 : -1; if (e.altKey) dir = -dir;
      side = { p: sd.pending.pc, a: { s: sd.pending.s, t: sd.pending.t }, b: { s: H.seg.id, t: H.seg.t }, dir: dir };
      sd.pending = null;
    }
    sd[sd.phase].push(side);
    if (e.shiftKey) { A.toast('M:N — keep Shift-clicking, press Enter to finish this side'); this.dirty = true; return; }
    this.advanceSew();
  }
  advanceSew() {
    const sd = this.sewDraft; if (!sd) return;
    if (sd.phase === 'a' && sd.a.length) { sd.phase = 'b'; this.dirty = true; return; }
    if (sd.phase === 'b' && sd.b.length) {
      const d = this.doc();
      const id = AT.addSew(d, sd.a, sd.b);
      this.sewDraft = null; this.app.setSel({ kind: 'sewing', id: id }); this.app.commit('Sew');
      const L = AT.sewLengths(d)[id]; if (L && Math.abs(L.a - L.b) > 3) this.app.toast('Sewing length differs by ' + AT.fmt(Math.abs(L.a - L.b), this.app.units()) + ' ' + AT.unitLabel(this.app.units()));
    }
    this.dirty = true;
  }
  onDbl(e) {
    const tool = this.app.tool();
    if ((tool === 'polygon' || tool === 'internal') && this.draft) { this.draft.pts.pop(); this.finishDraft(false); return; }
    if (tool === 'edit') { const [sx, sy] = this.local(e); const H = this.hit(sx, sy, { outlineOnly: true }); if (H.seg && !H.point) { const pc = H.seg.pc, old = AT.clone(pc.pts); AT.splitSegment(pc, H.seg.id, H.seg.t); AT.remapRefs(this.doc(), pc.id, old); this.app.commit('Add point'); } }
  }
  finishDraft(closed) {
    const dr = this.draft, A = this.app, d = this.doc(); this.draft = null; this.dirty = true;
    if (!dr) return;
    if (dr.tool === 'polygon') {
      if (dr.pts.length < 3) return;
      if (dr.pts[0].c) delete dr.pts[0].c;
      const pc = AT.newPiece('Pattern ' + (Object.keys(d.pieces).length + 1), dr.pts.map((p) => ({ id: AT.uid('p'), x: p.x, y: p.y, c: p.c })), { fabric: Object.keys(d.fabrics)[0] });
      AT.addPiece(d, pc); A.setSel({ kind: 'pieces', ids: [pc.id] }); A.commit('Create pattern');
    } else {
      const pc = d.pieces[dr.pc]; if (!pc || dr.pts.length < 2) return;
      pc.internals = (pc.internals || []).concat([{ id: AT.uid('il'), pts: (closed ? dr.pts : dr.pts).map((p) => ({ x: p.x, y: p.y, c: p.c })), closed: !!closed, fold: null }]);
      A.setSel({ kind: 'internal', piece: pc.id, id: pc.internals[pc.internals.length - 1].id }); A.commit('Internal line');
    }
  }
  cancel() { this.draft = null; this.sewDraft = null; this.fastenFrom = null; this.measureLine = null; this.dirty = true; }
  snapPieces(ids) { const d = this.doc(), o = {}; ids.forEach((i) => { if (d.pieces[i]) o[i] = AT.clone(d); }); return o; }
  mutate(pid, fn, label) {
    const d = this.doc(), pc = d.pieces[pid]; if (!pc) return;
    const old = AT.clone(pc.pts); fn(pc); AT.remapRefs(d, pid, old); this.app.commit(label || 'Edit');
  }
  onMove(e) {
    const [sx, sy] = this.local(e), [wx, wy] = this.S2W(sx, sy);
    this.cursor = [wx, wy];
    const dr = this.drag, A = this.app, d = this.doc(); if (!d) return;
    if (!dr) {
      if (e.target !== this.canvas && !this.draft) return;
      const H = this.hit(sx, sy);
      this.hover = H; A.status && A.status({ x: wx, y: wy });
      const tool = A.tool();
      this.canvas.style.cursor = (tool === 'transform' && H.piece) ? 'move' : (['segsew', 'freesew', 'addpoint', 'curvept', 'curvature', 'notch', 'stitch'].indexOf(tool) >= 0 && H.seg) ? 'crosshair' : (tool === 'edit' && (H.point || H.seg)) ? 'pointer' : ['polygon', 'rect', 'ellipse', 'internal', 'intellipse', 'measure'].indexOf(tool) >= 0 ? 'crosshair' : 'default';
      this.dirty = true; return;
    }
    if (dr.type === 'pan') { if (Math.abs(sx - dr.sx) + Math.abs(sy - dr.sy) > 3) dr.moved = true; this.view.ox = dr.ox + sx - dr.sx; this.view.oy = dr.oy + sy - dr.sy; this.dirty = true; return; }
    if (dr.type === 'movePieces') {
      let dx = wx - dr.last[0], dy = wy - dr.last[1]; dr.last = [wx, wy]; dr.moved = true;
      dr.ids.forEach((i) => AT.transformPiece(d, i, (x, y) => [x + dx, y + dy]));
      if (dr.ids.length) { AT.syncMirrors(d); A.live(dr.ids, true); }
      this.dirty = true; return;
    }
    if (dr.type === 'rotate') {
      let a = Math.atan2(wy - dr.c[1], wx - dr.c[0]) - dr.a0; if (e.shiftKey) a = Math.round(a / (Math.PI / 12)) * (Math.PI / 12);
      const da = a - dr.last; dr.last = a; const c = Math.cos(da), s = Math.sin(da);
      A.selPieces().forEach((i) => { if (d.pieces[i] && !d.pieces[i].mirror) AT.transformPiece(d, i, (x, y) => [dr.c[0] + (x - dr.c[0]) * c - (y - dr.c[1]) * s, dr.c[1] + (x - dr.c[0]) * s + (y - dr.c[1]) * c]); });
      AT.syncMirrors(d); A.live(A.selPieces(), true); this.dirty = true; return;
    }
    if (dr.type === 'box') { dr.x1 = wx; dr.y1 = wy; this.dirty = true; return; }
    if (dr.type === 'movePoints') {
      let tx = wx, ty = wy;
      if (dr.anchor && dr.ids.length === 1) { const s2 = this.snap(wx, wy, e, dr.anchor.id); tx = s2[0]; ty = s2[1]; }
      const tdx = tx - dr.start[0], tdy = ty - dr.start[1];
      dr.moved = true;
      const pc = d.pieces[dr.pc]; if (!pc) return;
      const grading = d.activeSize && d.activeSize !== d.baseSize;
      const target = pc.mirror ? d.pieces[pc.mirror.src] : pc;
      const sgn = pc.mirror ? -1 : 1;
      dr.ids.forEach((id) => {
        const sid = pc.mirror ? id.replace(/^m/, '') : id;
        const o = dr.orig.find((q) => q.id === id); if (!o) return;
        if (grading) {
          target.grade = target.grade || {}; const base = target.pts.find((q) => q.id === sid); if (!base) return;
          const r = target.grade[sid] = target.grade[sid] || {};
          const prev = (dr.gradeOrig || (dr.gradeOrig = AT.clone(target.grade)))[sid];
          const p0 = prev && prev[d.activeSize] ? prev[d.activeSize] : { dx: 0, dy: 0 };
          r[d.activeSize] = { dx: Math.round((p0.dx + tdx * sgn) * 10) / 10, dy: Math.round((p0.dy + tdy) * 10) / 10 };
        } else {
          const p = target.pts.find((q) => q.id === sid); if (!p) return;
          const oo = pc.mirror ? { x: 2 * pc.mirror.axis - o.x, y: o.y } : o;
          p.x = oo.x + tdx * sgn; p.y = oo.y + tdy;
        }
      });
      AT.syncMirrors(d); A.live([dr.pc, target.id]); this.dirty = true; return;
    }
    if (dr.type === 'moveInternal') {
      const dx = wx - dr.last[0], dy = wy - dr.last[1]; dr.last = [wx, wy]; dr.moved = true;
      const pc = d.pieces[dr.pc], il = pc && pc.internals.find((q) => q.id === dr.id); if (il) il.pts.forEach((p) => { p.x += dx; p.y += dy; });
      A.live([dr.pc]); this.dirty = true; return;
    }
    if (dr.type === 'curvature') { const pc = d.pieces[dr.pc]; if (!pc || pc.mirror) return; pc.pts = AT.clone(dr.orig); AT.setCurvature(pc, dr.seg, wx, wy); dr.moved = true; AT.syncMirrors(d); A.live([dr.pc]); this.dirty = true; return; }
    if (dr.type === 'shape') { const p = this.snap(wx, wy, e); dr.x1 = p[0]; dr.y1 = p[1]; if (e.shiftKey) { const s = Math.max(Math.abs(dr.x1 - dr.x0), Math.abs(dr.y1 - dr.y0)); dr.x1 = dr.x0 + Math.sign(dr.x1 - dr.x0 || 1) * s; dr.y1 = dr.y0 + Math.sign(dr.y1 - dr.y0 || 1) * s; } this.dirty = true; return; }
    if (dr.type === 'moveObj') { const o = d[dr.coll][dr.id]; if (o) { o.x += wx - dr.last[0]; o.y += wy - dr.last[1]; } dr.last = [wx, wy]; dr.moved = true; this.dirty = true; return; }
    if (dr.type === 'moveGraphic') { const pc = d.pieces[dr.pc], gr = pc && pc.graphics.find((q) => q.id === dr.id); if (gr) { gr.x += wx - dr.last[0]; gr.y += wy - dr.last[1]; } dr.last = [wx, wy]; dr.moved = true; A.live([dr.pc]); this.dirty = true; return; }
    if (dr.type === 'scaleGraphic') { const pc = d.pieces[dr.pc], gr = pc && pc.graphics.find((q) => q.id === dr.id); if (gr) { const w = Math.max(5, Math.abs(wx - gr.x) * 2); gr.w = w; gr.h = w / dr.ratio; } dr.moved = true; A.live([dr.pc]); this.dirty = true; return; }
    if (dr.type === 'measure') { const p = this.snap(wx, wy, e); this.measureLine.b = p; this.dirty = true; return; }
  }
  onUp(e) {
    const dr = this.drag; this.drag = null; const A = this.app, d = this.doc();
    if (!dr || !d) return;
    if (dr.type === 'pan') { if (!dr.moved && dr.btn === 2) { const [sx, sy] = this.local(e); A.onContext2D && A.onContext2D(e, this.hit(sx, sy)); } return; }
    if (dr.type === 'box') {
      const x0 = Math.min(dr.x0, dr.x1), x1 = Math.max(dr.x0, dr.x1), y0 = Math.min(dr.y0, dr.y1), y1 = Math.max(dr.y0, dr.y1);
      if (x1 - x0 < 2 / this.view.s && y1 - y0 < 2 / this.view.s) { this.dirty = true; return; }
      if (dr.mode === 'pieces') {
        const ids = this.pieces().filter((pc) => { const b = AT.bbox(this.ol(pc).poly); return b.x1 >= x0 && b.x0 <= x1 && b.y1 >= y0 && b.y0 <= y1; }).map((pc) => pc.id);
        A.setSel(ids.length ? { kind: 'pieces', ids: ids } : null);
      } else {
        let best = null;
        this.pieces().forEach((pc) => { const ids = this.pts(pc).filter((p) => p.x >= x0 && p.x <= x1 && p.y >= y0 && p.y <= y1).map((p) => p.id); if (ids.length && (!best || ids.length > best.ids.length)) best = { kind: 'points', piece: pc.id, ids: ids }; });
        A.setSel(best);
      }
      this.dirty = true; return;
    }
    if (dr.type === 'shape') {
      const x0 = Math.min(dr.x0, dr.x1), x1 = Math.max(dr.x0, dr.x1), y0 = Math.min(dr.y0, dr.y1), y1 = Math.max(dr.y0, dr.y1);
      if (x1 - x0 < 3 || y1 - y0 < 3) { this.dirty = true; return; }
      let pts;
      const cx = (x0 + x1) / 2, cy = (y0 + y1) / 2, rx = (x1 - x0) / 2, ry = (y1 - y0) / 2, k = 0.5523;
      if (dr.tool === 'rect') pts = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]].map((q) => ({ id: AT.uid('p'), x: q[0], y: q[1] }));
      else pts = [[cx + rx, cy, 0], [cx + rx, cy + ry * k, 1], [cx + rx * k, cy + ry, 1], [cx, cy + ry, 0], [cx - rx * k, cy + ry, 1], [cx - rx, cy + ry * k, 1], [cx - rx, cy, 0], [cx - rx, cy - ry * k, 1], [cx - rx * k, cy - ry, 1], [cx, cy - ry, 0], [cx + rx * k, cy - ry, 1], [cx + rx, cy - ry * k, 1]].map((q) => ({ id: AT.uid('p'), x: q[0], y: q[1], c: q[2] || undefined }));
      if (dr.tool === 'intellipse') {
        const pc = d.pieces[dr.pc]; if (!pc) return;
        pc.internals = (pc.internals || []).concat([{ id: AT.uid('il'), pts: pts.concat([pts[0]]).map((p) => ({ x: p.x, y: p.y, c: p.c })), closed: false, fold: null }]);
        A.commit('Internal ellipse');
      } else {
        pts.forEach((p) => { if (!p.c) delete p.c; });
        const pc = AT.newPiece((dr.tool === 'rect' ? 'Rectangle ' : 'Ellipse ') + (Object.keys(d.pieces).length + 1), pts, { fabric: Object.keys(d.fabrics)[0] });
        AT.addPiece(d, pc); A.setSel({ kind: 'pieces', ids: [pc.id] }); A.commit('Create pattern');
      }
      this.dirty = true; return;
    }
    if (dr.type === 'measure') { this.dirty = true; return; }
    if (dr.moved || dr.type === 'curvature') { A.commit({ movePieces: 'Move pattern', rotate: 'Rotate pattern', movePoints: d.activeSize !== d.baseSize ? 'Edit grading' : 'Move points', moveInternal: 'Move internal line', curvature: 'Edit curvature', moveObj: 'Move', moveGraphic: 'Move graphic', scaleGraphic: 'Scale graphic' }[dr.type] || 'Edit'); }
  }
  rotHandle() {
    const ids = this.app.selPieces(); if (!ids.length || this.app.tool() !== 'transform') return null;
    const d = this.doc(); let all = [];
    ids.forEach((i) => { if (d.pieces[i]) all = all.concat(this.ol(d.pieces[i]).poly); });
    if (!all.length) return null;
    const bb = AT.bbox(all), [sx, sy] = this.W2S(bb.cx, bb.y0);
    return [sx, sy - 26, [bb.cx, bb.cy], bb];
  }
  keyNudge(dx, dy) {
    const A = this.app, d = this.doc(), sel = A.sel(); if (!sel) return false;
    if (sel.kind === 'pieces') { sel.ids.forEach((i) => AT.transformPiece(d, i, (x, y) => [x + dx, y + dy])); AT.syncMirrors(d); A.commit('Nudge'); return true; }
    if (sel.kind === 'points') { const pc = d.pieces[sel.piece]; if (!pc) return false; pc.pts.forEach((p) => { if (sel.ids.indexOf(p.id) >= 0) { p.x += dx; p.y += dy; } }); AT.syncMirrors(d); A.commit('Nudge points'); return true; }
    return false;
  }
  // ---------- rendering ----------
  tilePattern(fab) {
    const key = AT.fabricKey(fab);
    if (!this.tileCache[key]) { const c = AT.fabricTile(fab, 96); this.tileCache[key] = this.g.createPattern(c, 'repeat'); }
    return this.tileCache[key];
  }
  img(url) {
    if (!url) return null;
    let im = this.imgCache[url];
    if (!im) { im = new Image(); im.crossOrigin = 'anonymous'; im.onload = () => { this.dirty = true; }; im.src = url; this.imgCache[url] = im; }
    return im.complete && im.naturalWidth ? im : null;
  }
  render() {
    const g = this.g, d = this.doc(), A = this.app, dpr = this.dpr || 1;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    const pal = A.palette();
    g.fillStyle = pal.canvas2d; g.fillRect(0, 0, this.w, this.h);
    if (!d) return;
    const disp = A.disp2d(), units = A.units(), V = this.view, s = V.s;
    const W2S = (x, y) => [x * s + V.ox, y * s + V.oy];
    // grid
    if (disp.grid) {
      const steps = [1, 2, 5, 10, 20, 50, 100, 200, 500, 1000, 2000]; let step = steps.find((v) => v * s >= 8) || 2000;
      const major = step * 10;
      const [x0, y0] = this.S2W(0, 0), [x1, y1] = this.S2W(this.w, this.h);
      g.lineWidth = 1;
      for (let lvl = 0; lvl < 2; lvl++) {
        const st = lvl ? major : step; if (st * s < 6) continue;
        g.strokeStyle = lvl ? pal.gridMajor : pal.grid; g.beginPath();
        for (let x = Math.floor(x0 / st) * st; x <= x1; x += st) { const X = Math.round(x * s + V.ox) + 0.5; g.moveTo(X, 0); g.lineTo(X, this.h); }
        for (let y = Math.floor(y0 / st) * st; y <= y1; y += st) { const Y = Math.round(y * s + V.oy) + 0.5; g.moveTo(0, Y); g.lineTo(this.w, Y); }
        g.stroke();
      }
    }
    const sel = A.sel() || {}, selPieces = sel.kind === 'pieces' ? sel.ids : [];
    const pcs = this.pieces();
    const path = (pts, close) => { g.beginPath(); pts.forEach((p, i) => { const q = W2S(p.x, p.y); if (i) g.lineTo(q[0], q[1]); else g.moveTo(q[0], q[1]); }); if (close) g.closePath(); };
    // grading nest
    if (disp.grading && d.sizes && d.sizes.length > 1) {
      pcs.forEach((pc) => {
        d.sizes.forEach((sz, i) => {
          if (sz === d.activeSize) return;
          const ol = AT.outline(AT.gradedPts(pc, sz, d.baseSize));
          g.strokeStyle = AT.SIZE_NEST_COLORS[i % AT.SIZE_NEST_COLORS.length]; g.globalAlpha = 0.75; g.lineWidth = 1; path(ol.poly, true); g.stroke(); g.globalAlpha = 1;
        });
      });
    }
    pcs.forEach((pc) => {
      const ol = this.ol(pc), fab = AT.effFabric(d, pc.fabric), selected = selPieces.indexOf(pc.id) >= 0;
      const st = pc.state || 'active';
      // seam allowance
      if (disp.sa && (pc.sa || pc.saSeg || pc.hem)) {
        const off = AT.cutOutline(pc, ol);
        g.setLineDash([5, 4]); g.strokeStyle = pal.sa; g.lineWidth = 1; path(off, true); g.stroke(); g.setLineDash([]);
        if (pc.hem) { const sgnH = AT.area(ol.poly) > 0 ? 1 : -1; ol.segs.forEach((sg) => { const h = pc.hem[sg.id]; if (!(h > 0) || sg.len * s < 50) return; const m = AT.segPoint(sg, 0.5), q = W2S(m.x + m.ty * sgnH * h * 0.5, m.y - m.tx * sgnH * h * 0.5); g.font = '600 9.5px Inter, system-ui, sans-serif'; g.textAlign = 'center'; g.fillStyle = pal.dim; g.fillText('HEM ' + AT.fmt(h, units) + (units === 'in' ? '' : ' ' + AT.unitLabel(units)), q[0], q[1] + (h * s > 14 ? 3.5 : h * s * 0.5 + 12)); }); }
      }
      // fill
      path(ol.poly, true);
      if (disp.texture && fab) {
        const pat = this.tilePattern(fab);
        const sc = (fab.scale || 10) * s / 96; const M = new DOMMatrix([sc, 0, 0, sc, V.ox, V.oy]).rotate((pc.grain || 0));
        pat.setTransform && pat.setTransform(M);
        g.fillStyle = pat; g.globalAlpha = st === 'off' ? 0.35 : 0.9; g.fill(); g.globalAlpha = 1;
      } else { g.fillStyle = fab ? fab.color : '#ddd'; g.globalAlpha = st === 'off' ? 0.12 : 0.28; g.fill(); g.globalAlpha = 1; }
      if (st === 'frozen') { g.fillStyle = 'rgba(37,99,235,0.18)'; g.fill(); }
      // graphics
      (pc.graphics || []).forEach((gr) => {
        const im = this.img(AT.resolveAsset(gr)), q = W2S(gr.x, gr.y);
        g.save(); g.translate(q[0], q[1]); g.rotate((gr.rot || 0) * AT.deg); g.globalAlpha = gr.opacity == null ? 1 : gr.opacity;
        if (im) g.drawImage(im, -gr.w / 2 * s, -gr.h / 2 * s, gr.w * s, gr.h * s); else { g.strokeStyle = pal.muted; g.strokeRect(-gr.w / 2 * s, -gr.h / 2 * s, gr.w * s, gr.h * s); }
        g.restore();
        if (sel.kind === 'graphic' && sel.id === gr.id) { g.strokeStyle = pal.accent; g.lineWidth = 1.5; g.strokeRect(q[0] - gr.w / 2 * s, q[1] - gr.h / 2 * s, gr.w * s, gr.h * s); g.fillStyle = pal.accent; [[-1, -1], [1, -1], [1, 1], [-1, 1]].forEach((c) => g.fillRect(q[0] + c[0] * gr.w / 2 * s - 3, q[1] + c[1] * gr.h / 2 * s - 3, 6, 6)); }
      });
      // outline
      path(ol.poly, true); g.lineWidth = selected ? 2 : 1.3; g.strokeStyle = selected ? pal.accent : (this.hover && this.hover.piece === pc && A.tool() === 'transform' ? pal.inkHover : pal.ink); g.stroke();
      g.setLineDash([]);
      // internal lines
      (pc.internals || []).forEach((il) => {
        const pl = AT.evalOpen(il.closed ? il.pts.concat([il.pts[0]]) : il.pts);
        const isSel = sel.kind === 'internal' && sel.id === il.id;
        g.setLineDash(il.fold != null && il.fold !== '' && il.fold != 180 ? [6, 3, 1, 3] : [4, 3]); g.lineWidth = isSel ? 2 : 1; g.strokeStyle = isSel ? pal.accent : pal.internal; path(pl, false); g.stroke(); g.setLineDash([]);
      });
      // notches
      (pc.notches || []).forEach((n) => { const k = ol.byId[n.s]; if (k == null) return; const p = AT.segPoint(ol.segs[k], n.t || 0), sg = AT.area(ol.poly) > 0 ? 1 : -1, nx = -p.ty * sg, ny = p.tx * sg, L = 6 / s; const a = W2S(p.x, p.y), b = W2S(p.x - nx * L, p.y - ny * L); g.strokeStyle = pal.ink; g.lineWidth = 1.5; g.beginPath(); g.moveTo(a[0], a[1]); g.lineTo(b[0], b[1]); g.stroke(); });
      // grain line
      if (disp.grain) { const bb = AT.bbox(ol.poly), gr = (pc.grain || 0) * AT.deg, L = bb.h * 0.3; const a = W2S(bb.cx + Math.sin(gr) * L, bb.cy - Math.cos(gr) * L), b = W2S(bb.cx - Math.sin(gr) * L, bb.cy + Math.cos(gr) * L); g.strokeStyle = pal.muted; g.lineWidth = 1; g.beginPath(); g.moveTo(a[0], a[1]); g.lineTo(b[0], b[1]); g.stroke(); const ah = (P, Q) => { const ang = Math.atan2(Q[1] - P[1], Q[0] - P[0]); g.beginPath(); g.moveTo(P[0], P[1]); g.lineTo(P[0] + Math.cos(ang + 0.45) * 7, P[1] + Math.sin(ang + 0.45) * 7); g.moveTo(P[0], P[1]); g.lineTo(P[0] + Math.cos(ang - 0.45) * 7, P[1] + Math.sin(ang - 0.45) * 7); g.stroke(); }; ah(a, b); ah(b, a); }
      // topstitches
      Object.values(d.stitches || {}).forEach((stc) => { if (stc.p !== pc.id) return; AT.drawStitch(g, ol, stc, (x) => x * s + V.ox, (y) => y * s + V.oy, s, true); if (sel.kind === 'stitch' && sel.id === stc.id) { g.globalAlpha = 0.25; AT.drawStitch(g, ol, Object.assign({}, stc, { color: pal.accent, w: 3 / s, len: 1000, gap: 0 }), (x) => x * s + V.ox, (y) => y * s + V.oy, s, true); g.globalAlpha = 1; } });
      // name
      if (disp.names) { const bb = AT.bbox(ol.poly), c = W2S(bb.cx, bb.cy); g.fillStyle = pal.label; g.font = '500 11px Inter, system-ui, sans-serif'; g.textAlign = 'center'; g.fillText(pc.name + (st === 'frozen' ? ' · frozen' : st === 'off' ? ' · deactivated' : ''), c[0], c[1] + (disp.grain ? -6 : 4)); if (d.activeSize && d.sizes.length > 1) { g.fillStyle = pal.muted; g.font = '10px Inter, system-ui, sans-serif'; g.fillText(d.activeSize, c[0], c[1] + 8); } }
      // pattern dimensions: net width × length, drawn outside the seam allowance
      if (disp.dims) {
        const bb = AT.bbox(ol.poly), wpx = bb.w * s, hpx = bb.h * s, showW = wpx > 24, showH = hpx > 24;
        if (showW || showH) {
          const saOff = (disp.sa ? AT.saMax(pc) : 0) * s + 14;
          let pl = ol.poly[0], pr = pl, pt = pl, pb = pl;
          ol.poly.forEach((p) => { if (p.x < pl.x) pl = p; if (p.x > pr.x) pr = p; if (p.y < pt.y) pt = p; if (p.y > pb.y) pb = p; });
          const L0 = W2S(pl.x, pl.y), R0 = W2S(pr.x, pr.y), T0 = W2S(pt.x, pt.y), B0 = W2S(pb.x, pb.y);
          const top = Math.round(W2S(0, bb.y0)[1] - saOff) + 0.5, left = Math.round(W2S(bb.x0, 0)[0] - saOff) + 0.5;
          const eT = top + saOff - 10, eL = left + saOff - 10;
          const col = selected ? pal.accent : pal.dim;
          const tri = (x, y, dx, dy) => { g.beginPath(); g.moveTo(x, y); g.lineTo(x - dx * 6 - dy * 2.4, y - dy * 6 + dx * 2.4); g.lineTo(x - dx * 6 + dy * 2.4, y - dy * 6 - dx * 2.4); g.closePath(); g.fill(); };
          const lbl = (v) => AT.fmt(v, units) + (units === 'in' ? '' : ' ' + AT.unitLabel(units));
          const chip = (txt, x, y, rot) => { g.save(); g.translate(x, y); if (rot) g.rotate(-Math.PI / 2); g.font = '500 10.5px ui-monospace, SFMono-Regular, Menlo, monospace'; g.textAlign = 'center'; const tw = g.measureText(txt).width + 10; g.fillStyle = pal.chip; g.fillRect(-tw / 2, -8, tw, 15); g.fillStyle = col; g.fillText(txt, 0, 3.5); g.restore(); };
          g.strokeStyle = col; g.fillStyle = col; g.lineWidth = 1;
          if (showW) {
            g.globalAlpha = 0.8; g.beginPath();
            g.moveTo(L0[0], Math.min(L0[1] - 4, eT)); g.lineTo(L0[0], top - 4); g.moveTo(R0[0], Math.min(R0[1] - 4, eT)); g.lineTo(R0[0], top - 4); g.moveTo(L0[0], top); g.lineTo(R0[0], top);
            g.stroke(); g.globalAlpha = 1; tri(L0[0], top, -1, 0); tri(R0[0], top, 1, 0);
            if (wpx > 56) chip(lbl(bb.w), (L0[0] + R0[0]) / 2, top, false);
          }
          if (showH) {
            g.globalAlpha = 0.8; g.beginPath();
            g.moveTo(Math.min(T0[0] - 4, eL), T0[1]); g.lineTo(left - 4, T0[1]); g.moveTo(Math.min(B0[0] - 4, eL), B0[1]); g.lineTo(left - 4, B0[1]); g.moveTo(left, T0[1]); g.lineTo(left, B0[1]);
            g.stroke(); g.globalAlpha = 1; tri(left, T0[1], 0, -1); tri(left, B0[1], 0, 1);
            if (hpx > 56) chip(lbl(bb.h), left, (T0[1] + B0[1]) / 2, true);
          }
        }
      }
      // segment lengths
      if (disp.lengths || (sel.kind === 'segment' && sel.piece === pc.id) || (this.hover && this.hover.seg && this.hover.seg.pc === pc && ['edit', 'segsew', 'freesew', 'addpoint', 'curvature', 'stitch'].indexOf(A.tool()) >= 0)) {
        g.font = '10px ui-monospace, SFMono-Regular, Menlo, monospace'; g.textAlign = 'center';
        ol.segs.forEach((sg) => {
          const show = disp.lengths || (sel.kind === 'segment' && sel.seg === sg.id) || (this.hover && this.hover.seg && this.hover.seg.pc === pc && this.hover.seg.id === sg.id);
          if (!show) return;
          const m = AT.segPoint(sg, 0.5), sgn = AT.area(ol.poly) > 0 ? 1 : -1, q = W2S(m.x + m.ty * sgn * 12 / s, m.y - m.tx * sgn * 12 / s);
          const txt = AT.fmt(sg.len, units);
          const tw = g.measureText(txt).width + 8; g.fillStyle = pal.chip; g.fillRect(q[0] - tw / 2, q[1] - 8, tw, 14); g.fillStyle = pal.text; g.fillText(txt, q[0], q[1] + 3);
        });
      }
      // points (edit-ish tools or selected piece)
      const showPts = ['edit', 'curvept', 'curvature', 'addpoint'].indexOf(A.tool()) >= 0 || selected;
      if (showPts) {
        const pts = this.pts(pc), selIds = sel.kind === 'points' && sel.piece === pc.id ? sel.ids : [];
        // curve control polygon
        g.strokeStyle = pal.muted; g.lineWidth = 0.75; g.setLineDash([2, 2]);
        ol.segs.forEach((sg) => { if (sg.ctrl.length > 2) { path(sg.ctrl, false); g.stroke(); } });
        g.setLineDash([]);
        pts.forEach((p) => {
          const q = W2S(p.x, p.y), isS = selIds.indexOf(p.id) >= 0;
          g.fillStyle = isS ? pal.accent : pal.pointFill; g.strokeStyle = isS ? pal.accent : pal.ink; g.lineWidth = 1.2;
          if (p.c) { g.beginPath(); g.arc(q[0], q[1], 3.5, 0, 6.29); g.fill(); g.stroke(); }
          else { g.fillRect(q[0] - 3.5, q[1] - 3.5, 7, 7); g.strokeRect(q[0] - 3.5, q[1] - 3.5, 7, 7); }
        });
      }
      // selected segment
      if (sel.kind === 'segment' && sel.piece === pc.id) { const k = ol.byId[sel.seg]; if (k != null) { g.strokeStyle = pal.accent; g.lineWidth = 3; path(ol.segs[k].poly, false); g.stroke(); } }
    });
    // sewing lines
    if (disp.sewing) {
      const lens = AT.sewLengths(d), seamNo = AT.seamNumbers(d), badges = [];
      Object.values(d.sewing || {}).forEach((sw) => {
        const isSel = sel.kind === 'sewing' && sel.id === sw.id;
        const mids = [];
        ['a', 'b'].forEach((side) => {
          sw[side].forEach((sd) => {
            const pc = d.pieces[sd.p]; if (!pc) return;
            const pl = AT.sidePolyline(this.ol(pc), sd, 3 / s + 1); if (pl.length < 2) return;
            g.strokeStyle = sw.color || pal.accent; g.lineWidth = isSel ? 4.5 : 3; g.globalAlpha = isSel ? 1 : 0.85; path(pl, false); g.stroke(); g.globalAlpha = 1;
            // direction ticks at start
            const a = W2S(pl[0].x, pl[0].y), b = W2S(pl[1].x, pl[1].y), ang = Math.atan2(b[1] - a[1], b[0] - a[0]);
            g.lineWidth = 2; g.beginPath(); g.moveTo(a[0] - Math.sin(ang) * 6, a[1] + Math.cos(ang) * 6); g.lineTo(a[0] + Math.sin(ang) * 6, a[1] - Math.cos(ang) * 6); g.stroke();
            const mid = pl[Math.floor(pl.length / 2)]; mids.push(W2S(mid.x, mid.y)); badges.push([W2S(mid.x, mid.y), seamNo[sw.id], sw.color || pal.accent]);
          });
        });
        if (isSel && mids.length >= 2) { g.strokeStyle = sw.color; g.setLineDash([3, 3]); g.lineWidth = 1; g.beginPath(); g.moveTo(mids[0][0], mids[0][1]); for (let i = 1; i < mids.length; i++) g.lineTo(mids[i][0], mids[i][1]); g.stroke(); g.setLineDash([]); }
        const L = lens[sw.id];
        if (L && Math.abs(L.a - L.b) > 2 && mids.length) { const q = mids[0], txt = 'Δ ' + AT.fmt(Math.abs(L.a - L.b), units); g.font = '600 10px Inter, system-ui, sans-serif'; const tw = g.measureText(txt).width + 8; g.fillStyle = sw.color; g.fillRect(q[0] - tw / 2, q[1] - 18, tw, 14); g.fillStyle = '#fff'; g.textAlign = 'center'; g.fillText(txt, q[0], q[1] - 8); }
      });
      g.font = '700 10px Inter, system-ui, sans-serif'; g.textAlign = 'center';
      badges.forEach((b) => { if (!b[1]) return; const r = b[1] > 9 ? 8.5 : 7.5; g.fillStyle = b[2]; g.beginPath(); g.arc(b[0][0], b[0][1], r, 0, 6.2832); g.fill(); g.strokeStyle = pal.canvas2d; g.lineWidth = 1.5; g.stroke(); g.fillStyle = '#fff'; g.fillText(String(b[1]), b[0][0], b[0][1] + 3.5); });
    }
    // buttons & holes
    Object.values(d.buttons || {}).forEach((b) => { const q = W2S(b.x, b.y), r = Math.max(3, (b.size || 15) / 2 * s); g.fillStyle = b.color || '#333'; g.beginPath(); g.arc(q[0], q[1], r, 0, 6.29); g.fill(); g.strokeStyle = (sel.kind === 'button' && sel.id === b.id) || this.fastenFrom === b.id ? pal.accent : 'rgba(255,255,255,0.7)'; g.lineWidth = 1.5; g.stroke(); if (b.fasten && d.holes[b.fasten]) { const h = W2S(d.holes[b.fasten].x, d.holes[b.fasten].y); g.setLineDash([3, 3]); g.strokeStyle = pal.muted; g.lineWidth = 1; g.beginPath(); g.moveTo(q[0], q[1]); g.lineTo(h[0], h[1]); g.stroke(); g.setLineDash([]); } });
    Object.values(d.holes || {}).forEach((h) => { const q = W2S(h.x, h.y), L = (h.len || 18) * s; g.save(); g.translate(q[0], q[1]); g.rotate((h.ang || 0) * AT.deg); g.strokeStyle = sel.kind === 'hole' && sel.id === h.id ? pal.accent : pal.ink; g.lineWidth = 2; g.beginPath(); g.moveTo(-L / 2, 0); g.lineTo(L / 2, 0); g.stroke(); g.beginPath(); g.moveTo(-L / 2, -3); g.lineTo(-L / 2, 3); g.moveTo(L / 2, -3); g.lineTo(L / 2, 3); g.stroke(); g.restore(); });
    // comments
    if (disp.comments) Object.values(d.comments || {}).forEach((c) => { if (c.resolved && !disp.resolved) return; const q = W2S(c.x, c.y); g.fillStyle = c.resolved ? pal.muted : pal.accent; g.beginPath(); g.arc(q[0], q[1] - 10, 9, 0, 6.29); g.fill(); g.beginPath(); g.moveTo(q[0] - 4, q[1] - 4); g.lineTo(q[0], q[1]); g.lineTo(q[0] + 4, q[1] - 4); g.fill(); g.fillStyle = '#fff'; g.font = '600 10px Inter, system-ui, sans-serif'; g.textAlign = 'center'; g.fillText(String(c.n || '•'), q[0], q[1] - 6.5); if (sel.kind === 'comment' && sel.id === c.id) { g.strokeStyle = pal.text; g.lineWidth = 1.5; g.beginPath(); g.arc(q[0], q[1] - 10, 11, 0, 6.29); g.stroke(); } });
    // hover highlight for segment tools
    const H = this.hover, tool = A.tool();
    if (H && H.seg && ['segsew', 'freesew', 'addpoint', 'curvept', 'curvature', 'notch', 'stitch', 'edit'].indexOf(tool) >= 0 && !this.drag) {
      const ol = this.ol(H.seg.pc), sg = ol.segs[H.seg.k];
      if (tool !== 'freesew') { g.strokeStyle = pal.hover; g.lineWidth = 4; g.globalAlpha = 0.6; path(sg.poly, false); g.stroke(); g.globalAlpha = 1; }
      const q = W2S(H.seg.x, H.seg.y); g.fillStyle = pal.accent; g.beginPath(); g.arc(q[0], q[1], 3.5, 0, 6.29); g.fill();
      if (tool === 'segsew') { const st = AT.segPoint(sg, H.seg.t > 0.5 ? 1 : 0), qq = W2S(st.x, st.y); g.strokeStyle = pal.accent; g.lineWidth = 2; g.beginPath(); g.arc(qq[0], qq[1], 6, 0, 6.29); g.stroke(); }
    }
    // sewing draft
    if (this.sewDraft) {
      const sd = this.sewDraft;
      ['a', 'b'].forEach((side) => sd[side].forEach((sdd) => { const pc = d.pieces[sdd.p]; if (!pc) return; const pl = AT.sidePolyline(this.ol(pc), sdd, 4 / s); g.strokeStyle = side === 'a' ? pal.accent : '#3B82F6'; g.lineWidth = 4; path(pl, false); g.stroke(); }));
      if (sd.pending) { const q = W2S(sd.pending.x, sd.pending.y); g.fillStyle = pal.accent; g.beginPath(); g.arc(q[0], q[1], 5, 0, 6.29); g.fill(); if (H && H.seg && H.seg.pc.id === sd.pending.pc) { const ol = this.ol(H.seg.pc), Pa = AT.perimParam(ol, ol.byId[sd.pending.s], sd.pending.t), Pb = AT.perimParam(ol, H.seg.k, H.seg.t), T = ol.total, fwd = ((Pb - Pa) % T + T) % T; const dir = fwd <= T / 2 ? 1 : -1; const pl = AT.sidePolyline(ol, { a: { s: sd.pending.s, t: sd.pending.t }, b: { s: H.seg.id, t: H.seg.t }, dir: dir }, 4 / s); g.strokeStyle = pal.accent; g.globalAlpha = 0.55; g.lineWidth = 4; path(pl, false); g.stroke(); g.globalAlpha = 1; } }
    }
    // drafts
    if (this.draft && this.cursor) {
      const dr = this.draft, pts = dr.pts.concat([{ x: this.cursor[0], y: this.cursor[1] }]);
      g.strokeStyle = pal.accent; g.lineWidth = 1.5; g.setLineDash(dr.tool === 'internal' ? [4, 3] : []); path(pts, false); g.stroke(); g.setLineDash([]);
      dr.pts.forEach((p) => { const q = W2S(p.x, p.y); g.fillStyle = pal.accent; if (p.c) { g.beginPath(); g.arc(q[0], q[1], 3.5, 0, 6.29); g.fill(); } else g.fillRect(q[0] - 3, q[1] - 3, 6, 6); });
      if (dr.pts.length) { const l = dr.pts[dr.pts.length - 1], L = Math.hypot(this.cursor[0] - l.x, this.cursor[1] - l.y), q = W2S(this.cursor[0], this.cursor[1]); g.fillStyle = pal.text; g.font = '10px ui-monospace, monospace'; g.textAlign = 'left'; g.fillText(AT.fmt(L, units) + ' ' + AT.unitLabel(units), q[0] + 10, q[1] - 8); }
    }
    if (this.drag && this.drag.type === 'shape') { const dr = this.drag, a = W2S(dr.x0, dr.y0), b = W2S(dr.x1, dr.y1); g.strokeStyle = pal.accent; g.lineWidth = 1.5; g.setLineDash([4, 3]); if (dr.tool === 'rect') g.strokeRect(a[0], a[1], b[0] - a[0], b[1] - a[1]); else { g.beginPath(); g.ellipse((a[0] + b[0]) / 2, (a[1] + b[1]) / 2, Math.abs(b[0] - a[0]) / 2, Math.abs(b[1] - a[1]) / 2, 0, 0, 6.29); g.stroke(); } g.setLineDash([]); g.fillStyle = pal.text; g.font = '10px ui-monospace, monospace'; g.fillText(AT.fmt(Math.abs(dr.x1 - dr.x0), units) + ' × ' + AT.fmt(Math.abs(dr.y1 - dr.y0), units), b[0] + 8, b[1] + 14); }
    if (this.drag && this.drag.type === 'box') { const dr = this.drag, a = W2S(dr.x0, dr.y0), b = W2S(dr.x1, dr.y1); g.fillStyle = pal.boxFill; g.strokeStyle = pal.accent; g.lineWidth = 1; g.fillRect(a[0], a[1], b[0] - a[0], b[1] - a[1]); g.strokeRect(a[0], a[1], b[0] - a[0], b[1] - a[1]); }
    if (this.measureLine) { const m = this.measureLine, a = W2S(m.a[0], m.a[1]), b = W2S(m.b[0], m.b[1]); g.strokeStyle = pal.accent; g.lineWidth = 1.5; g.beginPath(); g.moveTo(a[0], a[1]); g.lineTo(b[0], b[1]); g.stroke(); const L = Math.hypot(m.b[0] - m.a[0], m.b[1] - m.a[1]); const txt = AT.fmt(L, units) + ' ' + AT.unitLabel(units); g.font = '600 11px Inter, system-ui, sans-serif'; const tw = g.measureText(txt).width + 10; g.fillStyle = pal.accent; g.fillRect((a[0] + b[0]) / 2 - tw / 2, (a[1] + b[1]) / 2 - 20, tw, 16); g.fillStyle = '#fff'; g.textAlign = 'center'; g.fillText(txt, (a[0] + b[0]) / 2, (a[1] + b[1]) / 2 - 8); }
    if (this.snapMark && this.drag) { const q = W2S(this.snapMark[0], this.snapMark[1]); g.strokeStyle = '#10B981'; g.lineWidth = 1.5; g.beginPath(); g.arc(q[0], q[1], 7, 0, 6.29); g.stroke(); }
    // rotation handle
    const rh = this.rotHandle();
    if (rh) { const bb = rh[3], a = W2S(bb.x0, bb.y0), b = W2S(bb.x1, bb.y1); g.strokeStyle = pal.accent; g.globalAlpha = 0.5; g.setLineDash([3, 3]); g.lineWidth = 1; g.strokeRect(a[0] - 4, a[1] - 4, b[0] - a[0] + 8, b[1] - a[1] + 8); g.setLineDash([]); g.globalAlpha = 1; g.beginPath(); g.moveTo(rh[0], rh[1] + 6); g.lineTo(rh[0], a[1] - 4); g.stroke(); g.fillStyle = pal.panel; g.beginPath(); g.arc(rh[0], rh[1], 6, 0, 6.29); g.fill(); g.strokeStyle = pal.accent; g.lineWidth = 1.5; g.stroke(); }
  }
};


/* ============================================================
   TECH PACK — BOM, POM, pattern list, PDF/CSV export
   ============================================================ */
AT.techData = function (doc) {
  const pieces = AT.asArray(doc.pieces);
  const fabRows = {};
  pieces.forEach(function (pc) {
    const fid = pc.fabric, fab = doc.fabrics[fid]; if (!fab) return;
    const q = pc.qty || 1;
    const net = AT.pieceArea(doc, pc, doc.activeSize, false), gross = AT.pieceArea(doc, pc, doc.activeSize, true);
    const r = fabRows[fid] || (fabRows[fid] = { fab: fab, pieces: [], net: 0, gross: 0 });
    r.pieces.push(pc.name + (q > 1 ? ' ×' + q : '')); r.net += net * q; r.gross += gross * q;
  });
  const fabrics = Object.keys(fabRows).map(function (k) {
    const r = fabRows[k], width = r.fab.width || 1450, waste = r.fab.waste == null ? 0.25 : r.fab.waste;
    const linear = r.gross / width * (1 + waste) / 1000; // metres
    return { id: k, name: r.fab.name, color: (AT.effFabric(doc, k) || r.fab).color, content: r.fab.content || AT.FABRIC_PRESETS[r.fab.preset] && AT.FABRIC_PRESETS[r.fab.preset].name || '', weight: r.fab.phys && r.fab.phys.density, width: width, placement: r.pieces.join(', '), areaNet: r.net / 1e6, areaGross: r.gross / 1e6, linear: linear, price: r.fab.price || 0, cost: linear * (r.fab.price || 0) };
  });
  const btnGroups = {};
  Object.values(doc.buttons || {}).forEach(function (b) { const k = [b.shape || 'round', b.size || 15, b.color, b.finish || 'plastic'].join('|'); (btnGroups[k] = btnGroups[k] || { shape: b.shape || 'round', size: b.size || 15, color: b.color, finish: b.finish || 'plastic', qty: 0 }).qty++; });
  const holes = Object.values(doc.holes || {}).length;
  let thread = 0;
  Object.values(doc.stitches || {}).forEach(function (st) { const pc = doc.pieces[st.p]; if (!pc) return; const ol = AT.outline(AT.gradedPts(pc, doc.activeSize, doc.baseSize)); const segs = st.seg === '*' ? ol.segs : [ol.segs[ol.byId[st.seg]]].filter(Boolean); segs.forEach(function (sg) { thread += sg.len * (st.rows || 1); }); });
  const trims = Object.values(btnGroups).map(function (g) { return { name: 'Button — ' + g.shape + ' ' + g.size + ' mm (' + g.finish + ')', color: g.color, qty: g.qty, unit: 'pcs' }; });
  if (holes) trims.push({ name: 'Buttonhole — keyhole', color: '', qty: holes, unit: 'pcs' });
  if (thread) trims.push({ name: 'Topstitch thread (incl. 2.5× take-up)', color: '', qty: Math.ceil(thread * 2.5 / 1000), unit: 'm' });
  const patterns = pieces.map(function (pc) {
    const ol = AT.outline(AT.gradedPts(pc, doc.activeSize, doc.baseSize));
    const fab = doc.fabrics[pc.fabric];
    const bbx = AT.bbox(ol.poly);
    return { name: pc.name, fabric: fab ? fab.name : '—', qty: pc.qty || 1, w: bbx.w, h: bbx.h, net: Math.abs(AT.area(ol.poly)) / 1e4, gross: AT.pieceArea(doc, pc, doc.activeSize, true) / 1e4, perim: ol.total, sa: pc.sa || 0, grain: pc.grain || 0, pd: pc.pd || 20 };
  });
  const poms = Object.values(doc.poms || {}).map(function (pm) { const vals = {}; doc.sizes.forEach(function (sz) { vals[sz] = AT.pomValue(doc, pm, sz); }); return { id: pm.id, name: pm.name, tol: pm.tol == null ? 6 : pm.tol, vals: vals }; });
  const sl = AT.sewLengths(doc), seamNo = AT.seamNumbers(doc);
  const sewing = Object.values(doc.sewing || {}).filter(Boolean).map(function (sw, i) {
    const nm = function (sides) { return sides.map(function (sd) { const pc = doc.pieces[sd.p]; return pc ? pc.name : '?'; }).join(' + '); };
    const L = sl[sw.id] || { a: 0, b: 0 };
    return { n: seamNo[sw.id] || i + 1, color: sw.color, a: nm(sw.a), b: nm(sw.b), la: L.a, lb: L.b, diff: L.a - L.b };
  }).sort(function (a, b) { return a.n - b.n; });
  const totalCost = fabrics.reduce(function (s, f) { return s + f.cost; }, 0);
  return { fabrics: fabrics, trims: trims, patterns: patterns, poms: poms, sewing: sewing, totalCost: totalCost };
};
AT.techCSV = function (doc) {
  const D = AT.techData(doc), q = function (v) { return '"' + String(v == null ? '' : v).replace(/"/g, '""') + '"'; };
  const rows = [['Atelier tech pack', doc.meta.name], [], ['BILL OF MATERIALS'], ['Type', 'Item', 'Colour', 'Placement', 'Width (mm)', 'Consumption', 'Unit', 'Price/unit', 'Cost']];
  D.fabrics.forEach(function (f) { rows.push(['Fabric', f.name, f.color, f.placement, f.width, f.linear.toFixed(2), 'm', f.price, f.cost.toFixed(2)]); });
  D.trims.forEach(function (t) { rows.push(['Trim', t.name, t.color, '', '', t.qty, t.unit, '', '']); });
  rows.push([], ['POINTS OF MEASURE (cm)'], ['POM', 'Tol ±'].concat(doc.sizes));
  D.poms.forEach(function (p) { rows.push([p.name, (p.tol / 10).toFixed(1)].concat(doc.sizes.map(function (s) { return (p.vals[s] / 10).toFixed(1); }))); });
  rows.push([], ['PATTERN PIECES'], ['Piece', 'Fabric', 'Qty', 'Width (cm)', 'Length (cm)', 'Net area (cm²)', 'Cut area (cm²)', 'Perimeter (cm)', 'SA (mm)', 'Grain (°)']);
  D.patterns.forEach(function (p) { rows.push([p.name, p.fabric, p.qty, (p.w / 10).toFixed(1), (p.h / 10).toFixed(1), p.net.toFixed(0), p.gross.toFixed(0), (p.perim / 10).toFixed(1), p.sa, p.grain]); });
  rows.push([], ['SEWING'], ['#', 'Side A', 'Side B', 'Length A (cm)', 'Length B (cm)', 'Difference (mm)']);
  D.sewing.forEach(function (s) { rows.push([s.n, s.a, s.b, (s.la / 10).toFixed(1), (s.lb / 10).toFixed(1), s.diff.toFixed(1)]); });
  return rows.map(function (r) { return r.map(q).join(','); }).join('\n');
};
AT.techPDF = function (doc, images, info) {
  const J = window.jspdf && window.jspdf.jsPDF; if (!J) throw new Error('PDF library not loaded');
  const pdf = AT.pdfInstallUnicode(new J({ orientation: 'landscape', unit: 'mm', format: 'a4', compress: true }));
  const D = AT.techData(doc), W = 297, H = 210, M = 14;
  const ink = [28, 29, 25], mute = [120, 120, 125], acc = [232, 101, 74];
  const header = function (title, page) {
    pdf.setFillColor(250, 249, 247); pdf.rect(0, 0, W, H, 'F');
    pdf.setTextColor.apply(pdf, ink); pdf.setFont('helvetica', 'bold'); pdf.setFontSize(9); pdf.text('ATELIER · TECH PACK', M, 10);
    pdf.setFont('helvetica', 'normal'); pdf.setTextColor.apply(pdf, mute); pdf.text(doc.meta.name + '  ·  ' + (info.season || '') + '  ·  base size ' + doc.baseSize + '  ·  ' + new Date().toLocaleDateString(), M + 48, 10);
    pdf.text('p. ' + page, W - M, 10, { align: 'right' });
    pdf.setDrawColor(220, 218, 214); pdf.line(M, 13, W - M, 13);
    pdf.setTextColor.apply(pdf, ink); pdf.setFont('helvetica', 'bold'); pdf.setFontSize(15); pdf.text(title, M, 23);
  };
  const table = function (x, y, cols, rows, opts) {
    opts = opts || {}; const rh = opts.rh || 6.2;
    pdf.setFontSize(7.5); pdf.setFont('helvetica', 'bold'); pdf.setTextColor.apply(pdf, mute);
    let cx = x; cols.forEach(function (c) { pdf.text(String(c.h).toUpperCase(), cx + (c.align === 'right' ? c.w - 1 : 0), y, { align: c.align === 'right' ? 'right' : 'left' }); cx += c.w; });
    pdf.setDrawColor(200, 198, 194); pdf.line(x, y + 1.6, x + cols.reduce(function (s, c) { return s + c.w; }, 0), y + 1.6);
    pdf.setFont('helvetica', 'normal'); pdf.setTextColor.apply(pdf, ink); pdf.setFontSize(8.5);
    let yy = y + rh;
    rows.forEach(function (r) {
      if (yy > H - 12) return;
      cx = x;
      cols.forEach(function (c, i) {
        const v = r[i];
        if (c.swatch && v) { const n = parseInt(String(v).slice(1), 16); if (!isNaN(n)) { pdf.setFillColor((n >> 16) & 255, (n >> 8) & 255, n & 255); pdf.rect(cx, yy - 3.4, 4, 4, 'F'); } }
        else { let t = String(v == null ? '' : v); const maxC = Math.floor(c.w / 1.65); if (t.length > maxC) t = t.slice(0, maxC - 1) + '…'; pdf.text(t, cx + (c.align === 'right' ? c.w - 1 : 0), yy, { align: c.align === 'right' ? 'right' : 'left' }); }
        cx += c.w;
      });
      pdf.setDrawColor(236, 234, 230); pdf.line(x, yy + 1.8, cx, yy + 1.8);
      yy += rh;
    });
    return yy;
  };
  // page 1: overview
  header(doc.meta.name, 1);
  const imgs = images || {};
  let ix = M;
  ['front', 'back', 'side'].forEach(function (k) { if (imgs[k]) { try { pdf.addImage(imgs[k], 'PNG', ix, 30, 70, 120); } catch (e) {} pdf.setFontSize(8); pdf.setTextColor.apply(pdf, mute); pdf.text(k.toUpperCase(), ix, 155); ix += 74; } });
  pdf.setTextColor.apply(pdf, ink); pdf.setFontSize(9);
  const facts = [['Style', doc.meta.name], ['Season', info.season || '—'], ['Designer', info.designer || '—'], ['Size range', doc.sizes.join(' / ')], ['Base size', doc.baseSize], ['Colorway', (doc.colorways[doc.activeColorway] || {}).name || '—'], ['Pieces', String(AT.asArray(doc.pieces).length)], ['Est. fabric cost', D.totalCost ? D.totalCost.toFixed(2) + ' ' + (info.currency || '') : '—']];
  let fy = 34; facts.forEach(function (f) { pdf.setTextColor.apply(pdf, mute); pdf.setFontSize(7.5); pdf.text(f[0].toUpperCase(), 238, fy); pdf.setTextColor.apply(pdf, ink); pdf.setFontSize(10); pdf.text(String(f[1]), 238, fy + 5); fy += 13; });
  if (info.notes) { pdf.setFontSize(8.5); pdf.setTextColor.apply(pdf, ink); pdf.text(pdf.splitTextToSize(info.notes, W - 2 * M), M, 166); }
  // page 2: BOM
  pdf.addPage(); header('Bill of materials', 2);
  let y = table(M, 32, [{ h: '', w: 7, swatch: true }, { h: 'Material', w: 55 }, { h: 'Placement', w: 80 }, { h: 'Weight g/m²', w: 22, align: 'right' }, { h: 'Width mm', w: 20, align: 'right' }, { h: 'Cut area m²', w: 22, align: 'right' }, { h: 'Consumption m', w: 26, align: 'right' }, { h: 'Price/m', w: 18, align: 'right' }, { h: 'Cost', w: 18, align: 'right' }],
    D.fabrics.map(function (f) { return [f.color, f.name, f.placement, f.weight || '', f.width, f.areaGross.toFixed(3), f.linear.toFixed(2), f.price || '', f.cost ? f.cost.toFixed(2) : '']; }));
  y = table(M, y + 8, [{ h: '', w: 7, swatch: true }, { h: 'Trim', w: 135 }, { h: 'Qty', w: 20, align: 'right' }, { h: 'Unit', w: 20 }], D.trims.map(function (t) { return [t.color, t.name, t.qty, t.unit]; }));
  // page 3: POM
  pdf.addPage(); header('Points of measure (cm)', 3);
  const sz = doc.sizes, colsP = [{ h: 'POM', w: 70 }, { h: 'Tol ±', w: 16, align: 'right' }].concat(sz.map(function (s) { return { h: s + (s === doc.baseSize ? ' ●' : ''), w: Math.min(26, (W - 2 * M - 86) / Math.max(1, sz.length)), align: 'right' }; }));
  table(M, 32, colsP, D.poms.map(function (p) { return [p.name, (p.tol / 10).toFixed(1)].concat(sz.map(function (s) { return (p.vals[s] / 10).toFixed(1); })); }));
  // page 4: patterns & sewing
  pdf.addPage(); header('Pattern pieces & sewing', 4);
  y = table(M, 32, [{ h: 'Piece', w: 44 }, { h: 'Fabric', w: 40 }, { h: 'Qty', w: 10, align: 'right' }, { h: 'W × L cm', w: 26, align: 'right' }, { h: 'Net cm²', w: 20, align: 'right' }, { h: 'Cut cm²', w: 22, align: 'right' }, { h: 'Perim cm', w: 22, align: 'right' }, { h: 'SA mm', w: 16, align: 'right' }, { h: 'Grain°', w: 16, align: 'right' }],
    D.patterns.map(function (p) { return [p.name, p.fabric, p.qty, (p.w / 10).toFixed(1) + ' × ' + (p.h / 10).toFixed(1), p.net.toFixed(0), p.gross.toFixed(0), (p.perim / 10).toFixed(1), p.sa, p.grain]; }));
  table(M, y + 8, [{ h: '', w: 7, swatch: true }, { h: '#', w: 8 }, { h: 'Side A', w: 80 }, { h: 'Side B', w: 80 }, { h: 'A cm', w: 20, align: 'right' }, { h: 'B cm', w: 20, align: 'right' }, { h: 'Δ mm', w: 18, align: 'right' }],
    D.sewing.map(function (s) { return [s.color, s.n, s.a, s.b, (s.la / 10).toFixed(1), (s.lb / 10).toFixed(1), s.diff.toFixed(1)]; }));
  // page 5: colorways + comments
  pdf.addPage(); header('Colorways & review notes', 5);
  const cws = AT.asArray(doc.colorways).sort(function (a, b) { return (a.order || 0) - (b.order || 0); });
  const fids = Object.keys(doc.fabrics);
  table(M, 32, [{ h: 'Material', w: 60 }].concat(cws.map(function (c) { return { h: c.name, w: 30, swatch: true }; })), fids.map(function (fid) { return [doc.fabrics[fid].name].concat(cws.map(function (c) { return (c.over && c.over[fid] && c.over[fid].color) || doc.fabrics[fid].color; })); }));
  const cm = AT.asArray(doc.comments).filter(function (c) { return !c.resolved; });
  table(M, 32 + 8 + (fids.length + 1) * 6.2 + 8, [{ h: '#', w: 10 }, { h: 'Pattern', w: 40 }, { h: 'Note', w: 170 }, { h: 'By', w: 40 }], cm.map(function (c) { const pc = doc.pieces[c.p]; return [c.n || '', pc ? pc.name : '', c.text, c.byName || '']; }));
  return pdf;
};


/* ============================================================
   PATTERN DIMENSIONS + PATTERN PDF
   Net = sewing line; cut = with seam allowance. All values in mm.
   ============================================================ */
AT.allowanceSummary = function (pc) {
  const ol = AT.outline(pc.pts), sa = {}, hm = {};
  ol.segs.forEach(function (sg) { const h = pc.hem && pc.hem[sg.id]; if (h > 0) hm[h] = 1; else sa[AT.edgeAllowance(pc, sg.id)] = 1; });
  const fmt = function (o) { const v = Object.keys(o).map(Number).sort(function (a, b) { return a - b; }); return v.length > 1 ? v[0] + '–' + v[v.length - 1] : String(v[0]); };
  return (Object.keys(sa).length ? 'SA ' + fmt(sa) + ' mm' : '') + (Object.keys(hm).length ? (Object.keys(sa).length ? ' · ' : '') + 'hem ' + Object.keys(hm).map(Number).sort(function (a, b) { return a - b; }).join('/') + ' mm' : '');
};
AT.saMax = function (pc) { let m = pc.sa || 0; [pc.saSeg, pc.hem].forEach(function (o) { if (o) Object.keys(o).forEach(function (k) { if (o[k] > m) m = o[k]; }); }); return m; };
AT.pieceGeom = function (doc, pc, size) {
  const sz = size || doc.activeSize;
  const ol = AT.outline(AT.gradedPts(pc, sz, doc.baseSize));
  const hasSA = AT.saMax(pc) > 0;
  const cut = hasSA ? AT.cutOutline(pc, ol) : ol.poly;
  return { ol: ol, cut: cut, bb: AT.bbox(ol.poly), cb: AT.bbox(cut), hasSA: hasSA, area: Math.abs(AT.area(ol.poly)), cutArea: Math.abs(AT.area(cut)), perim: ol.total };
};
AT.pieceDims = function (doc, pc, size) {
  const g = AT.pieceGeom(doc, pc, size);
  return { w: g.bb.w, h: g.bb.h, cw: g.cb.w, ch: g.cb.h, area: g.area, cutArea: g.cutArea, perim: g.perim };
};
// length with unit, plain ASCII (safe for PDF core fonts)
AT.fmtU = function (mm, units) {
  if (mm == null || isNaN(mm)) return '–';
  if (units === 'in') return (mm / 25.4).toFixed(2) + ' in';
  if (units === 'mm') return Math.round(mm) + ' mm';
  return (mm / 10).toFixed(1) + ' cm';
};
AT.fmtArea = function (mm2, units) { return units === 'in' ? (mm2 / 645.16).toFixed(1) + ' in²' : (mm2 / 100).toFixed(0) + ' cm²'; };
AT.fileName = function (s) { return String(s || 'garment').replace(/[\\/:*?"<>|\u0000-\u001f]+/g, ' ').replace(/\s+/g, ' ').trim() || 'garment'; };

/* ---------- Unicode text for jsPDF ----------
   jsPDF core fonts only cover Latin-1. Text in other scripts (Persian, Arabic, CJK…) is drawn by the browser
   (correct shaping and right-to-left order) into a high-resolution image placed where the text belongs. */
AT.pdfNeedsRaster = function (s) { return /[^\x0a\x0d\x20-\x7e\xa0-\xff\u2013\u2014\u2018\u2019\u201c\u201d\u2022\u2026]/.test(String(s)); };
AT.PDF_FONT = 'Inter, "SF Arabic", "Geeza Pro", "Segoe UI", Tahoma, "Noto Sans Arabic", "Noto Naskh Arabic", "Noto Sans", system-ui, sans-serif';
AT.pdfInstallUnicode = function (pdf) {
  if (pdf.__uni) return pdf;
  pdf.__uni = true;
  const origText = pdf.text.bind(pdf), origColor = pdf.setTextColor.bind(pdf), cache = {};
  let col = [0, 0, 0];
  pdf.setTextColor = function (a, b, c) {
    if (typeof a === 'string' && a[0] === '#') { const n = parseInt(a.slice(1), 16); col = [(n >> 16) & 255, (n >> 8) & 255, n & 255]; }
    else if (b == null && typeof a === 'number') col = [a, a, a];
    else if (typeof a === 'number') col = [a, b, c];
    return origColor.apply(pdf, arguments);
  };
  const S = 10; // canvas px per pt
  const raster = function (s, fs, bold, angle) {
    const key = s + '|' + fs + '|' + bold + '|' + col.join(',') + '|' + angle;
    if (cache[key]) return cache[key];
    const cv = document.createElement('canvas'), cx = cv.getContext('2d');
    const px = fs * S, font = (bold ? '600 ' : '400 ') + px + 'px ' + AT.PDF_FONT;
    cx.font = font;
    const rtl = /^[^A-Za-z0-9]*[\u0590-\u08ff\ufb1d-\ufdff\ufe70-\ufeff]/.test(s);
    const tw = Math.ceil(cx.measureText(s).width) + 4, th = Math.ceil(px * 1.5);
    const vert = Math.abs(angle) === 90;
    cv.width = vert ? th : tw; cv.height = vert ? tw : th;
    cx.font = font; cx.direction = rtl ? 'rtl' : 'ltr'; cx.textAlign = rtl ? 'right' : 'left'; cx.textBaseline = 'alphabetic';
    cx.fillStyle = 'rgb(' + col.join(',') + ')';
    if (vert) { cx.translate(angle > 0 ? th * 0.72 : th * 0.28, angle > 0 ? tw : 0); cx.rotate(angle > 0 ? -Math.PI / 2 : Math.PI / 2); cx.fillText(s, rtl ? tw - 2 : 2, 0); }
    else cx.fillText(s, rtl ? tw - 2 : 2, th * 0.72);
    const k = 0.3528 / S;
    return (cache[key] = { url: cv.toDataURL('image/png'), w: tw * k, h: th * k, alias: 'u' + Object.keys(cache).length + '_' + Math.random().toString(36).slice(2, 7) });
  };
  const line = function (s, x, y, o) {
    if (!AT.pdfNeedsRaster(s)) { origText(s, x, y, o); return; }
    const fs = pdf.getFontSize(), f = pdf.getFont(), bold = /bold/i.test((f && f.fontStyle) || '');
    const angle = o.angle ? Math.round(o.angle) : 0, im = raster(s, fs, bold, angle), fmm = fs * 0.3528;
    if (Math.abs(angle) === 90) { // vertical: centred on (x, y)
      pdf.addImage(im.url, 'PNG', x - im.h * (o.baseline === 'middle' ? 0.5 : 0.72), y - im.w / 2, im.h, im.w, im.alias);
      return;
    }
    let left = x;
    if (o.align === 'center') left = x - im.w / 2; else if (o.align === 'right') left = x - im.w;
    let top = y - im.h * 0.72;
    if (o.baseline === 'middle') top = y + fmm * 0.35 - im.h * 0.72;
    else if (o.baseline === 'top') top = y + fmm * 0.75 - im.h * 0.72;
    pdf.addImage(im.url, 'PNG', left, top, im.w, im.h, im.alias);
  };
  pdf.uniWidth = function (str) {
    const t = String(str);
    if (!AT.pdfNeedsRaster(t)) return pdf.getTextWidth(t);
    const f = pdf.getFont(); return raster(t, pdf.getFontSize(), /bold/i.test((f && f.fontStyle) || ''), 0).w - 0.3;
  };
  pdf.text = function (str, x, y, opts, transform) {
    const arr = Array.isArray(str) ? str : [str];
    if (!arr.some(AT.pdfNeedsRaster)) return origText(str, x, y, opts, transform);
    const o = opts || {}, lh = pdf.getFontSize() * 0.3528 * (pdf.getLineHeightFactor ? pdf.getLineHeightFactor() : 1.15);
    arr.forEach(function (s, i) { line(String(s), x, y + i * lh, o); });
    return pdf;
  };
  return pdf;
};


/* ---------- CAD dimension model (pure geometry, model mm) ----------
   Corners get letters A, B, C… in outline order. Every edge has its true length (along the curve for curved edges)
   and chord; every corner has its interior angle from the edge tangents. Ordinates are measured from the datum at the
   top-left of the net (sewing line) bounding box. */
AT.ptLetter = function (i) { let s = '', n = i; do { s = String.fromCharCode(65 + (n % 26)) + s; n = Math.floor(n / 26) - 1; } while (n >= 0); return s; };
AT.fmtN = function (mm, units) { // number only, CAD style (unit stated once in the title block)
  if (mm == null || isNaN(mm)) return '–';
  if (units === 'in') return (mm / 25.4).toFixed(2);
  if (units === 'mm') return String(Math.round(mm));
  return (mm / 10).toFixed(1);
};
AT.unitWord = function (units) { return units === 'in' ? 'inches' : units === 'mm' ? 'millimetres' : 'centimetres'; };
AT.dimModel = function (pc, ol) {
  const segs = ol.segs, n = segs.length, sgnA = AT.area(ol.poly) > 0 ? 1 : -1, bb = AT.bbox(ol.poly);
  const unit = function (x, y) { const L = Math.hypot(x, y) || 1; return { x: x / L, y: y / L }; };
  const tanOut = function (sg) { const c = sg.ctrl; for (let j = 1; j < c.length; j++) { const dx = c[j].x - c[0].x, dy = c[j].y - c[0].y; if (Math.hypot(dx, dy) > 1e-6) return unit(dx, dy); } return { x: 1, y: 0 }; };
  const tanIn = function (sg) { const c = sg.ctrl, m = c.length - 1; for (let j = m - 1; j >= 0; j--) { const dx = c[m].x - c[j].x, dy = c[m].y - c[j].y; if (Math.hypot(dx, dy) > 1e-6) return unit(dx, dy); } return { x: 1, y: 0 }; };
  const corners = segs.map(function (sg, k) {
    const prev = segs[(k - 1 + n) % n], din = tanIn(prev), dout = tanOut(sg);
    const turn = Math.atan2(din.x * dout.y - din.y * dout.x, din.x * dout.x + din.y * dout.y);
    const ang = 180 - turn * sgnA / AT.deg;
    const p = sg.ctrl[0];
    return { k: k, id: sg.id, L: AT.ptLetter(k), x: p.x, y: p.y, X: p.x - bb.x0, Y: p.y - bb.y0, ang: ang, din: din, dout: dout, smooth: Math.abs(ang - 180) < 1 };
  });
  const edges = segs.map(function (sg, k) {
    const a = sg.ctrl[0], b = sg.ctrl[sg.ctrl.length - 1];
    return { k: k, id: sg.id, from: corners[k].L, to: corners[(k + 1) % n].L, curve: sg.ctrl.length > 2, len: sg.len, chord: Math.hypot(b.x - a.x, b.y - a.y), allow: AT.edgeAllowance(pc, sg.id), hem: !!(pc.hem && pc.hem[sg.id] > 0) };
  });
  // mirror symmetry about the vertical centre of the bounding box
  const cx = bb.cx, tol = Math.max(0.6, bb.w * 0.002);
  const sym = n >= 4 && corners.every(function (c) { return corners.some(function (d) { return Math.abs(d.x - (2 * cx - c.x)) < tol && Math.abs(d.y - c.y) < tol; }); });
  return { corners: corners, edges: edges, bb: bb, sgnA: sgnA, sym: sym, cx: cx };
};
// does a ray from p in direction (dx, dy) cross the polygon (ignoring the first `skip` mm)?
AT.rayCrossings = function (poly, p, dx, dy, skip) {
  const ox = p.x + dx * skip, oy = p.y + dy * skip; let c = 0;
  for (let i = 0, m = poly.length; i < m; i++) {
    const a = poly[i], b = poly[(i + 1) % m], ex = b.x - a.x, ey = b.y - a.y, den = dx * ey - dy * ex;
    if (Math.abs(den) < 1e-12) continue;
    const t = ((a.x - ox) * ey - (a.y - oy) * ex) / den, u = ((a.x - ox) * dy - (a.y - oy) * dx) / den;
    if (t > 0 && u >= 0 && u < 1) c++;
  }
  return c;
};
// rows of the dimension table for one item (shared by the planner and the renderer)
AT.dimTableRows = function (doc, it, nums) {
  const pc = it.pc, ol = it.g.ol, dm = AT.dimModel(pc, ol), cov = AT.edgeCoverage(doc, pc, ol, nums);
  const rows = dm.corners.map(function (c, k) {
    const e = dm.edges[k], P0 = ol.start[k], P1 = P0 + e.len;
    const seams = cov.sides.filter(function (sd) { const a = Math.max(sd.P0, P0), b = Math.min(sd.P1, P1), a2 = Math.max(sd.P0 - ol.total, P0), b2 = Math.min(sd.P1 - ol.total, P1); return b - a > 0.5 || b2 - a2 > 0.5; }).map(function (sd) { return sd.n; });
    return { c: c, e: e, seams: seams.filter(function (v, i, a) { return a.indexOf(v) === i; }).sort(function (a, b) { return a - b; }) };
  });
  const notches = AT.asArray(pc.notches).map(function (nt) { const k = ol.byId[nt.s]; if (k == null) return null; const sg = ol.segs[k]; return { k: k, from: dm.corners[k].L, to: dm.corners[(k + 1) % dm.corners.length].L, a: sg.len * (nt.t || 0), b: sg.len * (1 - (nt.t || 0)) }; }).filter(Boolean);
  return { dm: dm, rows: rows, notches: notches };
};
// a roomy point inside a piece for its label: far from every edge, leaning to the centre
AT.labelPole = function (ol) {
  const bb = AT.bbox(ol.poly), P = ol.poly, step = Math.max(1, Math.floor(P.length / 400)), Q = [];
  for (let i = 0; i < P.length; i += step) Q.push(P[i]);
  let best = { x: bb.cx, y: bb.cy, s: -Infinity, d: 0 };
  const N = 28, cands = [];
  for (let i = 1; i < N; i++) for (let j = 1; j < N; j++) {
    const x = bb.x0 + bb.w * i / N, y = bb.y0 + bb.h * j / N;
    if (!AT.inside(P, x, y)) continue;
    let d = Infinity;
    for (let m = 0; m < Q.length; m++) { const a = Q[m], b = Q[(m + 1) % Q.length], r = AT.distSeg(x, y, a.x, a.y, b.x, b.y).d; if (r < d) d = r; }
    const s = Math.min(d, Math.min(bb.w, bb.h) * 0.3) - 0.12 * Math.hypot(x - bb.cx, (y - bb.cy) * 0.5);
    cands.push({ x: x, y: y, s: s });
    if (s > best.s) best = { x: x, y: y, s: s, d: d };
  }
  best.cands = cands.sort(function (a, b) { return b.s - a.s; }).slice(0, 120);
  return best;
};
AT.DIM_TABLE_ROW = 4.3;
AT.dimTablePages = function (doc, items, P) { // two-column flow on landscape sheets; returns [[{it, start}], …]
  const nums = AT.seamNumbers(doc), H = P[0] - 32 - 16, pages = [];
  let page = [[], []], col = 0, y = 0;
  items.forEach(function (it) {
    const t = AT.dimTableRows(doc, it, nums), need = (3 + t.rows.length + (t.notches.length ? t.notches.length + 1 : 0)) * AT.DIM_TABLE_ROW + 4;
    if (y + need > H && y > 0) { col++; y = 0; if (col > 1) { pages.push(page); page = [[], []]; col = 0; } }
    page[col].push({ it: it, t: t, y: y }); y += need;
  });
  if (page[0].length) pages.push(page);
  return pages;
};

/* ---------- pattern PDF ---------- */
AT.PAPER = { a4: [210, 297, 'A4'], a3: [297, 420, 'A3'], letter: [215.9, 279.4, 'Letter'], tabloid: [279.4, 431.8, 'Tabloid'] };
AT.PLOTTER = [[914, '36″ · 914 mm'], [1118, '44″ · 1118 mm'], [1524, '60″ · 1524 mm']];
AT.ppdfItems = function (doc, o) {
  let pcs = AT.asArray(doc.pieces).filter(function (pc) { return pc && pc.pts && pc.pts.length > 2; });
  if (o.ids && o.ids.length) pcs = pcs.filter(function (pc) { return o.ids.indexOf(pc.id) >= 0; });
  else pcs = pcs.filter(function (pc) { return pc.state !== 'off'; });
  pcs.sort(function (a, b) { return (a.order || 0) - (b.order || 0); });
  return pcs.map(function (pc) {
    const g = AT.pieceGeom(doc, pc, doc.activeSize);
    const nest = o.sizes === 'all' && doc.sizes.length > 1 ? doc.sizes.map(function (sz, i) { return { sz: sz, i: i, poly: AT.outline(AT.gradedPts(pc, sz, doc.baseSize)).poly }; }) : null;
    let nb = g.cb;
    if (nest) { const all = g.cut.slice(); nest.forEach(function (n) { n.poly.forEach(function (p) { all.push(p); }); }); nb = AT.bbox(all); }
    return { pc: pc, g: g, nest: nest, box: nb };
  });
};
// shelf packing of item boxes (+ room for dimension lines) into a given width; returns placements in mm
AT.DIM_ROOM = [24, 23, 17, 17];
AT.DIM_FULL = 0.07; // below this scale a piece gets overall dimensions only // paper mm left, top, right, bottom around a fully dimensioned piece
AT.ppdfPack = function (items, width, gap, dimRoom, room) {
  const R = room ? (room.length === 4 ? room : [room[0], room[1], 0, 0]) : dimRoom ? AT.DIM_ROOM : [0, 0, 0, 0];
  const L = R[0], T = R[1], Rr = R[2], B = R[3];
  const order = items.map(function (it, i) { return i; }).sort(function (a, b) { return items[b].box.h - items[a].box.h; });
  let x = 0, y = 0, rowH = 0, maxW = 0;
  const pos = [], rows = [{ y: 0, h: 0, idx: [] }];
  order.forEach(function (i) {
    const it = items[i], w = it.box.w + L + Rr, h = it.box.h + T + B;
    if (x > 0 && x + w > width) { y += rowH + gap; x = 0; rowH = 0; rows.push({ y: y, h: 0, idx: [] }); }
    pos[i] = { x: x + L, y: y + T, w: w, h: h };
    const row = rows[rows.length - 1]; row.idx.push(i);
    x += w + gap; rowH = Math.max(rowH, h); row.h = rowH; maxW = Math.max(maxW, x - gap);
  });
  return { pos: pos, w: maxW, h: y + rowH, rows: rows, room: R };
};
// piece-per-page fit: drawing area between the header and the title block, with dimension room on every side
AT.ppdfPieceFit = function (it, P, dims) {
  const land = it.box.w > it.box.h, pw = land ? P[1] : P[0], ph = land ? P[0] : P[1];
  const R = dims ? AT.DIM_ROOM : [8, 8, 8, 8], aw = pw - 24 - R[0] - R[2], ah = ph - 27 - 46 - R[1] - R[3];
  const k = Math.min(1, aw / it.box.w, ah / it.box.h);
  return { land: land, pw: pw, ph: ph, k: k, R: R, aw: aw, ah: ah };
};
AT.ppdfPlan = function (doc, o) {
  const items = AT.ppdfItems(doc, o);
  const P = AT.PAPER[o.paper] || AT.PAPER.a4, M = 10;
  const plan = { items: items, pages: 0, note: '' };
  const nSeams = Object.values(doc.sewing || {}).filter(Boolean).length, sewPages = nSeams ? Math.ceil(nSeams / 22) : 0;
  plan.sewPages = sewPages;
  if (!items.length) { plan.note = 'No pieces to export.'; return plan; }
  const dimPages = o.dims && o.layout !== 'plotter' ? AT.dimTablePages(doc, items, P).length : 0;
  plan.dimPages = dimPages;
  if (o.layout === 'pieces') {
    let ones = 0;
    items.forEach(function (it) { const f = AT.ppdfPieceFit(it, P, o.dims); it.fit = f; it.k = f.k; it.land = f.land; if (f.k >= 0.999) ones++; });
    plan.pages = items.length + 1 + dimPages + sewPages;
    plan.note = 'One CAD-style sheet per piece' + (o.dims ? ', fully dimensioned (edge lengths, ordinates from a datum, angles, notch positions)' : '') + (ones ? ' (' + ones + ' at full scale)' : '') + ', with a title block and scale bar, then a piece schedule' + (o.dims ? ', dimension tables' : '') + ' and a sewing schedule.';
  } else if (o.layout === 'overview') {
    plan.pages = 2 + dimPages + sewPages; plan.note = 'All pieces on one ' + P[2] + ' sheet at a common scale, then a piece schedule' + (o.dims ? ', dimension tables' : '') + ' and a sewing schedule.';
  } else if (o.layout === 'tiled') {
    const tw = P[0] - 2 * M, th = P[1] - 2 * M;
    const RM = o.dims ? AT.DIM_ROOM : [0, 0, 0, 0];
    const maxW = Math.max.apply(null, items.map(function (it) { return it.box.w + RM[0] + RM[2]; }));
    const area = items.reduce(function (s, it) { return s + (it.box.w + 15 + RM[0] + RM[2]) * (it.box.h + 15 + RM[1] + RM[3]); }, 0);
    const cols = Math.max(Math.ceil(maxW / tw), Math.round(Math.sqrt(area) / tw), 1);
    const pk = AT.ppdfPack(items, cols * tw, 15, o.dims);
    const rows = Math.max(1, Math.ceil(pk.h / th));
    const used = [];
    for (let r = 0; r < rows; r++) for (let c = 0; c < cols; c++) {
      const x0 = c * tw, y0 = r * th;
      const hit = items.some(function (it, i) { const p = pk.pos[i]; return p.x - RM[0] < x0 + tw && p.x + it.box.w + RM[2] > x0 && p.y - RM[1] < y0 + th && p.y + it.box.h + RM[3] > y0; });
      if (hit) used.push([r, c]);
    }
    Object.assign(plan, { pk: pk, cols: cols, rows: rows, tw: tw, th: th, used: used, pages: used.length + 1 + dimPages + sewPages });
    plan.note = 'Full scale on ' + used.length + ' ' + P[2] + ' tiles (' + rows + ' × ' + cols + ' grid, blank tiles skipped) plus a cover with the assembly map and a test square' + (o.dims ? ', dimension tables' : '') + ' and a sewing schedule. Print at 100 % / actual size.';
  } else {
    const roll = o.plotter || 914, w = roll - 2 * M;
    const RM = o.dims ? AT.DIM_ROOM : [0, 0, 0, 0];
    const maxW = Math.max.apply(null, items.map(function (it) { return it.box.w + RM[0] + RM[2]; }));
    const pk = AT.ppdfPack(items, Math.max(w, maxW), 20, o.dims);
    const maxH = 5000 - 2 * M - 30, sheets = [];
    let cur = { y0: 0, h: 0, idx: [] };
    pk.rows.forEach(function (row) {
      if (cur.idx.length && row.y + row.h - cur.y0 > maxH) { sheets.push(cur); cur = { y0: row.y, h: 0, idx: [] }; }
      cur.idx = cur.idx.concat(row.idx); cur.h = row.y + row.h - cur.y0;
    });
    sheets.push(cur);
    Object.assign(plan, { pk: pk, roll: Math.max(roll, maxW + 2 * M), sheets: sheets, pages: sheets.length });
    plan.note = sheets.length === 1 ? 'One full-scale sheet, ' + Math.round(plan.roll) + ' × ' + Math.round(sheets[0].h + 2 * M + 30) + ' mm, for a plotter roll.'
      : sheets.length + ' full-scale sheets ' + Math.round(plan.roll) + ' mm wide (' + sheets.map(function (sh) { return (Math.round((sh.h + 2 * M + 30) / 10) / 100).toFixed(2) + ' m'; }).join(' + ') + '); PDF pages are limited to 5 m.';
  }
  return plan;
};
AT.patternPDF = function (doc, o, meta) {
  const J = window.jspdf && window.jspdf.jsPDF; if (!J) throw new Error('PDF library not loaded');
  meta = meta || {};
  const U = o.units || 'cm', P = AT.PAPER[o.paper] || AT.PAPER.a4;
  const plan = AT.ppdfPlan(doc, o), items = plan.items;
  if (!items.length) throw new Error('No pieces to export');
  const INK = [34, 34, 38], MUTE = [128, 128, 136], DIM = [70, 82, 104], NEST = AT.SIZE_NEST_COLORS || ['#3B82F6', '#10B981', '#111111', '#F59E0B', '#EF4444'];
  const hex = function (h) { const n = parseInt(String(h).slice(1), 16); return [(n >> 16) & 255, (n >> 8) & 255, n & 255]; };
  let pdf = null, pageNo = 0;
  const total = plan.pages;
  const newPage = function (w, h) {
    const land = w > h, fmt = [Math.min(w, h), Math.max(w, h)];
    if (!pdf) { pdf = new J({ orientation: land ? 'landscape' : 'portrait', unit: 'mm', format: fmt, compress: true }); AT.pdfInstallUnicode(pdf); }
    else pdf.addPage(fmt, land ? 'landscape' : 'portrait');
    pageNo++;
    return { w: w, h: h };
  };
  const header = function (pg, title) {
    const M = 12;
    pdf.setTextColor.apply(pdf, INK); pdf.setFont('helvetica', 'bold'); pdf.setFontSize(8.5); pdf.text('ATELIER · PATTERN', M, 10);
    pdf.setFont('helvetica', 'normal'); pdf.setTextColor.apply(pdf, MUTE);
    pdf.text(doc.meta.name, M + 38, 10);
    pdf.text('·   size ' + doc.activeSize + (o.sizes === 'all' && doc.sizes.length > 1 ? ' (all sizes nested)' : '') + '   ·   ' + new Date().toLocaleDateString(), M + 38 + pdf.uniWidth(doc.meta.name) + 3, 10);
    pdf.text(pageNo + ' / ' + total, pg.w - M, 10, { align: 'right' });
    pdf.setDrawColor(220, 218, 214); pdf.setLineWidth(0.2); pdf.line(M, 13, pg.w - M, 13);
    if (title) { pdf.setTextColor.apply(pdf, INK); pdf.setFont('helvetica', 'bold'); pdf.setFontSize(14); pdf.text(title, M, 22); }
  };
  const poly = function (pts, T, closed) {
    if (!pts || pts.length < 2) return;
    const q = pts.map(function (p) { return [T.ox + (p.x - T.bx) * T.k, T.oy + (p.y - T.by) * T.k]; });
    const rel = []; for (let i = 1; i < q.length; i++) rel.push([q[i][0] - q[i - 1][0], q[i][1] - q[i - 1][1]]);
    pdf.lines(rel, q[0][0], q[0][1], [1, 1], 'S', !!closed);
  };
  const map = function (T, x, y) { return [T.ox + (x - T.bx) * T.k, T.oy + (y - T.by) * T.k]; };
  const arrow = function (x, y, dx, dy, L) { const n = Math.hypot(dx, dy) || 1; dx /= n; dy /= n; const w = L * 0.36; pdf.triangle(x, y, x - dx * L - dy * w, y - dy * L + dx * w, x - dx * L + dy * w, y - dy * L - dx * w, 'F'); };
  const label = function (txt, x, y, size, opts) { pdf.setFontSize(size); const w = AT.pdfNeedsRaster(txt) ? txt.length * size * 0.2 : pdf.getTextWidth(txt); pdf.setFillColor(255, 255, 255); if (!(opts && opts.angle)) pdf.rect(x - w / 2 - 0.8, y - size * 0.22, w + 1.6, size * 0.44, 'F'); else pdf.rect(x - size * 0.22, y - w / 2 - 0.8, size * 0.44, w + 1.6, 'F'); pdf.text(txt, x, y, Object.assign({ align: 'center', baseline: 'middle' }, opts || {})); };
  // one pattern piece at transform T (mm on page = o + (x - b) * k)
  const nums = AT.seamNumbers(doc);
  const SA_FILL = [236, 233, 228], HEM_FILL = [222, 216, 206];
  const fillPoly = function (pts, T, col) { if (!pts || pts.length < 3) return; pdf.setFillColor.apply(pdf, col); const q = pts.map(function (p) { return map(T, p.x, p.y); }); const rel = []; for (let i = 1; i < q.length; i++) rel.push([q[i][0] - q[i - 1][0], q[i][1] - q[i - 1][1]]); pdf.lines(rel, q[0][0], q[0][1], [1, 1], 'F', true); };
  const drawPiece = function (it, T) {
    const pc = it.pc, g = it.g, k = T.k, fsz = Math.max(7.5, Math.min(12, 9 * Math.sqrt(k) + 3));
    const sgn = AT.area(g.ol.poly) > 0 ? 1 : -1, cov = AT.edgeCoverage(doc, pc, g.ol, nums), hem = pc.hem || {};
    const showSA = o.sa && g.hasSA, fullDims = o.dims && k >= AT.DIM_FULL;
    OCC = [];
    const outAt = function (P, d) { const q = AT.perimPoint(g.ol, P); return { x: q.x + q.ty * sgn * d, y: q.y - q.tx * sgn * d, tx: q.tx, ty: q.ty }; };
    const edgeText = function (P, d, txt, measureOnly) { // d in model mm, + outward / - inward
      const q = outAt(P, d), pq = map(T, q.x, q.y), sx = Math.sign(d) * q.ty * sgn, sy = -Math.sign(d) * q.tx * sgn;
      const o2 = Math.abs(sx) > 0.45 ? { align: sx > 0 ? 'left' : 'right', baseline: 'middle' } : { align: 'center', baseline: sy > 0 ? 'top' : 'middle' };
      const ty = pq[1] + (o2.baseline === 'middle' && Math.abs(sx) <= 0.45 ? -0.8 : 0), tw = pdf.uniWidth(txt), th = pdf.getFontSize() * 0.3528;
      const x0 = o2.align === 'left' ? pq[0] : o2.align === 'right' ? pq[0] - tw : pq[0] - tw / 2, y0 = o2.baseline === 'top' ? ty : ty - th * 0.5;
      const box = [x0 - 0.3, y0 - 0.3, x0 + tw + 0.3, y0 + th + 0.3];
      if (measureOnly) return box;
      pdf.text(txt, pq[0], ty, o2);
      occAdd(x0, y0, x0 + tw, y0 + th);
      return box;
    };
    // inside labels of a fully dimensioned piece are placed last, where they collide least
    const deferred = [];
    const deferText = function (P0, P1, d, txt, font, col, size) { deferred.push({ P0: P0, P1: P1, d: d, txt: txt, font: font, col: col, size: size }); };
    // allowance bands: seam allowance tint, hem allowance darker
    if (showSA) {
      fillPoly(g.cut, T, SA_FILL);
      g.ol.segs.forEach(function (sg) {
        const h = hem[sg.id]; if (!(h > 0)) return;
        const P = sg.poly, outer = P.map(function (pt, i) { const q = P[Math.min(i + 1, P.length - 1)], r = P[Math.max(i - 1, 0)], dx = q.x - r.x, dy = q.y - r.y, L = Math.hypot(dx, dy) || 1; return { x: pt.x + dy / L * sgn * h, y: pt.y - dx / L * sgn * h }; });
        fillPoly(P.concat(outer.reverse()), T, HEM_FILL);
      });
      fillPoly(g.ol.poly, T, [255, 255, 255]);
    }
    // nested sizes
    if (it.nest) it.nest.forEach(function (n) { if (n.sz === doc.activeSize) return; pdf.setDrawColor.apply(pdf, hex(NEST[n.i % NEST.length])); pdf.setLineWidth(0.18); pdf.setLineDashPattern([], 0); poly(n.poly, T, true); });
    // cut line
    pdf.setDrawColor.apply(pdf, INK); pdf.setLineDashPattern([], 0); pdf.setLineWidth(0.4);
    poly(showSA ? g.cut : g.ol.poly, T, true);
    // sewing line: sewn parts = stitching line (dashed); free parts = finished edge (solid) or hem fold (dash-dot)
    if (showSA) {
      pdf.setLineWidth(0.22); pdf.setLineDashPattern([1.6, 1.1], 0);
      cov.sewn.forEach(function (m) { poly(AT.olPath(g.ol, m[0], m[1]), T, false); });
    }
    cov.freeParts.forEach(function (f) {
      const isHem = hem[f.sid] > 0;
      pdf.setLineDashPattern(isHem ? [3.2, 1, 0.5, 1] : [], 0); pdf.setLineWidth(isHem ? 0.3 : (showSA ? 0.3 : 0.5)); pdf.setDrawColor.apply(pdf, INK);
      if (showSA || isHem) poly(AT.olPath(g.ol, f.P0, f.P1), T, false);
    });
    pdf.setLineDashPattern([], 0);
    // internal lines
    AT.asArray(pc.internals).forEach(function (il) {
      const pl = AT.evalOpen(il.closed ? il.pts.concat([il.pts[0]]) : il.pts);
      pdf.setDrawColor(90, 90, 98); pdf.setLineWidth(0.2); pdf.setLineDashPattern(il.fold != null && il.fold !== '' && il.fold != 180 ? [2.4, 1, 0.4, 1] : [1.2, 0.9], 0); poly(pl, T, false); pdf.setLineDashPattern([], 0);
    });
    // notches: from the sewing line out through the allowance
    AT.asArray(pc.notches).forEach(function (n) {
      const kk = g.ol.byId[n.s]; if (kk == null) return;
      const pt = AT.segPoint(g.ol.segs[kk], n.t || 0), ox = pt.ty * sgn, oy = -pt.tx * sgn;
      const sa = showSA ? AT.edgeAllowance(pc, n.s) : 0, Ln = sa > 0 ? sa : -Math.max(4, 1.5 / Math.max(k, 0.05));
      const a = map(T, pt.x, pt.y), b = map(T, pt.x + ox * Ln, pt.y + oy * Ln);
      pdf.setDrawColor.apply(pdf, INK); pdf.setLineWidth(0.3); pdf.line(a[0], a[1], b[0], b[1]);
    });
    const small = Math.max(5.2, Math.min(7, fsz * 0.6)), inset = 3.2 / k;
    // "free edge" labels (free parts without a hem)
    cov.freeParts.forEach(function (f) {
      if (hem[f.sid] > 0 || (f.P1 - f.P0) * k < 16) return;
      pdf.setFont('helvetica', 'italic'); pdf.setTextColor.apply(pdf, MUTE); pdf.setFontSize(small); edgeText((f.P0 + f.P1) / 2, -1.6 / k, 'free edge');
    });
    // allowance labels: one per run of consecutive edges with the same finish and width
    const runs = [], nSeg = g.ol.segs.length;
    g.ol.segs.forEach(function (sg, kk) {
      const isH = hem[sg.id] > 0, v = AT.edgeAllowance(pc, sg.id), last = runs[runs.length - 1];
      if (last && last.hem === isH && last.v === v) { last.P1 = g.ol.start[kk] + sg.len; }
      else runs.push({ hem: isH, v: v, P0: g.ol.start[kk], P1: g.ol.start[kk] + sg.len });
    });
    if (runs.length > 1 && runs[0].hem === runs[runs.length - 1].hem && runs[0].v === runs[runs.length - 1].v) { const l = runs.pop(); runs[0].P0 = l.P0 - g.ol.total; }
    if (nSeg) runs.forEach(function (rn) {
      const lenPg = (rn.P1 - rn.P0) * k, mid = (rn.P0 + rn.P1) / 2;
      if (rn.hem) {
        if (lenPg < 10) return;
        const h = rn.v, q0 = AT.perimPoint(g.ol, mid), vertical = Math.abs(q0.tx) < 0.6, inBand = showSA && h * k >= 3.2;
        pdf.setFont('helvetica', 'bold'); pdf.setTextColor.apply(pdf, INK); pdf.setFontSize(small);
        if (inBand) { const q = outAt(mid, h / 2), pq = map(T, q.x, q.y); pdf.text('HEM ' + AT.fmtU(h, U), pq[0], pq[1], vertical ? { align: 'center', baseline: 'middle', angle: 90 } : { align: 'center', baseline: 'middle' }); }
        else if (fullDims) { deferText(rn.P0, rn.P1, -1.4 / k, 'HEM ' + AT.fmtU(h, U) + ' (fold)', 'bold', INK, small); return; }
        else edgeText(mid, (showSA ? h : 0) + 1.6 / k, 'HEM ' + AT.fmtU(h, U));
        pdf.setFont('helvetica', 'italic'); pdf.setTextColor.apply(pdf, MUTE); pdf.setFontSize(small * 0.9); edgeText(mid + (vertical ? 0 : 0), -1.4 / k, 'hem fold');
      } else if (showSA && k >= 0.09 && rn.v > 0 && lenPg >= 12) {
        pdf.setFont('helvetica', 'normal'); pdf.setFontSize(small); pdf.setTextColor.apply(pdf, DIM);
        if (fullDims) deferText(rn.P0, rn.P1, -1.5 / k, 'SA ' + AT.fmtU(rn.v, U), 'italic', MUTE, small);
        else edgeText(rn.P0 + (rn.P1 - rn.P0) * 0.62, rn.v + 1.6 / k, 'SA ' + AT.fmtU(rn.v, U));
      }
    });
    // seam numbers on every sewn side (same number on both pieces)
    const rb = k >= 0.25 ? 2.4 : 1.9;
    cov.sides.forEach(function (sd) {
      const mid = (sd.P0 + sd.P1) / 2, q = outAt(mid, -(rb + 1.6) / k), pq = map(T, q.x, q.y);
      pdf.setFillColor.apply(pdf, hex(sd.sw.color || '#E8654A')); pdf.circle(pq[0], pq[1], rb, 'F'); occAdd(pq[0] - rb - 0.3, pq[1] - rb - 0.3, pq[0] + rb + 0.3, pq[1] + rb + 0.3);
      pdf.setFont('helvetica', 'bold'); pdf.setFontSize(rb >= 2.4 ? 7.5 : 6.2); pdf.setTextColor(255, 255, 255);
      pdf.text(String(sd.n), pq[0], pq[1] + 0.05, { align: 'center', baseline: 'middle' });
      if (o.layout === 'pieces' && (sd.P1 - sd.P0) * k > 26) {
        const names = sd.partners.filter(function (v, i, arr) { return arr.indexOf(v) === i; }).map(function (pid) { return pid === pc.id ? 'same piece' : (doc.pieces[pid] || {}).name || '?'; });
        pdf.setFont('helvetica', 'normal'); pdf.setFontSize(5.6); pdf.setTextColor.apply(pdf, MUTE);
        edgeText(mid, -(2 * rb + 2.6) / k, names.join(' + '));
      }
    });
    // grain line
    const bb = g.bb, gr = (pc.grain || 0) * AT.deg, pole = AT.labelPole(g.ol);
    // piece label
    const fab = doc.fabrics[pc.fabric];
    pdf.setTextColor.apply(pdf, INK); pdf.setFont('helvetica', 'bold'); pdf.setFontSize(fsz);
    const labW = Math.max(pdf.uniWidth(pc.name), 18) + 1, labBox = function (cx, cy) { return [cx + 2.5, cy - fsz * 0.25 - fsz * 0.3528 * 0.8, cx + 3 + labW, cy + fsz * 0.25 + 3 * fsz * 0.34]; };
    let c = map(T, pole.x, pole.y);
    if (occHit(labBox(c[0], c[1])) > 0) { // look for a roomier spot that stays inside the piece
      let bestA = occHit(labBox(c[0], c[1]));
      (pole.cands || []).forEach(function (cd) {
        const q = map(T, cd.x, cd.y), b = labBox(q[0], q[1]), inv = function (x, y) { return AT.inside(g.ol.poly, (x - T.ox) / k + T.bx, (y - T.oy) / k + T.by); };
        if (!(inv(b[0], b[1]) && inv(b[2], b[1]) && inv(b[0], b[3]) && inv(b[2], b[3]))) return;
        const a = occHit(b); if (a < bestA - 1e-6) { bestA = a; c = q; }
      });
    }
    let gl = bb.h * 0.32, gcx = (c[0] - T.ox) / k + T.bx, gcy = bb.cy;
    for (let tries = 0; tries < 12; tries++) { // keep the grain line inside the piece
      if (AT.inside(g.ol.poly, gcx + Math.sin(gr) * gl, gcy - Math.cos(gr) * gl) && AT.inside(g.ol.poly, gcx - Math.sin(gr) * gl, gcy + Math.cos(gr) * gl)) break;
      if (tries === 5) gcy = pole.y; gl *= 0.85;
    }
    const ga = map(T, gcx + Math.sin(gr) * gl, gcy - Math.cos(gr) * gl), gb = map(T, gcx - Math.sin(gr) * gl, gcy + Math.cos(gr) * gl);
    occAdd(Math.min(ga[0], gb[0]) - 1.2, Math.min(ga[1], gb[1]), Math.max(ga[0], gb[0]) + 1.2, Math.max(ga[1], gb[1]));
    pdf.setDrawColor.apply(pdf, INK); pdf.setFillColor.apply(pdf, INK); pdf.setLineWidth(0.25); pdf.line(ga[0], ga[1], gb[0], gb[1]);
    arrow(ga[0], ga[1], ga[0] - gb[0], ga[1] - gb[1], 2.4); arrow(gb[0], gb[1], gb[0] - ga[0], gb[1] - ga[1], 2.4);
    const lx = c[0] + 3;
    occAdd.apply(null, labBox(c[0], c[1]));
    pdf.text(pc.name, lx, c[1] - fsz * 0.25, { align: 'left' });
    pdf.setFont('helvetica', 'normal'); pdf.setFontSize(fsz * 0.72); pdf.setTextColor.apply(pdf, MUTE);
    [('Size ' + doc.activeSize), 'Cut ' + (pc.qty || 1) + (pc.mirror ? ' (mirrored)' : ''), fab ? fab.name : ''].filter(Boolean).forEach(function (t, i) { pdf.text(t, lx, c[1] + fsz * 0.25 + i * fsz * 0.34, { align: 'left' }); });
    // segment lengths (superseded by the edge dimensions when the piece is fully dimensioned)
    if (o.lengths && !(o.dims && k >= AT.DIM_FULL)) {
      pdf.setTextColor.apply(pdf, DIM);
      g.ol.segs.forEach(function (sg) {
        if (sg.len < 8 / Math.max(k, 0.05)) return;
        const m = AT.segPoint(sg, 0.5), off = (showSA ? AT.edgeAllowance(pc, sg.id) : 0) + 7 / k;
        const q = map(T, m.x + m.ty * sgn * off, m.y - m.tx * sgn * off);
        label(AT.fmtU(sg.len, U), q[0], q[1], Math.max(5, fsz * 0.62));
      });
    }
    if (o.dims) drawDims(it, T, showSA);
    deferred.forEach(function (dfr) {
      pdf.setFont('helvetica', dfr.font); pdf.setTextColor.apply(pdf, dfr.col); pdf.setFontSize(dfr.size);
      let best = null, bestD = dfr.d, bestA = Infinity;
      [1, 2.6, 4.2].forEach(function (dm2) { [0.5, 0.3, 0.7, 0.18, 0.82, 0.4, 0.6].forEach(function (f) { if (bestA === 0) return; const P = dfr.P0 + (dfr.P1 - dfr.P0) * f, a = occHit(edgeText(P, dfr.d * dm2, dfr.txt, true)) + (dm2 - 1) * 0.01; if (a < bestA - 1e-6) { bestA = a < 0.02 ? 0 : a; best = P; bestD = dfr.d * dm2; } }); });
      edgeText(best, bestD, dfr.txt);
    });
  };
  /* ---------- CAD dimensioning ----------
     full (scale ≥ 1:14): every edge with an aligned (straight) or arc-length (curved) dimension, notch chains,
     ordinate dimensions to every corner from the datum, overall size, corner letters, angles, right-angle and
     centre-line constraint marks. basic (smaller scales): overall size and corner letters. */
  let OCC = []; // paper-space boxes already used on the current piece
  const occAdd = function (x0, y0, x1, y1) { OCC.push([Math.min(x0, x1), Math.min(y0, y1), Math.max(x0, x1), Math.max(y0, y1)]); };
  const occHit = function (b) { let a = 0; OCC.forEach(function (r) { const w = Math.min(r[2], b[2]) - Math.max(r[0], b[0]), h = Math.min(r[3], b[3]) - Math.max(r[1], b[1]); if (w > 0 && h > 0) a += w * h; }); return a; };
  const rot = function (v, a) { const c = Math.cos(a), s = Math.sin(a); return { x: v.x * c - v.y * s, y: v.x * s + v.y * c }; };
  const plines = function (q, closed) { if (q.length < 2) return; const rel = []; for (let i = 1; i < q.length; i++) rel.push([q[i][0] - q[i - 1][0], q[i][1] - q[i - 1][1]]); pdf.lines(rel, q[0][0], q[0][1], [1, 1], 'S', !!closed); };
  // text centred on (cx, cy) reading along (ux, uy) (flipped so it never reads upside down)
  const rtext = function (txt, cx, cy, ux, uy, size) {
    if (ux < -1e-6 || (Math.abs(ux) <= 1e-6 && uy > 0)) { ux = -ux; uy = -uy; }
    pdf.setFontSize(size);
    const w = pdf.getTextWidth(txt), h = size * 0.3528, upx = uy, upy = -ux;
    const bx = cx - ux * w / 2 - upx * h * 0.36, by = cy - uy * w / 2 - upy * h * 0.36, ang = Math.atan2(-uy, ux) / AT.deg;
    if (Math.abs(ang) < 0.05) pdf.text(txt, bx, by); else pdf.text(txt, bx, by, { angle: ang });
    const hx = Math.abs(ux) * w / 2 + Math.abs(upx) * h * 0.45, hy = Math.abs(uy) * w / 2 + Math.abs(upy) * h * 0.45; occAdd(cx - hx, cy - hy, cx + hx, cy + hy);
    return { w: w, h: h, ux: ux, uy: uy, upx: upx, upy: upy };
  };
  const arcSym = function (cx, cy, r) { // arc-length symbol over a dimension value
    const hw = Math.min(r.w / 2, 1.5), b0x = cx + r.upx * (r.h * 0.36 + 0.45), b0y = cy + r.upy * (r.h * 0.36 + 0.45), q = [];
    for (let i = 0; i <= 8; i++) { const s = -1 + i / 4; q.push([b0x + r.ux * s * hw + r.upx * (1 - s * s) * 0.65, b0y + r.uy * s * hw + r.upy * (1 - s * s) * 0.65]); }
    pdf.setLineWidth(0.12); plines(q, false);
  };
  const spread = function (want, gap) { // non-overlapping label positions, clusters centred on their targets
    const cl = want.map(function (v, i) { return { s: v, n: 1, sum: v, idx: [i] }; });
    let moved = true, guard = 0;
    while (moved && guard++ < 200) {
      moved = false;
      for (let i = 0; i < cl.length - 1; i++) {
        const A = cl[i], B = cl[i + 1];
        if (A.s + A.n * gap > B.s + 1e-6) {
          const n = A.n + B.n, sum = A.sum + B.sum, idx = A.idx.concat(B.idx);
          const mean = idx.reduce(function (t, j, jj) { return t + want[j] - jj * gap; }, 0) / n;
          cl.splice(i, 2, { s: mean, n: n, sum: sum, idx: idx }); moved = true; break;
        }
      }
    }
    const out = []; cl.forEach(function (c) { c.idx.forEach(function (j, jj) { out[j] = c.s + jj * gap; }); });
    return out;
  };
  const DFS = 5.6;
  const dimInk = function () { pdf.setDrawColor.apply(pdf, DIM); pdf.setFillColor.apply(pdf, DIM); pdf.setTextColor.apply(pdf, DIM); pdf.setLineDashPattern([], 0); pdf.setFont('helvetica', 'normal'); };
  const drawDims = function (it, T, showSA) {
    const pc = it.pc, g = it.g, k = T.k, ol = g.ol, dm = AT.dimModel(pc, ol), sgn = dm.sgnA, bb = dm.bb;
    const full = k >= AT.DIM_FULL, Pm = function (x, y) { return map(T, x, y); };
    const cb = AT.bbox(showSA ? g.cut : ol.poly), X0 = Pm(cb.x0, 0)[0], X1 = Pm(cb.x1, 0)[0], Y0 = Pm(0, cb.y0)[1], Y1 = Pm(0, cb.y1)[1];
    const pts = ol.poly; let pl = pts[0], pr = pts[0], pt = pts[0], pb = pts[0];
    pts.forEach(function (q) { if (q.x < pl.x) pl = q; if (q.x > pr.x) pr = q; if (q.y < pt.y) pt = q; if (q.y > pb.y) pb = q; });
    // corner letters (both levels), angles and right-angle marks (full)
    const cornerMarks = function () {
      dm.corners.forEach(function (c) {
        const p = Pm(c.x, c.y), v1 = { x: -c.din.x, y: -c.din.y }, v2 = c.dout, ia = c.ang * AT.deg;
        const s = (function () { const a = rot(v1, ia), b2 = rot(v1, -ia); return a.x * v2.x + a.y * v2.y >= b2.x * v2.x + b2.y * v2.y ? 1 : -1; })();
        const bis = rot(v1, s * ia / 2);
        pdf.setFont('helvetica', 'bold'); pdf.setTextColor(96, 96, 104); pdf.setFontSize(full ? 5.6 : 4.2);
        const rl = full ? 2.7 : 1.6;
        pdf.text(c.L, p[0] + bis.x * rl, p[1] + bis.y * rl, { align: 'center', baseline: 'middle' });
        occAdd(p[0] + bis.x * rl - 1.3, p[1] + bis.y * rl - 1.2, p[0] + bis.x * rl + 1.3, p[1] + bis.y * rl + 1.2);
        if (!full || c.smooth) return;
        const e0 = dm.edges[(c.k - 1 + dm.edges.length) % dm.edges.length], e1 = dm.edges[c.k];
        if (Math.min(e0.len, e1.len) * k < 7) return;
        dimInk(); pdf.setLineWidth(0.12);
        if (Math.abs(c.ang - 90) < 0.3) { // perpendicular constraint
          const r = 1.9; plines([[p[0] + v1.x * r, p[1] + v1.y * r], [p[0] + (v1.x + v2.x) * r, p[1] + (v1.y + v2.y) * r], [p[0] + v2.x * r, p[1] + v2.y * r]], false);
          return;
        }
        const R = 4.6, q = [], steps = Math.max(6, Math.ceil(c.ang / 8));
        for (let i = 0; i <= steps; i++) { const d = rot(v1, s * ia * i / steps); q.push([p[0] + d.x * R, p[1] + d.y * R]); }
        plines(q, false);
        pdf.setFontSize(5); pdf.setTextColor.apply(pdf, DIM);
        const txt = c.ang.toFixed(1) + '°', tw = pdf.getTextWidth(txt), th = 5 * 0.3528, r0 = R + (c.ang < 60 ? 4.2 : 2.9);
        let bestP = null, bestA = Infinity;
        [[0, r0], [0.35, r0 + 0.6], [-0.35, r0 + 0.6], [0, r0 + 3], [0.6, r0 + 2], [-0.6, r0 + 2], [0, r0 + 6]].forEach(function (cnd) {
          const d = rot(bis, cnd[0] * Math.min(1, ia / Math.PI)), x = p[0] + d.x * cnd[1], y = p[1] + d.y * cnd[1];
          if (!AT.inside(g.ol.poly, (x - T.ox) / k + T.bx, (y - T.oy) / k + T.by)) return;
          const a = occHit([x - tw / 2 - 0.3, y - th / 2 - 0.3, x + tw / 2 + 0.3, y + th / 2 + 0.3]);
          if (a < bestA - 1e-6) { bestA = a; bestP = [x, y]; }
        });
        if (!bestP) bestP = [p[0] + bis.x * r0, p[1] + bis.y * r0];
        pdf.text(txt, bestP[0], bestP[1], { align: 'center', baseline: 'middle' });
        occAdd(bestP[0] - tw / 2, bestP[1] - th / 2, bestP[0] + tw / 2, bestP[1] + th / 2);
      });
    };
    if (!full) { // overall size only, as before
      const saK = (showSA ? AT.saMax(pc) : 0) * k, off = saK + 5, fs2 = Math.max(5, Math.min(9, 7 * Math.sqrt(k) + 3));
      const top = Pm(bb.x0, bb.y0)[1] - off, left = Pm(bb.x0, bb.y0)[0] - off;
      const L0 = Pm(pl.x, pl.y), R0 = Pm(pr.x, pr.y), T0 = Pm(pt.x, pt.y), B0 = Pm(pb.x, pb.y);
      dimInk(); pdf.setLineWidth(0.15);
      const edgeT = Pm(bb.x0, bb.y0)[1] - saK - 1, edgeL = Pm(bb.x0, bb.y0)[0] - saK - 1;
      pdf.line(L0[0], Math.min(L0[1] - 1, edgeT), L0[0], top - 1.5); pdf.line(R0[0], Math.min(R0[1] - 1, edgeT), R0[0], top - 1.5);
      pdf.line(L0[0], top, R0[0], top); arrow(L0[0], top, -1, 0, 1.8); arrow(R0[0], top, 1, 0, 1.8);
      pdf.line(Math.min(T0[0] - 1, edgeL), T0[1], left - 1.5, T0[1]); pdf.line(Math.min(B0[0] - 1, edgeL), B0[1], left - 1.5, B0[1]);
      pdf.line(left, T0[1], left, B0[1]); arrow(left, T0[1], 0, -1, 1.8); arrow(left, B0[1], 0, 1, 1.8);
      label(AT.fmtU(bb.w, U), (L0[0] + R0[0]) / 2, top, fs2);
      label(AT.fmtU(bb.h, U), left, (T0[1] + B0[1]) / 2, fs2, { angle: 90 });
      if (k >= 0.03) cornerMarks();
      return;
    }
    // --- aligned / arc-length dimension along part of a segment, dP paper mm outside the sewing line
    const dimAlong = function (sg, La, Lb, dP, curve, txt) {
      const q0 = [AT.polyPointAt(sg.poly, sg.cum, La)];
      for (let i = 0; i < sg.poly.length; i++) if (sg.cum[i] > La + 1e-6 && sg.cum[i] < Lb - 1e-6) q0.push(sg.poly[i]);
      q0.push(AT.polyPointAt(sg.poly, sg.cum, Lb));
      const m = q0.length, nr = [];
      for (let i = 0; i < m; i++) { const a = q0[Math.max(0, i - 1)], b = q0[Math.min(m - 1, i + 1)]; let dx = b.x - a.x, dy = b.y - a.y; const L = Math.hypot(dx, dy) || 1; dx /= L; dy /= L; nr.push({ x: dy * sgn, y: -dx * sgn }); }
      const base = q0.map(function (p) { return Pm(p.x, p.y); });
      let q = base.map(function (p, i) { return [p[0] + nr[i].x * dP, p[1] + nr[i].y * dP]; });
      let bad = false;
      for (let i = 1; i < m; i++) { const ox = base[i][0] - base[i - 1][0], oy = base[i][1] - base[i - 1][1], nx = q[i][0] - q[i - 1][0], ny = q[i][1] - q[i - 1][1]; if (ox * nx + oy * ny < 0) { bad = true; break; } }
      const cum = [0]; for (let i = 1; i < m; i++) cum.push(cum[i - 1] + Math.hypot(q[i][0] - q[i - 1][0], q[i][1] - q[i - 1][1]));
      const Lp = cum[m - 1]; if (Lp < 2.2) return;
      dimInk(); pdf.setLineWidth(0.12);
      [0, m - 1].forEach(function (i) { pdf.line(base[i][0] + nr[i].x * 0.7, base[i][1] + nr[i].y * 0.7, base[i][0] + nr[i].x * (dP + 1.1), base[i][1] + nr[i].y * (dP + 1.1)); });
      pdf.setFontSize(DFS); const tw = pdf.getTextWidth(txt);
      if (!bad) {
        plines(q, false);
        if (Lp > tw + 4.5) { arrow(q[0][0], q[0][1], q[0][0] - q[1][0], q[0][1] - q[1][1], 1.5); arrow(q[m - 1][0], q[m - 1][1], q[m - 1][0] - q[m - 2][0], q[m - 1][1] - q[m - 2][1], 1.5); }
      }
      // value at the middle of the dimension line, just outside it
      let j = 1; while (j < m - 1 && cum[j] < Lp / 2) j++;
      const u = (Lp / 2 - cum[j - 1]) / ((cum[j] - cum[j - 1]) || 1), mx = q[j - 1][0] + (q[j][0] - q[j - 1][0]) * u, my = q[j - 1][1] + (q[j][1] - q[j - 1][1]) * u;
      let ux = q[j][0] - q[j - 1][0], uy = q[j][1] - q[j - 1][1]; const ul = Math.hypot(ux, uy) || 1; ux /= ul; uy /= ul;
      const nI = nr[Math.min(m - 1, j)], h = DFS * 0.3528, lift = 0.75 + h * 0.36 + (curve ? 0.9 : 0) + (Lp < tw + 1.5 ? h * 1.05 : 0);
      const cx = mx + nI.x * lift, cy = my + nI.y * lift;
      const r = rtext(txt, cx, cy, ux, uy, DFS);
      if (curve) arcSym(cx, cy, r);
    };
    // edges (with chained notch positions)
    dm.edges.forEach(function (e, kk) {
      const sg = ol.segs[kk], d1 = (showSA ? e.allow : 0) * k + 3.0;
      const nts = AT.asArray(pc.notches).filter(function (n) { return n.s === sg.id; }).map(function (n) { return sg.len * (n.t || 0); }).filter(function (L) { return L > 0.5 && L < sg.len - 0.5; }).sort(function (a, b) { return a - b; });
      if (nts.length) {
        const cuts = [0].concat(nts, [sg.len]);
        for (let i = 0; i < cuts.length - 1; i++) dimAlong(sg, cuts[i], cuts[i + 1], d1, e.curve, AT.fmtN(cuts[i + 1] - cuts[i], U));
        dimAlong(sg, 0, sg.len, d1 + 4.2, e.curve, AT.fmtN(sg.len, U));
      } else dimAlong(sg, 0, sg.len, d1, e.curve, AT.fmtN(sg.len, U));
    });
    // internal lines: length along the line
    AT.asArray(pc.internals).forEach(function (il) {
      const pl2 = AT.evalOpen(il.closed ? il.pts.concat([il.pts[0]]) : il.pts); if (pl2.length < 2) return;
      const cm = AT.polyCum(pl2), L = cm[cm.length - 1]; if (L * k < 8) return;
      const mp = AT.polyPointAt(pl2, cm, L / 2), p = Pm(mp.x, mp.y);
      dimInk(); const r = rtext(AT.fmtN(L, U), p[0] - mp.ty * 1.6, p[1] + mp.tx * 1.6, mp.tx, mp.ty, DFS);
      if (il.pts.some(function (q) { return q.c; })) arcSym(p[0] - mp.ty * 1.6, p[1] + mp.tx * 1.6, r);
    });
    // ordinates from the datum (top-left of the net bounding box): each feature goes to the nearest rail it can reach
    const feats = dm.corners.map(function (c) { return { x: c.x, y: c.y }; });
    AT.asArray(pc.internals).forEach(function (il) { const q = AT.evalOpen(il.pts); if (q.length) { feats.push(q[0]); feats.push(q[q.length - 1]); } });
    const rails = { T: [], B: [], L: [], R: [] };
    feats.forEach(function (f) {
      const score = function (dx, dy, dist) { return AT.rayCrossings(ol.poly, f, dx, dy, 0.8) * 1e6 + dist; };
      const sT = score(0, -1, f.y - bb.y0), sB = score(0, 1, bb.y1 - f.y), sL = score(-1, 0, f.x - bb.x0), sR = score(1, 0, bb.x1 - f.x);
      rails[sT <= sB ? 'T' : 'B'].push({ v: f.x - bb.x0, f: f });
      rails[sL <= sR ? 'L' : 'R'].push({ v: f.y - bb.y0, f: f });
    });
    Object.keys(rails).forEach(function (key) { // one tick per value, from the feature nearest the rail
      const near = function (e) { return key === 'T' ? e.f.y : key === 'B' ? -e.f.y : key === 'L' ? e.f.x : -e.f.x; };
      const arr = rails[key].slice().sort(function (a, b) { return a.v - b.v || near(a) - near(b); }), out = [];
      arr.forEach(function (e) { const l = out[out.length - 1]; if (l && Math.abs(l.v - e.v) < 0.3) { if (near(e) < near(l)) out[out.length - 1] = e; } else out.push(e); });
      rails[key] = out;
    });
    const RO = 7, JOG = 2.0, TG = 2.6, h = DFS * 0.3528, gap = h + 0.55, maxW = {};
    dimInk(); pdf.setLineWidth(0.12); pdf.setFontSize(DFS);
    ['T', 'B', 'L', 'R'].forEach(function (key) {
      const arr = rails[key]; maxW[key] = 0; if (!arr.length) return;
      const horiz = key === 'T' || key === 'B', rail = key === 'T' ? Y0 - RO : key === 'B' ? Y1 + RO : key === 'L' ? X0 - RO : X1 + RO, dir = key === 'T' || key === 'L' ? -1 : 1;
      const fp = arr.map(function (e) { return Pm(e.f.x, e.f.y); }), want = fp.map(function (p) { return horiz ? p[0] : p[1]; }), at = spread(want, gap);
      arr.forEach(function (e, i) {
        const p = fp[i], txt = AT.fmtN(e.v, U), w = pdf.getTextWidth(txt); maxW[key] = Math.max(maxW[key], w);
        if (horiz) {
          pdf.line(p[0], p[1] + dir * 0.7, p[0], rail); pdf.line(p[0], rail, at[i], rail + dir * JOG);
          rtext(txt, at[i], rail + dir * (TG + w / 2), 0, -1, DFS);
        } else {
          pdf.line(p[0] + dir * 0.7, p[1], rail, p[1]); pdf.line(rail, p[1], rail + dir * JOG, at[i]);
          rtext(txt, rail + dir * (TG + w / 2), at[i], 1, 0, DFS);
        }
      });
    });
    // overall size (outermost)
    const ovT = (rails.T.length ? Y0 - RO - TG - maxW.T - 3.4 : Y0 - 5), ovL = (rails.L.length ? X0 - RO - TG - maxW.L - 3.4 : X0 - 5);
    const L0 = Pm(bb.x0, 0)[0], R0 = Pm(bb.x1, 0)[0], T0 = Pm(0, bb.y0)[1], B0 = Pm(0, bb.y1)[1];
    dimInk(); pdf.setLineWidth(0.12);
    pdf.line(L0, Y0 - 1, L0, ovT - 1.2); pdf.line(R0, Y0 - 1, R0, ovT - 1.2); pdf.line(L0, ovT, R0, ovT); arrow(L0, ovT, -1, 0, 1.6); arrow(R0, ovT, 1, 0, 1.6);
    pdf.line(X0 - 1, T0, ovL - 1.2, T0); pdf.line(X0 - 1, B0, ovL - 1.2, B0); pdf.line(ovL, T0, ovL, B0); arrow(ovL, T0, 0, -1, 1.6); arrow(ovL, B0, 0, 1, 1.6);
    pdf.setFont('helvetica', 'bold');
    rtext(AT.fmtN(bb.w, U), (L0 + R0) / 2, ovT - 0.8 - h * 0.36, 1, 0, DFS + 0.6);
    rtext(AT.fmtN(bb.h, U), ovL - 0.8 - h * 0.36, (T0 + B0) / 2, 0, -1, DFS + 0.6);
    pdf.setFont('helvetica', 'normal');
    // datum mark at 0,0
    const dx0 = L0, dy0 = T0; pdf.setLineWidth(0.12); pdf.circle(dx0, dy0, 0.9, 'S'); pdf.line(dx0 - 1.6, dy0, dx0 + 1.6, dy0); pdf.line(dx0, dy0 - 1.6, dx0, dy0 + 1.6);
    // symmetry: centre line
    if (dm.sym) {
      const cxp = Pm(dm.cx, 0)[0]; pdf.setDrawColor(110, 110, 118); pdf.setLineWidth(0.15); pdf.setLineDashPattern([6, 1.2, 1, 1.2], 0);
      pdf.line(cxp, Y0 - 3, cxp, Y1 + 3); pdf.setLineDashPattern([], 0);
      pdf.setTextColor(110, 110, 118); pdf.setFontSize(5.4); pdf.setFont('helvetica', 'bold');
      pdf.setFont('helvetica', 'normal');
    }
    cornerMarks();
    if (dm.sym) {
      const cxp = Pm(dm.cx, 0)[0]; pdf.setFontSize(5.4); pdf.setFont('helvetica', 'bold'); pdf.setTextColor(110, 110, 118);
      let yb = null;
      for (let y = Y0 + 9; y < Y1 - 6 && yb == null; y += 2.5) { if (AT.inside(ol.poly, dm.cx, (y - T.oy) / k + T.by) && occHit([cxp + 0.5, y - 1.3, cxp + 4.6, y + 1.3]) === 0) yb = y; }
      if (yb != null) { pdf.text('CL', cxp + 1.0, yb, { baseline: 'middle' }); occAdd(cxp + 0.5, yb - 1.2, cxp + 4.4, yb + 1.2); }
      pdf.setFont('helvetica', 'normal');
    }
  };
  // line legend (one row)
  const lineLegend = function (x, y, maxX) {
    const items = [['cut', 'cut line'], ['sew', 'stitching line (sewn)'], ['free', 'free edge (finished)'], ['hem', 'hem fold'], ['band', 'seam / hem allowance'], ['num', 'seam number']];
    pdf.setFont('helvetica', 'normal'); pdf.setFontSize(6.3);
    items.forEach(function (itm) {
      const w = 7 + pdf.getTextWidth(itm[1]) + 5; if (x + w > maxX) return;
      pdf.setDrawColor.apply(pdf, INK); pdf.setLineDashPattern([], 0);
      if (itm[0] === 'cut') { pdf.setLineWidth(0.4); pdf.line(x, y, x + 5.5, y); }
      if (itm[0] === 'sew') { pdf.setLineWidth(0.22); pdf.setLineDashPattern([1.6, 1.1], 0); pdf.line(x, y, x + 5.5, y); }
      if (itm[0] === 'free') { pdf.setLineWidth(0.3); pdf.line(x, y, x + 5.5, y); }
      if (itm[0] === 'hem') { pdf.setLineWidth(0.3); pdf.setLineDashPattern([3.2, 1, 0.5, 1], 0); pdf.line(x, y, x + 5.5, y); }
      if (itm[0] === 'band') { pdf.setFillColor.apply(pdf, SA_FILL); pdf.rect(x, y - 1.3, 2.7, 2.6, 'F'); pdf.setFillColor.apply(pdf, HEM_FILL); pdf.rect(x + 2.8, y - 1.3, 2.7, 2.6, 'F'); }
      if (itm[0] === 'num') { pdf.setFillColor(232, 101, 74); pdf.circle(x + 2.5, y, 1.8, 'F'); pdf.setTextColor(255, 255, 255); pdf.setFont('helvetica', 'bold'); pdf.setFontSize(5.5); pdf.text('3', x + 2.5, y + 0.05, { align: 'center', baseline: 'middle' }); pdf.setFont('helvetica', 'normal'); pdf.setFontSize(6.3); }
      pdf.setLineDashPattern([], 0); pdf.setTextColor.apply(pdf, MUTE); pdf.text(itm[1], x + 7, y + 0.9);
      x += w;
    });
  };
  // sewing schedule: every seam with its number, both sides and their lengths
  const sewSchedule = function () {
    const lens = AT.sewLengths(doc), list = Object.values(doc.sewing || {}).filter(Boolean).sort(function (a, b) { return (nums[a.id] || 0) - (nums[b.id] || 0); });
    const per = 22;
    for (let start = 0; start < list.length; start += per) {
      const pg = newPage(P[1], P[0]); header(pg, 'Sewing schedule' + (list.length > per ? ' (' + (start / per + 1) + ')' : ''));
      const M = 12, cols = [{ h: '#', w: 10 }, { h: 'Side A', w: 104 }, { h: 'Side B', w: 104 }, { h: 'A', w: 20, a: 1 }, { h: 'B', w: 20, a: 1 }, { h: 'Δ', w: 15, a: 1 }];
      let y = 32;
      pdf.setFontSize(7.5); pdf.setFont('helvetica', 'bold'); pdf.setTextColor.apply(pdf, MUTE);
      let cx = M; cols.forEach(function (c) { pdf.text(c.h === 'Δ' ? 'DIFF' : c.h.toUpperCase(), cx + (c.a ? c.w - 1 : 0), y, { align: c.a ? 'right' : 'left' }); cx += c.w; });
      pdf.setDrawColor(200, 198, 194); pdf.setLineWidth(0.2); pdf.line(M, y + 1.8, cx, y + 1.8);
      list.slice(start, start + per).forEach(function (sw) {
        y += 7;
        const L = lens[sw.id] || { a: 0, b: 0 }, nm = function (sides) { return AT.asArray(sides).map(function (sd) { const pc = doc.pieces[sd.p]; return pc ? pc.name : '?'; }).join(' + '); };
        pdf.setFillColor.apply(pdf, hex(sw.color || '#E8654A')); pdf.circle(M + 2.6, y - 1.1, 2.3, 'F');
        pdf.setFont('helvetica', 'bold'); pdf.setFontSize(7.2); pdf.setTextColor(255, 255, 255); pdf.text(String(nums[sw.id]), M + 2.6, y - 1.05, { align: 'center', baseline: 'middle' });
        pdf.setFont('helvetica', 'normal'); pdf.setFontSize(8.5); pdf.setTextColor.apply(pdf, INK);
        let x = M + cols[0].w;
        pdf.text(nm(sw.a), x, y); x += cols[1].w; pdf.text(nm(sw.b), x, y); x += cols[2].w;
        pdf.text(AT.fmtU(L.a, U), x + cols[3].w - 1, y, { align: 'right' }); x += cols[3].w;
        pdf.text(AT.fmtU(L.b, U), x + cols[4].w - 1, y, { align: 'right' }); x += cols[4].w;
        const df = Math.abs(L.a - L.b); pdf.setTextColor.apply(pdf, df > 3 ? [200, 70, 50] : MUTE); pdf.text(df < 0.5 ? '–' : Math.round(df) + ' mm', x + cols[5].w - 1, y, { align: 'right' });
        pdf.setDrawColor(236, 234, 230); pdf.line(M, y + 2.2, x + cols[5].w, y + 2.2);
      });
      pdf.setFontSize(7); pdf.setTextColor.apply(pdf, MUTE);
      pdf.text('Lengths along the stitching line, size ' + doc.activeSize + '. The coloured number is printed beside both edges of each seam on the pattern pieces; sew edges with the same number together.', M, pg.h - 10);
    }
  };
  const pieceFacts = function (it) {
    const g = it.g, pc = it.pc, fab = doc.fabrics[pc.fabric];
    return [['Fabric', fab ? fab.name : '—'], ['Cut', String(pc.qty || 1) + (pc.mirror ? ' · mirrored pair' : '')], ['Net size', AT.fmtU(g.bb.w, U) + ' × ' + AT.fmtU(g.bb.h, U)], ['Cut size', AT.fmtU(g.cb.w, U) + ' × ' + AT.fmtU(g.cb.h, U)], ['Allowances', AT.allowanceSummary(pc)], ['Perimeter', AT.fmtU(g.perim, U)], ['Area (cut)', AT.fmtArea(g.cutArea, U)], ['Grain', (pc.grain || 0) + '°']];
  };
  const scaleBar = function (x, y, k) {
    const unitMM = U === 'in' ? 25.4 : 10, n = U === 'in' ? 4 : 10, len = unitMM * n * k;
    pdf.setDrawColor.apply(pdf, INK); pdf.setLineWidth(0.2);
    for (let i = 0; i < n; i++) { if (i % 2 === 0) { pdf.setFillColor.apply(pdf, INK); pdf.rect(x + i * unitMM * k, y, unitMM * k, 1.6, 'F'); } }
    pdf.rect(x, y, len, 1.6, 'S');
    pdf.setFontSize(6.5); pdf.setTextColor.apply(pdf, MUTE); pdf.setFont('helvetica', 'normal');
    pdf.text('0', x, y + 4.6, { align: 'center' }); pdf.text(n + (U === 'in' ? ' in' : ' cm'), x + len, y + 4.6, { align: 'center' });
    pdf.text(k >= 0.999 ? 'Scale 1:1' : 'Scale 1:' + (1 / k).toFixed(1), x + len + 6, y + 1.5);
  };
  const legend = function (x, y) {
    if (!(o.sizes === 'all' && doc.sizes.length > 1)) return x;
    pdf.setFontSize(7); pdf.setFont('helvetica', 'normal');
    doc.sizes.forEach(function (sz, i) {
      const col = sz === doc.activeSize ? INK : hex(NEST[i % NEST.length]);
      pdf.setDrawColor.apply(pdf, col); pdf.setLineWidth(sz === doc.activeSize ? 0.4 : 0.25); pdf.line(x, y, x + 6, y);
      pdf.setTextColor.apply(pdf, col); pdf.text(sz, x + 7.5, y + 1); x += 16;
    });
    return x;
  };
  const schedule = function () {
    const pg = newPage(P[1], P[0]); header(pg, 'Piece schedule');
    const M = 12, cols = [{ h: '#', w: 8 }, { h: 'Piece', w: 54 }, { h: 'Fabric', w: 46 }, { h: 'Cut', w: 12, a: 1 }, { h: 'Net W', w: 22, a: 1 }, { h: 'Net L', w: 22, a: 1 }, { h: 'Cut W', w: 22, a: 1 }, { h: 'Cut L', w: 22, a: 1 }, { h: 'Perimeter', w: 24, a: 1 }, { h: 'Area (cut)', w: 24, a: 1 }];
    let y = 32;
    pdf.setFontSize(7.5); pdf.setFont('helvetica', 'bold'); pdf.setTextColor.apply(pdf, MUTE);
    let cx = M; cols.forEach(function (c) { pdf.text(c.h.toUpperCase(), cx + (c.a ? c.w - 1 : 0), y, { align: c.a ? 'right' : 'left' }); cx += c.w; });
    pdf.setDrawColor(200, 198, 194); pdf.setLineWidth(0.2); pdf.line(M, y + 1.8, cx, y + 1.8);
    pdf.setFont('helvetica', 'normal'); pdf.setFontSize(8.5);
    items.forEach(function (it, i) {
      y += 7; if (y > pg.h - 16) return;
      const g = it.g, pc = it.pc, fab = doc.fabrics[pc.fabric];
      const row = [i + 1, pc.name, fab ? fab.name : '—', (pc.qty || 1) + (pc.mirror ? ' M' : ''), AT.fmtU(g.bb.w, U), AT.fmtU(g.bb.h, U), AT.fmtU(g.cb.w, U), AT.fmtU(g.cb.h, U), AT.fmtU(g.perim, U), AT.fmtArea(g.cutArea, U)];
      let x = M; pdf.setTextColor.apply(pdf, INK);
      cols.forEach(function (c, j) { pdf.text(String(row[j]), x + (c.a ? c.w - 1 : 0), y, { align: c.a ? 'right' : 'left' }); x += c.w; });
      pdf.setDrawColor(236, 234, 230); pdf.line(M, y + 2, x, y + 2);
    });
    pdf.setFontSize(7); pdf.setTextColor.apply(pdf, MUTE);
    pdf.text('Net = sewing line. Cut = including seam allowance. W = width, L = length (2D pattern axes). "M" = mirrored pair.', M, pg.h - 10);
  };
  const tileId = function (r, c) { let s = '', n = r; do { s = String.fromCharCode(65 + (n % 26)) + s; n = Math.floor(n / 26) - 1; } while (n >= 0); return s + (c + 1); };

  // CAD sheet frame and title block (piece pages)
  const frame = function (pg) { pdf.setDrawColor.apply(pdf, INK); pdf.setLineDashPattern([], 0); pdf.setLineWidth(0.35); pdf.rect(6, 6, pg.w - 12, pg.h - 12, 'S'); };
  const titleBlock = function (pg, it, k) {
    const g = it.g, pc = it.pc, fab = doc.fabrics[pc.fabric], x0 = 6, x1 = pg.w - 6, W = x1 - x0, y0 = pg.h - 38;
    const rowsTB = [
      { h: 13, cells: [['PIECE', pc.name, 0.3, 1], ['FABRIC', fab ? fab.name : '—', 0.2], ['CUT', String(pc.qty || 1) + (pc.mirror ? ' · mirrored' : ''), 0.12], ['SIZE', doc.activeSize, 0.08], ['SCALE', k >= 0.999 ? '1 : 1' : '1 : ' + (1 / k).toFixed(1), 0.18, 0, 'bar'], ['SHEET', pageNo + ' / ' + total, 0.12]] },
      { h: 11, cells: [['NET W × L', AT.fmtU(g.bb.w, U) + ' × ' + AT.fmtU(g.bb.h, U), 0.2], ['CUT W × L', AT.fmtU(g.cb.w, U) + ' × ' + AT.fmtU(g.cb.h, U), 0.2], ['PERIMETER (NET)', AT.fmtU(g.perim, U), 0.15], ['AREA (CUT)', AT.fmtArea(g.cutArea, U), 0.13], ['ALLOWANCES', AT.allowanceSummary(pc), 0.22], ['GRAIN', (pc.grain || 0) + '°', 0.1]] },
      { h: 8, cells: [['NOTES', '', 1, 0, 'notes']] }
    ];
    pdf.setLineDashPattern([], 0);
    let y = y0;
    rowsTB.forEach(function (row) {
      let x = x0;
      row.cells.forEach(function (c) {
        const w = W * c[2];
        pdf.setDrawColor.apply(pdf, INK); pdf.setLineWidth(c[4] === 'notes' ? 0.2 : 0.2); pdf.rect(x, y, w, row.h, 'S');
        pdf.setFont('helvetica', 'normal'); pdf.setFontSize(5.2); pdf.setTextColor.apply(pdf, MUTE); pdf.text(c[0], x + 1.6, y + 2.9);
        if (c[4] === 'notes') {
          pdf.setFontSize(6.3); pdf.setTextColor.apply(pdf, INK);
          const t1 = 'Dimensions in ' + AT.unitWord(U) + ', on the sewing line (net). Ordinates from datum 0 at the top-left of the net bounding box. ';
          pdf.text(t1, x + 13, y + 5.2);
          let tx = x + 13 + pdf.getTextWidth(t1) + 1.4;
          dimInk(); arcSym(tx + 1.5, y + 5.2 - 0.8, { w: 3, h: 1.6, ux: 1, uy: 0, upx: 0, upy: -1 }); pdf.setFontSize(6.3); pdf.setTextColor.apply(pdf, INK);
          pdf.text('= length along a curve.  Letters key the dimension table.', tx + 3.6, y + 5.2);
        } else {
          pdf.setTextColor.apply(pdf, INK); pdf.setFont('helvetica', c[3] ? 'bold' : 'normal'); pdf.setFontSize(c[3] ? 11 : 8.4);
          pdf.text(String(c[1]), x + 1.6, y + row.h - (c[3] ? 2.6 : 2.4));
          if (c[4] === 'bar') { // 10 cm (4 in) bar at drawing scale, if it fits
            const um = U === 'in' ? 25.4 : 10, n = U === 'in' ? 4 : 10, bw = um * n * k, bx = x + w - bw - 2.4, by = y + row.h - 4.4;
            pdf.setFont('helvetica', 'normal');
            if (bw < w - 22 && bw > 6) { for (let i = 0; i < n; i++) if (i % 2 === 0) { pdf.setFillColor.apply(pdf, INK); pdf.rect(bx + i * um * k, by, um * k, 1.3, 'F'); } pdf.setDrawColor.apply(pdf, INK); pdf.setLineWidth(0.15); pdf.rect(bx, by, bw, 1.3, 'S'); pdf.setFontSize(4.8); pdf.setTextColor.apply(pdf, MUTE); pdf.text('0', bx, by + 3.4, { align: 'center' }); pdf.text(n + (U === 'in' ? ' in' : ' cm'), bx + bw, by + 3.4, { align: 'center' }); }
          }
        }
        x += w;
      });
      y += row.h;
    });
  };
  // dimension tables: every corner (ordinate, angle) and the edge that leaves it (type, length, chord, seam, finish)
  const dimTables = function () {
    const pages = AT.dimTablePages(doc, items, P), RH = AT.DIM_TABLE_ROW;
    const cols = [{ h: 'PT', w: 8 }, { h: 'X', w: 14, a: 1 }, { h: 'Y', w: 14, a: 1 }, { h: 'ANGLE', w: 14, a: 1 }, { h: 'EDGE', w: 13, c: 1 }, { h: 'TYPE', w: 11 }, { h: 'LENGTH', w: 15, a: 1 }, { h: 'CHORD', w: 15, a: 1 }, { h: 'SEAM', w: 10, a: 1 }, { h: 'FINISH', w: 19, a: 1 }];
    pages.forEach(function (page, pi) {
      const pg = newPage(P[1], P[0]); header(pg, 'Dimension tables' + (pages.length > 1 ? ' (' + (pi + 1) + ' / ' + pages.length + ')' : ''));
      const colW = (pg.w - 24 - 8) / 2;
      page.forEach(function (col, ci) {
        const cx0 = 12 + ci * (colW + 8);
        col.forEach(function (blk) {
          const t = blk.t, it = blk.it; let y = 32 + blk.y;
          pdf.setFont('helvetica', 'bold'); pdf.setFontSize(8.5); pdf.setTextColor.apply(pdf, INK); pdf.text(it.pc.name, cx0, y);
          pdf.setFont('helvetica', 'normal'); pdf.setFontSize(6.5); pdf.setTextColor.apply(pdf, MUTE);
          pdf.text('net ' + AT.fmtU(it.g.bb.w, U) + ' × ' + AT.fmtU(it.g.bb.h, U) + '  ·  perimeter ' + AT.fmtU(it.g.perim, U) + (t.dm.sym ? '  ·  symmetric about CL' : ''), cx0 + colW, y, { align: 'right' });
          y += RH;
          pdf.setFont('helvetica', 'bold'); pdf.setFontSize(5.8); pdf.setTextColor.apply(pdf, MUTE);
          let x = cx0; cols.forEach(function (c) { pdf.text(c.h, c.a ? x + c.w - 1 : c.c ? x + c.w / 2 : x, y, { align: c.a ? 'right' : c.c ? 'center' : 'left' }); x += c.w; });
          pdf.setDrawColor(200, 198, 194); pdf.setLineWidth(0.2); pdf.line(cx0, y + 1.3, cx0 + colW, y + 1.3);
          pdf.setFont('helvetica', 'normal'); pdf.setFontSize(7);
          t.rows.forEach(function (r) {
            y += RH;
            const e = r.e, c = r.c;
            const vals = [c.L, AT.fmtN(c.X, U), AT.fmtN(c.Y, U), c.smooth ? 'tangent' : c.ang.toFixed(1) + '°', e.from + '–' + e.to, e.curve ? 'curve' : 'line', AT.fmtN(e.len, U), e.curve ? AT.fmtN(e.chord, U) : '', r.seams.join(', ') || 'free', (e.hem ? 'hem ' : 'SA ') + AT.fmtN(e.allow, U)];
            let xx = cx0;
            cols.forEach(function (cc, j) {
              pdf.setFont('helvetica', j === 0 ? 'bold' : 'normal'); pdf.setTextColor.apply(pdf, j === 8 && !r.seams.length ? MUTE : INK);
              pdf.text(String(vals[j]), cc.a ? xx + cc.w - 1 : cc.c ? xx + cc.w / 2 : xx, y, { align: cc.a ? 'right' : cc.c ? 'center' : 'left' }); xx += cc.w;
            });
            pdf.setDrawColor(236, 234, 230); pdf.line(cx0, y + 1.3, cx0 + colW, y + 1.3);
          });
          if (t.notches.length) {
            y += RH; pdf.setFont('helvetica', 'bold'); pdf.setFontSize(5.8); pdf.setTextColor.apply(pdf, MUTE); pdf.text('NOTCHES', cx0, y);
            pdf.setFont('helvetica', 'normal'); pdf.setFontSize(7); pdf.setTextColor.apply(pdf, INK);
            t.notches.forEach(function (nt) { y += RH; pdf.text('on ' + nt.from + '–' + nt.to + ':  ' + AT.fmtN(nt.a, U) + ' from ' + nt.from + '  ·  ' + AT.fmtN(nt.b, U) + ' from ' + nt.to, cx0 + 8, y); });
          }
        });
      });
      pdf.setFontSize(7); pdf.setTextColor.apply(pdf, MUTE); pdf.setFont('helvetica', 'normal');
      pdf.text('Values in ' + AT.unitWord(U) + ', size ' + doc.activeSize + ', on the sewing line. X, Y from datum 0 at the top-left of each piece\'s net bounding box (Y downward). Angle = interior angle between edge tangents. Length runs along the edge (along the curve for curves); chord = straight distance.', 12, pg.h - 10, { maxWidth: pg.w - 24 });
    });
  };

  if (o.layout === 'pieces') {
    items.forEach(function (it) {
      const f = it.fit || AT.ppdfPieceFit(it, P, o.dims), pw = f.pw, ph = f.ph, k = f.k, pg = newPage(pw, ph);
      frame(pg); header(pg, null);
      pdf.setTextColor.apply(pdf, INK); pdf.setFont('helvetica', 'bold'); pdf.setFontSize(14); pdf.text(it.pc.name, 12, 22);
      const T = { k: k, bx: it.box.x0, by: it.box.y0, ox: 12 + f.R[0] + (f.aw - it.box.w * k) / 2, oy: 27 + f.R[1] + (f.ah - it.box.h * k) / 2 };
      drawPiece(it, T);
      lineLegend(12, ph - 41.5, pw - 12);
      titleBlock(pg, it, k);
      legend(pw - 12 - doc.sizes.length * 16, 21);
    });
    schedule(); if (o.dims) dimTables(); sewSchedule();
  } else if (o.layout === 'overview') {
    const pw = P[1], ph = P[0], pg = newPage(pw, ph);
    header(pg, 'Pattern overview');
    const aw = pw - 24, ah = ph - 30 - 20, maxW = Math.max.apply(null, items.map(function (it) { return it.box.w + 14; }));
    const sumW = items.reduce(function (t, it) { return t + it.box.w + 54; }, 0);
    let pk = null, k = 0, kg = 0.08;
    for (let pass = 0; pass < 4; pass++) { // room for labels and dimension lines is fixed on the page, so it depends on the scale
      const room = o.dims ? (kg >= AT.DIM_FULL ? AT.DIM_ROOM.map(function (v) { return v / kg; }) : [13 / kg, 11 / kg, 2 / kg, 2 / kg]) : [2 / kg, 2 / kg, 2 / kg, 2 / kg], gap = 7 / kg; let best = null, bk = 0;
      for (let i = 0; i <= 24; i++) { const W = maxW + (sumW - maxW) * i / 24, c = AT.ppdfPack(items, W + room[0], gap, o.dims, room), kk = Math.min(aw / c.w, ah / c.h, 1); if (kk > bk) { bk = kk; best = c; } }
      pk = best; k = bk; if (Math.abs(kg - k) / k < 0.02) break; kg = k;
    }
    const ox = 12 + (aw - pk.w * k) / 2, oy = 28 + (ah - pk.h * k) / 2;
    items.forEach(function (it, i) { const p = pk.pos[i]; drawPiece(it, { k: k, bx: it.box.x0, by: it.box.y0, ox: ox + p.x * k, oy: oy + p.y * k }); });
    scaleBar(12, ph - 9, k);
    lineLegend(80, ph - 7.6, pw - 12);
    legend(pw - 12 - doc.sizes.length * 16, 21);
    schedule(); if (o.dims) dimTables(); sewSchedule();
  } else if (o.layout === 'tiled') {
    const pk = plan.pk, tw = plan.tw, th = plan.th, M = 10;
    // cover
    const cv = newPage(P[0], P[1]); header(cv, 'Full-scale pattern');
    pdf.setFont('helvetica', 'normal'); pdf.setFontSize(9); pdf.setTextColor.apply(pdf, INK);
    pdf.text(['Print every page at 100 % ("Actual size"), not "Fit to page".', 'Check the test square, then trim each tile on its border and butt the borders together,', 'matching the tile letters on each edge. Rows are letters, columns are numbers.'], 12, 32);
    const sq = U === 'in' ? 50.8 : 50;
    pdf.setDrawColor.apply(pdf, INK); pdf.setLineWidth(0.3); pdf.rect(12, 50, sq, sq, 'S');
    pdf.setFontSize(8); pdf.text(U === 'in' ? '2 in test square' : '5 cm test square', 12 + sq / 2, 50 + sq / 2, { align: 'center', baseline: 'middle' });
    // assembly map
    const mx = 12 + sq + 14, my = 50, mw = cv.w - mx - 12, mh = cv.h - my - 30, mk = Math.min(mw / (plan.cols * tw), mh / (plan.rows * th));
    pdf.setFontSize(6.5);
    for (let r = 0; r < plan.rows; r++) for (let c = 0; c < plan.cols; c++) {
      const used = plan.used.some(function (u) { return u[0] === r && u[1] === c; });
      const x = mx + c * tw * mk, y = my + r * th * mk;
      pdf.setDrawColor(200, 198, 194); pdf.setLineWidth(0.15);
      if (used) { pdf.setFillColor(244, 242, 238); pdf.rect(x, y, tw * mk, th * mk, 'FD'); } else pdf.rect(x, y, tw * mk, th * mk, 'S');
      pdf.setTextColor.apply(pdf, used ? INK : MUTE); pdf.text(tileId(r, c), x + 1.2, y + 3);
    }
    items.forEach(function (it, i) { const p = pk.pos[i]; pdf.setDrawColor.apply(pdf, INK); pdf.setLineWidth(0.2); poly(it.g.cut, { k: mk, bx: it.box.x0, by: it.box.y0, ox: mx + p.x * mk, oy: my + p.y * mk }, true); });
    lineLegend(12, cv.h - 14, cv.w - 12);
    // tiles
    plan.used.forEach(function (u) {
      const r = u[0], c = u[1], pg = newPage(P[0], P[1]);
      const x0 = c * tw, y0 = r * th;
      pdf.saveGraphicsState(); pdf.rect(M, M, tw, th, null); pdf.clip(); pdf.discardPath();
      items.forEach(function (it, i) { const p = pk.pos[i], RM = pk.room; if (p.x - RM[0] - 2 > x0 + tw || p.x + it.box.w + RM[2] + 2 < x0 || p.y - RM[1] - 2 > y0 + th || p.y + it.box.h + RM[3] + 2 < y0) return; drawPiece(it, { k: 1, bx: it.box.x0, by: it.box.y0, ox: M + p.x - x0, oy: M + p.y - y0 }); });
      pdf.restoreGraphicsState();
      pdf.setDrawColor(150, 150, 156); pdf.setLineWidth(0.15); pdf.setLineDashPattern([2, 1.5], 0); pdf.rect(M, M, tw, th, 'S'); pdf.setLineDashPattern([], 0);
      pdf.setFont('helvetica', 'bold'); pdf.setFontSize(9); pdf.setTextColor.apply(pdf, INK); pdf.text(tileId(r, c), M + 1.5, M + 4.5);
      pdf.setFont('helvetica', 'normal'); pdf.setFontSize(6.5); pdf.setTextColor.apply(pdf, MUTE);
      if (r > 0) pdf.text(tileId(r - 1, c), M + tw / 2, M - 2.5, { align: 'center' });
      if (r < plan.rows - 1) pdf.text(tileId(r + 1, c), M + tw / 2, M + th + 4.5, { align: 'center' });
      if (c > 0) pdf.text(tileId(r, c - 1), M - 2.5, M + th / 2, { align: 'center', angle: 90 });
      if (c < plan.cols - 1) pdf.text(tileId(r, c + 1), M + tw + 4.5, M + th / 2, { align: 'center', angle: 90 });
      const ft = 'size ' + doc.activeSize + '  ·  ' + pageNo + ' / ' + total;
      pdf.text(ft, P[0] - M, P[1] - 4, { align: 'right' }); pdf.text(doc.meta.name, P[0] - M - pdf.uniWidth(ft) - 3, P[1] - 4, { align: 'right' });
    });
    if (o.dims) dimTables(); sewSchedule();
  } else {
    const pk = plan.pk, M = 10, pw = plan.roll;
    plan.sheets.forEach(function (sh) {
      const ph = sh.h + 2 * M + 30, pg = newPage(pw, ph);
      header(pg, null);
      sh.idx.forEach(function (i) { const it = items[i], p = pk.pos[i]; drawPiece(it, { k: 1, bx: it.box.x0, by: it.box.y0, ox: M + p.x, oy: 22 + p.y - sh.y0 }); });
      scaleBar(12, ph - 9, 1);
      lineLegend(140, ph - 7.6, pw - 12 - doc.sizes.length * 16 - 6);
      legend(pw - 12 - doc.sizes.length * 16, ph - 7.5);
    });
  }
  return pdf;
};


/* ============================================================
   ATELIER APP — React UI
   ============================================================ */
const Ctx = React.createContext(null);
const useApp = () => React.useContext(Ctx);
const ATL_CSS = `
.atl{--bg:#17181b;--panel:#1e1f23;--panel2:#25262b;--line:rgba(255,255,255,.075);--line2:rgba(255,255,255,.13);--text:#e9e9ec;--muted:#8d8e96;--faint:#5f6068;--accent:#E8654A;--accentSoft:rgba(232,101,74,.15);--input:#2a2b30;--hover:rgba(255,255,255,.06);--shadow:0 12px 32px rgba(0,0,0,.45);color:var(--text);background:var(--bg);font-family:Inter,ui-sans-serif,system-ui,-apple-system,"Segoe UI",sans-serif;font-size:12px;letter-spacing:-.003em}
.atl.light{--bg:#e9e9eb;--panel:#fbfbfb;--panel2:#f2f2f4;--line:rgba(0,0,0,.075);--line2:rgba(0,0,0,.14);--text:#1b1b1e;--muted:#6c6c74;--faint:#a0a0a8;--input:#ffffff;--hover:rgba(0,0,0,.045);--shadow:0 12px 32px rgba(0,0,0,.14)}
.atl *{box-sizing:border-box}
.atl ::-webkit-scrollbar{width:8px;height:8px}.atl ::-webkit-scrollbar-thumb{background:var(--line2);border-radius:8px}.atl ::-webkit-scrollbar-track{background:transparent}
.atl-ib{display:inline-flex;align-items:center;justify-content:center;width:28px;height:28px;border-radius:7px;color:var(--muted);background:transparent;border:0;cursor:pointer;position:relative;flex:none;transition:background .12s,color .12s}
.atl-ib:hover{background:var(--hover);color:var(--text)}.atl-ib.on{background:var(--accentSoft);color:var(--accent)}.atl-ib:disabled{opacity:.35;cursor:default}
.atl-ib.sm{width:22px;height:22px;border-radius:6px}
.atl-btn{display:inline-flex;align-items:center;gap:6px;height:26px;padding:0 10px;border-radius:999px;border:1px solid var(--line2);background:var(--panel2);color:var(--text);font-size:11.5px;font-weight:500;cursor:pointer;white-space:nowrap}
.atl-btn:hover{border-color:var(--muted)}.atl-btn.primary{background:var(--accent);border-color:var(--accent);color:#fff}.atl-btn.primary:hover{filter:brightness(1.06)}.atl-btn.ghost{background:transparent;border-color:transparent;color:var(--muted)}.atl-btn.ghost:hover{color:var(--text);background:var(--hover)}.atl-btn:disabled{opacity:.4;cursor:default}
.atl-in{height:24px;width:100%;min-width:0;padding:0 7px;border-radius:6px;border:1px solid var(--line);background:var(--input);color:var(--text);font:inherit;font-variant-numeric:tabular-nums;outline:none}
.atl-in:focus{border-color:var(--accent)}select.atl-in{padding-right:4px}
textarea.atl-in{height:auto;padding:6px 7px;resize:vertical}
.atl-row{display:grid;grid-template-columns:minmax(84px,40%) 1fr;align-items:center;gap:8px;min-height:28px;padding:2px 12px}
.atl-row>label{color:var(--muted);font-size:11.5px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.atl-sec{border-top:1px solid var(--line);padding:6px 0 8px}.atl-sec:first-child{border-top:0}
.atl-sech{display:flex;align-items:center;justify-content:space-between;padding:6px 12px 4px;font-size:10.5px;font-weight:600;letter-spacing:.06em;text-transform:uppercase;color:var(--muted)}
.atl-tab{height:30px;padding:0 10px;border:0;background:transparent;color:var(--muted);font:inherit;font-weight:500;cursor:pointer;border-bottom:1.5px solid transparent;white-space:nowrap}
.atl-tab:hover{color:var(--text)}.atl-tab.on{color:var(--text);border-bottom-color:var(--accent)}
.atl-menu{position:absolute;z-index:60;min-width:210px;padding:5px;border-radius:10px;background:var(--panel);border:1px solid var(--line2);box-shadow:var(--shadow)}
.atl-mi{display:flex;align-items:center;gap:8px;width:100%;height:27px;padding:0 9px;border-radius:6px;border:0;background:transparent;color:var(--text);font:inherit;text-align:left;cursor:pointer;white-space:nowrap}
.atl-mi:hover{background:var(--accentSoft)}.atl-mi:disabled{opacity:.38;cursor:default;background:transparent}.atl-mi .k{margin-left:auto;color:var(--faint);font-size:10.5px;padding-left:18px;font-family:ui-monospace,SFMono-Regular,Menlo,monospace}
.atl-msep{height:1px;margin:4px 6px;background:var(--line)}.atl-mh{padding:6px 9px 3px;font-size:10px;font-weight:600;letter-spacing:.06em;text-transform:uppercase;color:var(--faint)}
.atl-list{display:flex;align-items:center;gap:8px;min-height:28px;padding:0 10px 0 12px;cursor:pointer;border-radius:0}
.atl-list:hover{background:var(--hover)}.atl-list.on{background:var(--accentSoft)}
.atl-sw{width:16px;height:16px;border-radius:4px;flex:none;box-shadow:inset 0 0 0 1px rgba(0,0,0,.18)}
.atl-chip{display:inline-flex;align-items:center;height:18px;padding:0 6px;border-radius:999px;background:var(--panel2);color:var(--muted);font-size:10.5px;border:1px solid var(--line)}
[data-tip]:hover::after{content:attr(data-tip);position:absolute;top:calc(100% + 6px);left:50%;transform:translateX(-50%);padding:4px 7px;border-radius:6px;background:#0d0d0f;color:#f2f2f2;font-size:10.5px;font-weight:500;white-space:nowrap;z-index:80;pointer-events:none;box-shadow:0 4px 12px rgba(0,0,0,.3)}
[data-tip-r]:hover::after{content:attr(data-tip-r);position:absolute;left:calc(100% + 8px);top:50%;transform:translateY(-50%);padding:4px 7px;border-radius:6px;background:#0d0d0f;color:#f2f2f2;font-size:10.5px;font-weight:500;white-space:nowrap;z-index:80;pointer-events:none}
.atl-range{-webkit-appearance:none;appearance:none;width:100%;height:16px;background:transparent;cursor:pointer}
.atl-range::-webkit-slider-runnable-track{height:3px;border-radius:3px;background:var(--line2)}.atl-range::-webkit-slider-thumb{-webkit-appearance:none;width:12px;height:12px;border-radius:50%;background:var(--text);margin-top:-4.5px;border:0}
.atl-range::-moz-range-track{height:3px;border-radius:3px;background:var(--line2)}.atl-range::-moz-range-thumb{width:12px;height:12px;border-radius:50%;background:var(--text);border:0}
.atl-kbd{font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:10.5px;padding:1px 5px;border-radius:4px;border:1px solid var(--line2);color:var(--muted)}
.atl-mono{font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-variant-numeric:tabular-nums}
.atl-tbl{width:100%;border-collapse:collapse;font-variant-numeric:tabular-nums}.atl-tbl th{font-size:10px;font-weight:600;letter-spacing:.05em;text-transform:uppercase;color:var(--muted);text-align:left;padding:6px 8px;border-bottom:1px solid var(--line2);white-space:nowrap}.atl-tbl td{padding:6px 8px;border-bottom:1px solid var(--line);vertical-align:top}.atl-tbl .r{text-align:right}
@keyframes atlpulse{0%,100%{opacity:.55}50%{opacity:1}}
@keyframes atlin{from{opacity:0;transform:translateY(4px)}to{opacity:1;transform:none}}
.atl-pop{animation:atlin .14s ease-out}
@media (prefers-reduced-motion: reduce){.atl-pop{animation:none}}
`;
const Icon = AT.Icon;
function IB(props) {
  return <button type="button" className={'atl-ib' + (props.on ? ' on' : '') + (props.sm ? ' sm' : '')} data-tip={props.tipR ? undefined : props.tip} data-tip-r={props.tipR ? props.tip : undefined} onClick={props.onClick} disabled={props.disabled} style={props.style} onPointerDown={(e) => e.stopPropagation()}>
    <Icon name={props.icon} size={props.size || (props.sm ? 15 : 17)} />{props.children}
  </button>;
}
function Row(props) { return <div className="atl-row" title={props.title}><label>{props.label}</label><div style={{ display: 'flex', gap: 6, alignItems: 'center', minWidth: 0 }}>{props.children}</div></div>; }
function Sec(props) {
  const [open, setOpen] = React.useState(props.open !== false);
  return <div className="atl-sec"><div className="atl-sech" onClick={() => setOpen(!open)} style={{ cursor: 'pointer' }}><span>{props.title}</span><span style={{ display: 'flex', alignItems: 'center', gap: 4 }}>{props.right}<Icon name={open ? 'down' : 'chevron'} size={13} /></span></div>{open && props.children}</div>;
}
// number input in mm, displayed in project units
function Num(props) {
  const app = useApp();
  const units = props.raw ? null : app.units;
  const show = (v) => v == null || isNaN(v) ? '' : props.raw ? String(Math.round(v * 1000) / 1000) : String(Math.round(AT.fromMM(v, units) * 100) / 100);
  const [txt, setTxt] = React.useState(show(props.value));
  const focused = React.useRef(false);
  React.useEffect(() => { if (!focused.current) setTxt(show(props.value)); }, [props.value, units]);
  const commit = () => {
    if (props.allowEmpty && String(txt).trim() === '') { if (props.value != null) props.onChange(null); return; }
    const v = props.raw ? parseFloat(txt) : AT.toMM(txt, units);
    if (v == null || isNaN(v)) { setTxt(show(props.value)); return; }
    let vv = v; if (props.min != null) vv = Math.max(props.min, vv); if (props.max != null) vv = Math.min(props.max, vv);
    if (vv !== props.value) props.onChange(vv);
  };
  return <div style={{ position: 'relative', flex: 1, minWidth: 0 }}>
    <input className="atl-in atl-mono" value={txt} placeholder={props.placeholder} disabled={props.disabled} onChange={(e) => setTxt(e.target.value)} onFocus={(e) => { focused.current = true; e.target.select(); props.onFocus && props.onFocus(); }}
      onBlur={() => { focused.current = false; commit(); props.onBlur && props.onBlur(); }} onKeyDown={(e) => { e.stopPropagation(); if (e.key === 'Enter') { e.target.blur(); } if (e.key === 'Escape') { setTxt(show(props.value)); e.target.blur(); } }} style={{ paddingRight: 26 }} />
    <span style={{ position: 'absolute', right: 6, top: 5, color: 'var(--faint)', fontSize: 10.5, pointerEvents: 'none' }}>{props.suffix != null ? props.suffix : props.raw ? '' : AT.unitLabel(units)}</span>
  </div>;
}
function Txt(props) {
  const [v, setV] = React.useState(props.value || '');
  const f = React.useRef(false);
  React.useEffect(() => { if (!f.current) setV(props.value || ''); }, [props.value]);
  const El = props.area ? 'textarea' : 'input';
  return <El className="atl-in" rows={props.rows || 3} value={v} placeholder={props.placeholder} onChange={(e) => setV(e.target.value)} onFocus={() => { f.current = true; }}
    onBlur={() => { f.current = false; if (v !== (props.value || '')) props.onChange(v); }} onKeyDown={(e) => { e.stopPropagation(); if (e.key === 'Enter' && !props.area) e.target.blur(); if (e.key === 'Escape') { setV(props.value || ''); e.target.blur(); } }} />;
}
function Sel(props) {
  return <select className="atl-in" value={props.value == null ? '' : props.value} onChange={(e) => props.onChange(e.target.value)} onKeyDown={(e) => e.stopPropagation()} disabled={props.disabled}>
    {props.options.map((o) => Array.isArray(o) ? <option key={o[0]} value={o[0]}>{o[1]}</option> : <option key={o} value={o}>{o}</option>)}
  </select>;
}
function Color(props) {
  return <label style={{ display: 'flex', alignItems: 'center', gap: 6, flex: 1, minWidth: 0, cursor: 'pointer' }}>
    <span className="atl-sw" style={{ background: props.value, width: 22, height: 22, borderRadius: 6 }} />
    <input type="color" value={props.value || '#cccccc'} onChange={(e) => props.onLive ? props.onLive(e.target.value) : null} onBlur={(e) => props.onChange(e.target.value)} onInput={(e) => props.onLive && props.onLive(e.target.value)} style={{ width: 0, height: 0, opacity: 0, position: 'absolute' }} />
    <span className="atl-mono" style={{ color: 'var(--muted)' }}>{(props.value || '').toUpperCase()}</span>
  </label>;
}
// slider with live preview + commit on release
function Slide(props) {
  const [v, setV] = React.useState(props.value);
  React.useEffect(() => { setV(props.value); }, [props.value]);
  return <div style={{ display: 'flex', alignItems: 'center', gap: 8, flex: 1, minWidth: 0 }}>
    <input type="range" className="atl-range" min={props.min} max={props.max} step={props.step || 1} value={v == null ? 0 : v}
      onChange={(e) => { const n = parseFloat(e.target.value); setV(n); props.onLive && props.onLive(n); }}
      onPointerUp={() => props.onChange(v)} onKeyUp={() => props.onChange(v)} onKeyDown={(e) => e.stopPropagation()} onPointerDown={(e) => e.stopPropagation()} />
    <span className="atl-mono" style={{ width: 34, textAlign: 'right', color: 'var(--muted)' }}>{v == null ? '' : (props.fmt ? props.fmt(v) : v)}</span>
  </div>;
}
function Check(props) {
  return <button type="button" onClick={() => props.onChange(!props.value)} onPointerDown={(e) => e.stopPropagation()} style={{ display: 'inline-flex', alignItems: 'center', gap: 7, background: 'transparent', border: 0, color: 'var(--text)', cursor: 'pointer', padding: 0, font: 'inherit' }}>
    <span style={{ width: 28, height: 16, borderRadius: 999, background: props.value ? 'var(--accent)' : 'var(--line2)', position: 'relative', transition: 'background .15s', flex: 'none' }}>
      <span style={{ position: 'absolute', top: 2, left: props.value ? 14 : 2, width: 12, height: 12, borderRadius: 999, background: '#fff', transition: 'left .15s' }} />
    </span>{props.label}
  </button>;
}
function Seg(props) {
  return <div style={{ display: 'inline-flex', padding: 2, borderRadius: 999, background: 'var(--panel2)', border: '1px solid var(--line)', gap: 2, flexWrap: 'wrap' }}>
    {props.options.map((o) => <button key={o[0]} type="button" onClick={() => props.onChange(o[0])} onPointerDown={(e) => e.stopPropagation()}
      style={{ height: 22, padding: '0 10px', borderRadius: 999, border: 0, background: props.value === o[0] ? 'var(--accent)' : 'transparent', color: props.value === o[0] ? '#fff' : 'var(--muted)', font: 'inherit', fontWeight: 500, cursor: 'pointer', whiteSpace: 'nowrap' }}>{o[1]}</button>)}
  </div>;
}
function Modal(props) {
  return <div onPointerDown={(e) => { e.stopPropagation(); if (e.target === e.currentTarget) props.onClose && props.onClose(); }} style={{ position: 'absolute', inset: 0, zIndex: 70, background: 'rgba(0,0,0,.35)', display: 'flex', alignItems: 'center', justifyContent: 'center', padding: 16 }}>
    <div className="atl-pop" style={{ width: props.width || 520, maxWidth: '100%', maxHeight: '100%', display: 'flex', flexDirection: 'column', background: 'var(--panel)', border: '1px solid var(--line2)', borderRadius: 14, boxShadow: 'var(--shadow)', overflow: 'hidden' }}>
      <div style={{ display: 'flex', alignItems: 'center', padding: '12px 14px', borderBottom: '1px solid var(--line)' }}><div style={{ fontSize: 13, fontWeight: 600 }}>{props.title}</div><div style={{ marginLeft: 'auto' }}><IB icon="close" sm onClick={props.onClose} /></div></div>
      <div style={{ overflow: 'auto', padding: props.pad === false ? 0 : 14 }}>{props.children}</div>
      {props.footer && <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8, padding: '10px 14px', borderTop: '1px solid var(--line)' }}>{props.footer}</div>}
    </div>
  </div>;
}
function MenuList(props) {
  // items: [{label, k, on, disabled, run, sep, head, check}]
  return <div className="atl-menu atl-pop" style={props.style} onPointerDown={(e) => e.stopPropagation()}>
    {props.items.filter(Boolean).map((it, i) => it.sep ? <div key={i} className="atl-msep" /> : it.head ? <div key={i} className="atl-mh">{it.head}</div> :
      <button key={i} className="atl-mi" disabled={it.disabled} onClick={() => { props.onClose && props.onClose(); it.run && it.run(); }}>
        <span style={{ width: 14, color: 'var(--accent)' }}>{it.check ? '✓' : ''}</span>{it.label}{it.k && <span className="k">{it.k}</span>}
      </button>)}
  </div>;
}


function MenuBar() {
  const app = useApp(), A = app.actions, d = app.doc;
  const [open, setOpen] = React.useState(null);
  React.useEffect(() => { if (!open) return; const h = () => setOpen(null); window.addEventListener('pointerdown', h); return () => window.removeEventListener('pointerdown', h); }, [open]);
  const pcSel = app.selPieces.length > 0, sel = app.sel;
  const menus = {
    File: [
      { label: 'New garment…', run: A.newProject }, { label: 'Open…', k: '⌘O', run: () => app.openDialog('open') }, { sep: 1 },
      { label: 'Save version…', k: '⌘S', run: A.saveVersion }, { label: 'Version history…', run: () => app.openDialog('versions') }, { sep: 1 },
      { head: 'Import' }, { label: 'DXF pattern (AAMA / ASTM)…', run: A.importDXF }, { label: 'Atelier project (.json)…', run: A.importJSON }, { sep: 1 },
      { head: 'Export' }, { label: 'DXF pattern (AAMA)', run: A.exportDXF }, { label: 'SVG pattern (1:1)', run: A.exportSVG }, { label: 'OBJ garment (metres)', run: () => A.exportOBJ(false) }, { label: 'OBJ garment + avatar', run: () => A.exportOBJ(true) },
      { label: 'Pattern (PDF)…', run: () => A.openPatternPDF() }, { label: 'Tech pack (PDF)', run: A.exportPDF }, { label: 'Tech pack (CSV)', run: A.exportCSV }, { label: 'Atelier project (.json)', run: A.exportJSON }
    ],
    Edit: [
      { label: 'Undo ' + (app.undoLabel || ''), k: '⌘Z', disabled: !app.undoLabel, run: A.undo }, { label: 'Redo ' + (app.redoLabel || ''), k: '⇧⌘Z', disabled: !app.redoLabel, run: A.redo }, { sep: 1 },
      { label: 'Copy', k: '⌘C', disabled: !pcSel, run: A.copy }, { label: 'Paste', k: '⌘V', run: A.paste }, { label: 'Mirror paste (linked)', k: '⌘R', disabled: !pcSel, run: () => A.mirror(true) }, { label: 'Duplicate', k: '⌘D', disabled: !pcSel, run: A.duplicate },
      { label: 'Delete', k: '⌫', disabled: !sel, run: A.del }, { sep: 1 }, { label: 'Select all patterns', k: '⌘A', run: A.selectAll }
    ],
    '2D Pattern': Object.keys(AT.TOOLS2D).filter((k) => ['transform', 'edit', 'curvature', 'curvept', 'addpoint', 'polygon', 'rect', 'ellipse', 'internal', 'intellipse', 'notch'].indexOf(k) >= 0).map((k) => ({ label: AT.TOOLS2D[k].label, k: AT.TOOLS2D[k].key.toUpperCase(), check: app.tool2d === k, run: () => app.setTool2d(k) }))
      .concat([{ sep: 1 }, { label: 'Symmetric pattern (linked mirror)', disabled: !pcSel, run: () => A.mirror(true) }, { label: 'Flip horizontally', disabled: !pcSel, run: () => A.flip('h') }, { label: 'Flip vertically', disabled: !pcSel, run: () => A.flip('v') }, { label: 'Rotate 90°', disabled: !pcSel, run: () => A.rotate90() }, { sep: 1 }, { label: 'Seam allowance on all patterns…', run: A.saAll }, { label: 'Zoom to fit', k: '⇧Z', run: () => app.editor && app.editor.fit() }]),
    '3D Garment': [
      { label: app.simOn ? 'Stop simulation' : 'Simulate', k: 'Space', run: app.toggleSim }, { sep: 1 },
      { label: 'Reset 3D arrangement (all)', run: () => A.resetArrangement(null) }, { label: 'Reset 3D arrangement (selected)', k: '⌘F', disabled: !pcSel, run: () => A.resetArrangement(app.selPieces) }, { sep: 1 },
      { label: 'Freeze / unfreeze', k: '⌘K', disabled: !pcSel, run: () => A.toggleState('frozen') }, { label: 'Deactivate / activate', k: '⌘J', disabled: !pcSel, run: () => A.toggleState('off') }, { label: 'Strengthen / unstrengthen', k: '⌘H', disabled: !pcSel, run: A.toggleStrengthen },
      { label: 'Hide selected in 3D', k: '⇧Q', disabled: !pcSel, run: A.hide3d }, { label: 'Show all 3D patterns', k: '⇧W', run: A.showAll3d }, { sep: 1 },
      { label: 'Layer +1 (outer)', disabled: !pcSel, run: () => A.layer(1) }, { label: 'Layer −1 (inner)', disabled: !pcSel, run: () => A.layer(-1) }, { sep: 1 },
      { head: 'Simulation preset' }].concat(Object.keys(AT.SIM_PRESETS).map((k) => ({ label: AT.SIM_PRESETS[k].name, check: AT.simSettings(d).preset === k, run: () => A.setSimPreset(k) })), [{ label: 'Simulation settings…', run: () => app.setSel({ kind: 'sim' }) }]),
    Sewing: [{ label: 'Segment sewing', k: 'N', check: app.tool2d === 'segsew', run: () => app.setTool2d('segsew') }, { label: 'Free sewing', k: 'M', check: app.tool2d === 'freesew', run: () => app.setTool2d('freesew') }, { label: 'Edit sewing', k: 'B', check: app.tool2d === 'editsew', run: () => app.setTool2d('editsew') }, { sep: 1 },
      { label: 'Check sewing lengths', run: A.checkSewing }, { label: 'Mark hems automatically', run: A.detectHems }, { label: 'Flip selected sewing', disabled: !(sel && sel.kind === 'sewing'), run: A.flipSewing }, { label: 'Delete all sewing…', run: A.deleteAllSewing }],
    Materials: [{ label: 'Add fabric', run: () => A.addFabric('poplin') }, { label: 'Apply fabric to selected', disabled: !pcSel || !(sel && sel.kind === 'pieces'), run: () => app.setObTab('fabric') }, { sep: 1 },
      { label: 'Segment topstitch', k: 'K', run: () => app.setTool2d('stitch') }, { label: 'Button', run: () => app.setTool2d('button') }, { label: 'Buttonhole', run: () => app.setTool2d('hole') }, { label: 'Fasten button', run: () => app.setTool2d('fasten') }, { label: 'Place graphic', k: 'T', run: () => app.setObTab('graphic') }, { sep: 1 },
      { label: 'Colorways…', run: () => app.setMode('colorway') }],
    Avatar: [{ label: 'Avatar properties…', run: () => app.setSel({ kind: 'avatar' }) }, { label: 'Female avatar', check: d.avatar.sex !== 'male', run: () => A.setAvatarSex('female') }, { label: 'Male avatar', check: d.avatar.sex === 'male', run: () => A.setAvatarSex('male') }, { sep: 1 },
      { label: app.disp3.avatar ? 'Hide avatar' : 'Show avatar', k: '⇧A', run: () => app.setDisp3({ avatar: !app.disp3.avatar }) }, { label: app.disp3.arrPts ? 'Hide arrangement points' : 'Show arrangement points', k: '⇧F', run: () => app.setDisp3({ arrPts: !app.disp3.arrPts }) }, { label: app.disp3.tapes ? 'Hide measurement tapes' : 'Show measurement tapes', run: () => app.setDisp3({ tapes: !app.disp3.tapes }) }, { sep: 1 }, { label: 'Export avatar size…', run: A.exportAvatar }, { label: 'Import avatar size…', run: A.importAvatar }],
    Render: [{ label: 'Snapshot (PNG)', k: 'F10', run: A.snapshot }, { label: 'Turntable video (6 s)', run: () => A.turntable(6) }, { label: 'Render all colorways', run: () => { app.setMode('colorway'); setTimeout(A.renderColorways, 200); } }, { sep: 1 },
      { label: 'Soft shadows', check: app.disp3.shadows !== false, run: () => app.setDisp3({ shadows: app.disp3.shadows === false }) }],
    Display: [{ head: '3D' }, { label: 'Textures', check: app.disp3.textures, run: () => app.setDisp3({ textures: !app.disp3.textures }) }, { label: 'Mesh (wireframe)', check: app.disp3.wire, run: () => app.setDisp3({ wire: !app.disp3.wire }) }, { label: 'X-ray', check: app.disp3.xray, run: () => app.setDisp3({ xray: !app.disp3.xray }) }, { label: 'Sewing lines', check: app.disp3.seams, run: () => app.setDisp3({ seams: !app.disp3.seams }) }, { label: 'Measurement tapes', check: !!app.disp3.tapes, run: () => app.setDisp3({ tapes: !app.disp3.tapes }) },
      { label: 'Stress map', k: '⌥6', check: app.disp3.fit === 'stress', run: () => A.fit('stress') }, { label: 'Strain map', k: '⌥7', check: app.disp3.fit === 'strain', run: () => A.fit('strain') }, { label: 'Fit map', k: '⌥8', check: app.disp3.fit === 'fit', run: () => A.fit('fit') }, { label: 'Pressure points', k: '⌥9', check: app.disp3.fit === 'pressure', run: () => A.fit('pressure') }, { sep: 1 },
      { head: '2D' }].concat([['grid', 'Grid'], ['sa', 'Seam allowance'], ['sewing', 'Sewing lines'], ['lengths', 'Segment lengths'], ['dims', 'Pattern dimensions'], ['texture', '2D textures'], ['names', 'Pattern names'], ['grain', 'Grain lines'], ['comments', 'Comments'], ['grading', 'Grading nest']].map((x) => ({ label: x[1], check: app.disp2[x[0]], run: () => app.setDisp2({ [x[0]]: !app.disp2[x[0]] }) })))
      .concat([{ sep: 1 }, { head: 'Layout' }, { label: '3D + 2D', check: app.layout === 'split', run: () => app.setLayout('split') }, { label: '3D only', check: app.layout === '3d', run: () => app.setLayout('3d') }, { label: '2D only', check: app.layout === '2d', run: () => app.setLayout('2d') }, { label: app.libOpen ? 'Hide library' : 'Show library', k: 'Y', run: () => app.setLibOpen(!app.libOpen) }]),
    Settings: [{ head: 'Units' }].concat([['mm', 'Millimetres'], ['cm', 'Centimetres'], ['in', 'Inches']].map((u) => ({ label: u[1], check: app.units === u[0], run: () => A.setUnits(u[0]) })))
      .concat([{ sep: 1 }, { head: 'Grid snap' }], [[0, 'Off'], [1, '1 mm'], [5, '5 mm'], [10, '10 mm']].map((g) => ({ label: g[1], check: app.snap === g[0], run: () => app.setSnap(g[0]) })), [{ sep: 1 }, { label: 'Self-collision', check: d.sim && d.sim.selfCollision !== false, run: A.toggleSelfCollision }]),
    Help: [{ label: 'Keyboard shortcuts', k: '?', run: () => app.openDialog('shortcuts') }, { label: 'CLO feature coverage', run: () => app.openDialog('coverage') }, { label: 'Quick start', run: () => app.openDialog('help') }]
  };
  const compact = (app.width || 1400) < 1440, collapse = (app.width || 1400) < 1180;
  const MODES = [['sim', 'Simulation'], ['grading', 'Grading'], ['colorway', 'Colorways'], ['techpack', 'Tech Pack'], ['review', 'Review']];
  const menuKeys = collapse ? ['Menu'] : Object.keys(menus);
  if (collapse) menus.Menu = Object.keys(menus).filter((k) => k !== 'Menu').reduce((acc, k) => acc.concat([{ head: k }]).concat(menus[k].filter((it) => !it.head)).concat([{ sep: 1 }]), []);
  return <div style={{ display: 'flex', alignItems: 'center', height: 38, padding: '0 8px', gap: 2, background: 'var(--panel)', borderBottom: '1px solid var(--line)', flex: 'none', position: 'relative', zIndex: 30, minWidth: 0 }}>
    <div style={{ display: 'flex', alignItems: 'center', gap: 7, padding: '0 8px 0 4px', flex: 'none' }}>
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none"><path d="M12 3c2.4 3.6 6 4.8 6 9.4A6 6 0 0 1 12 18.5 6 6 0 0 1 6 12.4C6 7.8 9.6 6.6 12 3z" stroke="var(--accent)" strokeWidth="1.6" /><path d="M12 9v12" stroke="var(--accent)" strokeWidth="1.6" strokeLinecap="round" /></svg>
      <span style={{ fontWeight: 650, letterSpacing: '-.01em', fontSize: 12.5 }}>Atelier</span>
    </div>
    <div style={{ display: 'flex', alignItems: 'center', minWidth: 0, overflow: 'visible', flexWrap: 'nowrap' }}>
      {menuKeys.map((m) => <div key={m} style={{ position: 'relative' }}>
        <button className="atl-tab" style={{ height: 28, borderBottom: 0, borderRadius: 6, background: open === m ? 'var(--hover)' : 'transparent', color: open === m ? 'var(--text)' : undefined }}
          onPointerDown={(e) => { e.stopPropagation(); setOpen(open === m ? null : m); }} onPointerEnter={() => { if (open && open !== m) setOpen(m); }}>{m}</button>
        {open === m && <MenuList items={menus[m]} onClose={() => setOpen(null)} style={{ top: 32, left: 0, maxHeight: 'calc(var(--mana-app-height, 82vh) - 80px)', overflowY: 'auto' }} />}
      </div>)}
    </div>
    <div style={{ flex: 1, minWidth: 8 }} />
    <div style={{ minWidth: 0, flex: '0 1 auto', overflow: 'hidden', display: 'flex', justifyContent: 'center' }}><ProjectTitle /></div>
    <div style={{ flex: 1, minWidth: 8 }} />
    <Presence />
    <div style={{ flex: 'none' }}>{compact ? <div style={{ width: 132 }}><Sel value={app.mode} onChange={app.setMode} options={MODES} /></div> : <Seg value={app.mode} onChange={app.setMode} options={MODES} />}</div>
  </div>;
}
function ProjectTitle() {
  const app = useApp();
  const [edit, setEdit] = React.useState(false);
  if (edit) return <div style={{ width: 220 }}><Txt value={app.doc.meta.name} onChange={(v) => { setEdit(false); app.actions.rename(v); }} /></div>;
  return <button onClick={() => setEdit(true)} style={{ background: 'transparent', border: 0, color: 'var(--text)', font: 'inherit', fontWeight: 600, cursor: 'text', maxWidth: 300, minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }} title="Rename">
    {app.doc.meta.name}<span style={{ color: 'var(--faint)', fontWeight: 400, marginLeft: 8 }}>{app.saving ? 'Saving…' : 'Saved'}</span></button>;
}
function Presence() {
  const app = useApp();
  const peers = (app.peers || []).filter((p) => p && p.name);
  if (!peers.length) return null;
  return <div style={{ display: 'flex', alignItems: 'center', marginRight: 10 }} data-tip={peers.map((p) => p.name + (p.mode ? ' · ' + p.mode : '')).join(', ')}>
    {peers.slice(0, 5).map((p, i) => <div key={i} style={{ width: 22, height: 22, borderRadius: 999, background: p.color || '#888', color: '#fff', fontSize: 10, fontWeight: 600, display: 'flex', alignItems: 'center', justifyContent: 'center', marginLeft: i ? -6 : 0, boxShadow: '0 0 0 2px var(--panel)' }}>{(p.name || '?').slice(0, 1).toUpperCase()}</div>)}
  </div>;
}
function Toolbar3D() {
  const app = useApp(), A = app.actions;
  return <div style={{ display: 'flex', alignItems: 'center', gap: 2, height: 36, padding: '0 6px', borderBottom: '1px solid var(--line)', background: 'var(--panel)', flex: 'none', overflowX: 'auto', overflowY: 'visible' }}>
    <button type="button" onClick={app.toggleSim} onPointerDown={(e) => e.stopPropagation()} data-tip="Simulate (Space)"
      style={{ display: 'inline-flex', alignItems: 'center', gap: 6, height: 26, padding: '0 11px 0 8px', borderRadius: 999, border: 0, cursor: 'pointer', background: app.simOn ? 'var(--accent)' : 'var(--panel2)', color: app.simOn ? '#fff' : 'var(--text)', font: 'inherit', fontWeight: 600, position: 'relative', boxShadow: app.simOn ? '0 0 0 3px var(--accentSoft)' : 'inset 0 0 0 1px var(--line2)', flex: 'none' }}>
      <Icon name={app.simOn ? 'pause' : 'simulate'} size={14} sw={2} />{app.simOn ? 'Simulating' : 'Simulate'}
    </button>
    <div style={{ width: 1, height: 18, background: 'var(--line)', margin: '0 6px' }} />
    <IB icon="select3d" tip="Select / Move (Q)" on={app.tool3d === 'select'} onClick={() => app.setTool3d('select')} />
    <IB icon="pin" tip="Pin (W)" on={app.tool3d === 'pin'} onClick={() => app.setTool3d('pin')} />
    <IB icon="comment" tip="Comment on garment" on={app.tool3d === 'comment'} onClick={() => app.setTool3d('comment')} />
    <div style={{ width: 1, height: 18, background: 'var(--line)', margin: '0 6px' }} />
    <IB icon="arrpts" tip="Arrangement points (Shift+F)" on={app.disp3.arrPts} onClick={() => app.setDisp3({ arrPts: !app.disp3.arrPts })} />
    <IB icon="rotate" tip="Reset 3D arrangement" onClick={() => A.resetArrangement(app.selPieces.length ? app.selPieces : null)} />
    <IB icon="snow" tip="Freeze selected (Ctrl+K)" disabled={!app.selPieces.length} onClick={() => A.toggleState('frozen')} />
    <IB icon="lock" tip="Strengthen selected (Ctrl+H)" disabled={!app.selPieces.length} onClick={A.toggleStrengthen} />
    <div style={{ width: 1, height: 18, background: 'var(--line)', margin: '0 6px' }} />
    <IB icon="camera" tip="Snapshot (F10)" onClick={A.snapshot} />
    <IB icon="video" tip="Turntable video" onClick={() => A.turntable(6)} />
    <div style={{ flex: 1 }} />
  </div>;
}
function ViewBar3D() {
  const app = useApp(), A = app.actions, D = app.disp3;
  const [cam, setCam] = React.useState(false);
  return <div style={{ position: 'absolute', left: 8, top: 8, display: 'flex', flexDirection: 'column', gap: 2, padding: 3, borderRadius: 10, background: 'var(--panel)', border: '1px solid var(--line)', zIndex: 5, boxShadow: '0 4px 14px rgba(0,0,0,.12)' }}>
    <IB tipR icon="avatar" tip="Avatar (Shift+A)" on={D.avatar} onClick={() => app.setDisp3({ avatar: !D.avatar })} />
    <IB tipR icon="texture2d" tip="Textures" on={D.textures} onClick={() => app.setDisp3({ textures: !D.textures })} />
    <IB tipR icon="wire" tip="Mesh" on={D.wire} onClick={() => app.setDisp3({ wire: !D.wire })} />
    <IB tipR icon="xray" tip="X-ray" on={D.xray} onClick={() => app.setDisp3({ xray: !D.xray })} />
    <IB tipR icon="seams" tip="Sewing lines" on={D.seams} onClick={() => app.setDisp3({ seams: !D.seams })} />
    <div style={{ height: 1, background: 'var(--line)', margin: '2px 4px' }} />
    <IB tipR icon="fit" tip="Strain map (Alt+7)" on={D.fit === 'strain'} onClick={() => A.fit('strain')} />
    <IB tipR icon="layers" tip="Stress map (Alt+6)" on={D.fit === 'stress'} onClick={() => A.fit('stress')} />
    <IB tipR icon="ruler" tip="Fit map (Alt+8)" on={D.fit === 'fit'} onClick={() => A.fit('fit')} />
    <IB tipR icon="dot" tip="Pressure points (Alt+9)" on={D.fit === 'pressure'} onClick={() => A.fit('pressure')} />
    <div style={{ height: 1, background: 'var(--line)', margin: '2px 4px' }} />
    <div style={{ position: 'relative' }}>
      <IB tipR icon="camera" tip="Camera views (numpad 2/8/4/6)" on={cam} onClick={() => setCam(!cam)} />
      {cam && <MenuList onClose={() => setCam(false)} style={{ left: 36, top: 0, minWidth: 160 }} items={[['front', 'Front', '2'], ['back', 'Back', '8'], ['left', 'Left', '6'], ['right', 'Right', '4'], ['q3l', '¾ Left', '3'], ['q3r', '¾ Right', '1'], ['top', 'Top', '5'], ['bottom', 'Bottom', '0']].map((v) => ({ label: v[1], k: v[2], run: () => app.scene && app.scene.view(v[0]) }))} />}
    </div>
    <IB tipR icon="focus" tip="Focus (F)" onClick={A.focus3d} />
  </div>;
}
function FitLegend() {
  const app = useApp(), m = app.disp3.fit;
  if (!m) return null;
  const sum = app.fitSummary;
  const scale = m === 'strain' ? ['100%', '105%', '110%', '115%', '120%'] : m === 'stress' ? ['0', '25', '50', '75', '100 kPa'] : null;
  return <div style={{ position: 'absolute', right: 10, bottom: 10, padding: '8px 10px', borderRadius: 10, background: 'var(--panel)', border: '1px solid var(--line)', zIndex: 5, minWidth: 170 }}>
    <div style={{ fontSize: 10.5, fontWeight: 600, textTransform: 'uppercase', letterSpacing: '.06em', color: 'var(--muted)', marginBottom: 6 }}>{{ strain: 'Strain map', stress: 'Stress map', fit: 'Fit map', pressure: 'Pressure points' }[m]}</div>
    {scale && <><div style={{ height: 8, borderRadius: 4, background: 'linear-gradient(90deg,' + AT.FIT_COLORS.join(',') + ')' }} /><div style={{ display: 'flex', justifyContent: 'space-between', marginTop: 3 }} className="atl-mono">{scale.map((s) => <span key={s} style={{ fontSize: 9.5, color: 'var(--muted)' }}>{s}</span>)}</div></>}
    {m === 'fit' && <div style={{ display: 'grid', gap: 4 }}>{[['#d9262c', "Can't wear", sum && sum.cant], ['#f5c931', 'Tight', sum && sum.tight]].map((r) => <div key={r[1]} style={{ display: 'flex', alignItems: 'center', gap: 6 }}><span className="atl-sw" style={{ background: r[0], width: 12, height: 12 }} />{r[1]}<span className="atl-mono" style={{ marginLeft: 'auto', color: 'var(--muted)' }}>{r[2] != null ? (r[2] * 100).toFixed(1) + '%' : '–'}</span></div>)}</div>}
    {m === 'pressure' && <div style={{ color: 'var(--muted)' }}>Red = garment touching the avatar</div>}
  </div>;
}
function Toolbar2D() {
  const app = useApp();
  const groups = [['transform', 'edit', 'curvature', 'curvept', 'addpoint'], ['polygon', 'rect', 'ellipse', 'internal', 'intellipse', 'notch'], ['segsew', 'freesew', 'editsew'], ['stitch', 'editstitch', 'button', 'hole', 'fasten', 'graphic'], ['comment', 'measure']];
  return <div style={{ display: 'flex', alignItems: 'center', gap: 2, height: 36, padding: '0 6px', borderBottom: '1px solid var(--line)', background: 'var(--panel)', flex: 'none', overflowX: 'auto' }}>
    {groups.map((g, gi) => <React.Fragment key={gi}>{gi > 0 && <div style={{ width: 1, height: 18, background: 'var(--line)', margin: '0 5px', flex: 'none' }} />}
      {g.map((k) => { const t = AT.TOOLS2D[k]; return <IB key={k} icon={t.icon} tip={t.label + (t.key ? ' (' + t.key.toUpperCase() + ')' : '')} on={app.tool2d === k} onClick={() => app.setTool2d(k)} />; })}
    </React.Fragment>)}
    <div style={{ flex: 1 }} />
    {app.doc.sizes.length > 1 && <div style={{ width: 70, flex: 'none' }}><Sel value={app.doc.activeSize} options={app.doc.sizes} onChange={(v) => app.actions.setActiveSize(v)} /></div>}
  </div>;
}
function ViewBar2D() {
  const app = useApp(), D = app.disp2;
  const t = (k, icon, tip) => <IB tipR icon={icon} tip={tip} on={D[k]} onClick={() => app.setDisp2({ [k]: !D[k] })} />;
  return <div style={{ position: 'absolute', left: 8, top: 8, display: 'flex', flexDirection: 'column', gap: 2, padding: 3, borderRadius: 10, background: 'var(--panel)', border: '1px solid var(--line)', zIndex: 5, boxShadow: '0 4px 14px rgba(0,0,0,.12)' }}>
    {t('grid', 'grid', 'Grid')}{t('sa', 'sa', 'Seam allowance')}{t('sewing', 'segsew', 'Sewing lines')}{t('lengths', 'len', 'Segment lengths')}{t('dims', 'dims', 'Pattern dimensions')}{t('texture', 'texture2d', '2D textures')}{t('grading', 'layers', 'Grading nest')}{t('comments', 'comment', 'Comments')}
    <div style={{ height: 1, background: 'var(--line)', margin: '2px 4px' }} />
    <IB tipR icon="focus" tip="Zoom to fit" onClick={() => app.editor && app.editor.fit()} />
    <IB tipR icon="pdf" tip="Export pattern PDF" onClick={() => app.actions.openPatternPDF()} />
  </div>;
}
function LibraryPanel() {
  const app = useApp(), A = app.actions, d = app.doc;
  const [sec, setSec] = React.useState('garments');
  const chart = AT.SIZE_CHARTS[d.avatar.sex === 'male' ? 'male' : 'female'];
  return <div style={{ display: 'flex', flexDirection: 'column', height: '100%', minHeight: 0 }}>
    <div style={{ display: 'flex', alignItems: 'center', height: 36, padding: '0 6px 0 12px', borderBottom: '1px solid var(--line)' }}><span style={{ fontWeight: 600 }}>Library</span><div style={{ marginLeft: 'auto' }}><IB icon="close" sm tip="Hide (Y)" onClick={() => app.setLibOpen(false)} /></div></div>
    <div style={{ display: 'flex', gap: 0, borderBottom: '1px solid var(--line)', padding: '0 4px', overflowX: 'auto' }}>
      {[['garments', 'Garments'], ['avatars', 'Avatars'], ['fabrics', 'Fabrics'], ['trims', 'Trims'], ['projects', 'Projects']].map((s) => <button key={s[0]} className={'atl-tab' + (sec === s[0] ? ' on' : '')} onClick={() => setSec(s[0])}>{s[1]}</button>)}
    </div>
    <div style={{ flex: 1, overflowY: 'auto', padding: '8px 0' }}>
      {sec === 'garments' && AT.TEMPLATES.filter((t) => t.fn).map((t) => <div key={t.id} style={{ padding: '8px 12px', borderBottom: '1px solid var(--line)' }}>
        <div style={{ fontWeight: 600 }}>{t.name}</div><div style={{ color: 'var(--muted)', margin: '2px 0 6px' }}>{t.desc}</div>
        <div style={{ display: 'flex', gap: 6 }}><button className="atl-btn" onClick={() => A.addTemplate(t.id)}>Add to board</button><button className="atl-btn ghost" onClick={() => A.newFromTemplate(t.id)}>Open as new</button></div>
      </div>)}
      {sec === 'avatars' && <div>
        <div className="atl-sech">Body</div>
        {['female', 'male'].map((sx) => <div key={sx} className={'atl-list' + (d.avatar.sex === sx || (sx === 'female' && d.avatar.sex !== 'male') ? ' on' : '')} onClick={() => A.setAvatarSex(sx)}><Icon name="avatar" size={16} />{sx === 'female' ? 'Female — 170 cm' : 'Male — 180 cm'}</div>)}
        <div style={{ padding: '4px 12px', color: 'var(--faint)', fontSize: 10.5 }}>MakeHuman body (CC0)</div>
        <div className="atl-sech" style={{ marginTop: 8 }}>Size chart</div>
        {chart.map((c) => <div key={c.name} className="atl-list" onClick={() => A.applySizeChart(c)}><span className="atl-chip">{c.name}</span><span className="atl-mono" style={{ color: 'var(--muted)' }}>B {c.bust / 10} · W {c.waist / 10} · H {c.hip / 10}</span></div>)}
        {Object.keys(app.avSizes || {}).length > 0 && <div className="atl-sech" style={{ marginTop: 8 }}>Saved sizes</div>}
        {Object.entries(app.avSizes || {}).map(([k, v]) => <div key={k} className="atl-list" onClick={() => A.applyAvatarSize(Object.assign({ id: k }, v))}><Icon name="avatar" size={14} />{v.name}</div>)}
        <div style={{ padding: '10px 12px' }}><button className="atl-btn" onClick={() => app.setSel({ kind: 'avatar' })}>Edit measurements…</button></div>
      </div>}
      {sec === 'fabrics' && Object.keys(AT.FABRIC_PRESETS).map((k) => { const f = AT.FABRIC_PRESETS[k]; return <div key={k} className="atl-list" onClick={() => A.addFabric(k)} title="Add to project (applies to selected patterns)">
        <span className="atl-sw" style={{ background: '#d8d3c8' }} /><div style={{ minWidth: 0 }}><div>{f.name}</div><div style={{ color: 'var(--faint)', fontSize: 10.5 }}>{f.phys.density} g/m² · stretch {f.phys.stretchWeft}/{f.phys.stretchWarp} · bend {f.phys.bendWeft}</div></div></div>; })}
      {sec === 'trims' && <div>
        <div className="atl-sech">Buttons</div>
        {[['Round 4-hole 15 mm', { shape: 'round', size: 15, holes: 4, color: '#2b2b2b', finish: 'plastic' }], ['Round 2-hole 11 mm', { shape: 'round', size: 11, holes: 2, color: '#e9e4da', finish: 'plastic' }], ['Metal shank 18 mm', { shape: 'shank', size: 18, holes: 0, color: '#b8a16a', finish: 'metal' }], ['Square 20 mm', { shape: 'square', size: 20, holes: 4, color: '#3d2b1f', finish: 'plastic' }], ['Snap 12 mm', { shape: 'round', size: 12, holes: 0, color: '#c0c0c0', finish: 'metal' }]].map((b) =>
          <div key={b[0]} className="atl-list" onClick={() => { app.setButtonPreset(b[1]); app.setTool2d('button'); app.toast('Click on a pattern to place: ' + b[0]); }}><span className="atl-sw" style={{ background: b[1].color, borderRadius: 999 }} />{b[0]}</div>)}
        <div className="atl-sech" style={{ marginTop: 8 }}>Buttonholes</div>
        <div className="atl-list" onClick={() => app.setTool2d('hole')}><Icon name="hole" size={16} />Keyhole 18 mm</div>
        <div className="atl-sech" style={{ marginTop: 8 }}>Topstitch</div>
        <div className="atl-list" onClick={() => app.setTool2d('stitch')}><Icon name="stitch" size={16} />Single needle 3 mm</div>
      </div>}
      {sec === 'projects' && <ProjectList compact />}
    </div>
  </div>;
}
function ProjectList(props) {
  const app = useApp();
  const items = Object.entries(app.index || {}).map(([k, v]) => Object.assign({ id: k }, v)).sort((a, b) => (b.updatedAt || 0) - (a.updatedAt || 0));
  if (!items.length) return <div style={{ padding: 12, color: 'var(--muted)' }}>No projects yet.</div>;
  return <div>{items.map((p) => <div key={p.id} className={'atl-list' + (p.id === app.pid ? ' on' : '')} style={{ minHeight: props.compact ? 34 : 44 }} onClick={() => app.actions.openProject(p.id)}>
    <Icon name="doc" size={16} /><div style={{ minWidth: 0, flex: 1 }}><div style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', fontWeight: 500 }}>{p.name}</div><div style={{ color: 'var(--faint)', fontSize: 10.5 }}>{p.pieces || 0} pieces · {p.updatedAt ? new Date(p.updatedAt).toLocaleString() : ''}</div></div>
    {!props.compact && <IB icon="trash" sm tip="Delete project" onClick={(e) => { e.stopPropagation(); app.actions.deleteProject(p.id); }} />}
  </div>)}</div>;
}
function ObjectBrowser() {
  const app = useApp(), d = app.doc, A = app.actions, sel = app.sel || {}, tab = app.obTab;
  const tabs = [['fabric', 'Fabric'], ['pattern', 'Pattern'], ['sewing', 'Sewing'], ['trims', 'Trims'], ['stitch', 'Topstitch'], ['graphic', 'Graphic'], ['pom', 'POM']];
  const fabUse = {}; AT.asArray(d.pieces).forEach((pc) => { fabUse[pc.fabric] = (fabUse[pc.fabric] || 0) + 1; });
  const lens = tab === 'sewing' ? AT.sewLengths(d) : null, seamNo = tab === 'sewing' ? AT.seamNumbers(d) : null;
  const fileRef = React.useRef(null);
  return <div style={{ display: 'flex', flexDirection: 'column', height: '100%', minHeight: 0 }}>
    <div style={{ display: 'flex', borderBottom: '1px solid var(--line)', padding: '0 4px', overflowX: 'auto', flex: 'none' }}>{tabs.map((t) => <button key={t[0]} className={'atl-tab' + (tab === t[0] ? ' on' : '')} onClick={() => app.setObTab(t[0])}>{t[1]}</button>)}</div>
    <div style={{ flex: 1, overflowY: 'auto', padding: '4px 0' }}>
      {tab === 'fabric' && <>
        {Object.values(d.fabrics).map((f) => { const ef = AT.effFabric(d, f.id); return <div key={f.id} className={'atl-list' + (sel.kind === 'fabric' && sel.id === f.id ? ' on' : '')} onClick={() => app.setSel({ kind: 'fabric', id: f.id })}
          onDoubleClick={() => A.applyFabric(f.id)}><span className="atl-sw" style={{ background: ef.color }} /><span style={{ flex: 1, minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{f.name}</span><span className="atl-chip">{fabUse[f.id] || 0}</span>
          {app.selPieces.length > 0 && <button className="atl-btn ghost" style={{ height: 20, padding: '0 6px' }} onClick={(e) => { e.stopPropagation(); A.applyFabric(f.id); }}>Apply</button>}</div>; })}
        <div style={{ display: 'flex', gap: 6, padding: '8px 12px' }}><button className="atl-btn" onClick={() => A.addFabric('poplin')}><Icon name="plus" size={13} />Add</button>{sel.kind === 'fabric' && <><button className="atl-btn ghost" onClick={() => A.dupFabric(sel.id)}>Duplicate</button><button className="atl-btn ghost" disabled={fabUse[sel.id] > 0} onClick={() => A.delFabric(sel.id)}>Delete</button></>}</div>
      </>}
      {tab === 'pattern' && AT.asArray(d.pieces).sort((a, b) => (a.order || 0) - (b.order || 0)).map((pc) => <div key={pc.id} className={'atl-list' + (app.selPieces.indexOf(pc.id) >= 0 ? ' on' : '')} onClick={(e) => app.setSel({ kind: 'pieces', ids: e.shiftKey ? Array.from(new Set(app.selPieces.concat([pc.id]))) : [pc.id] })}>
        <span className="atl-sw" style={{ background: (AT.effFabric(d, pc.fabric) || {}).color }} /><span style={{ flex: 1, minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{pc.name}</span>
        <span className="atl-mono" title="Net width × length" style={{ color: 'var(--faint)', fontSize: 10.5, flex: 'none' }}>{(() => { const dm = AT.pieceDims(d, pc, d.activeSize); return AT.fmt(dm.w, app.units) + '×' + AT.fmt(dm.h, app.units); })()}</span>
        {pc.mirror && <span className="atl-chip">linked</span>}{pc.state === 'frozen' && <Icon name="snow" size={13} />}{pc.state === 'off' && <span className="atl-chip">off</span>}{(pc.layer || 0) !== 0 && <span className="atl-chip">L{pc.layer}</span>}
        <IB sm icon={pc.hide3d ? 'eyeoff' : 'eye'} tip="Show in 3D" onClick={(e) => { e.stopPropagation(); A.patchPiece(pc.id, { hide3d: !pc.hide3d }, 'Toggle 3D visibility'); }} />
      </div>)}
      {tab === 'sewing' && Object.values(d.sewing).map((sw, i) => { const L = lens[sw.id] || {}; const diff = Math.abs((L.a || 0) - (L.b || 0)); const nm = (ss) => ss.map((s) => (d.pieces[s.p] || {}).name).join('+'); return <div key={sw.id} className={'atl-list' + (sel.kind === 'sewing' && sel.id === sw.id ? ' on' : '')} onClick={() => app.setSel({ kind: 'sewing', id: sw.id })}>
        <span className="atl-mono" style={{ minWidth: 20, height: 18, padding: '0 4px', borderRadius: 999, background: sw.color, color: '#fff', fontSize: 10.5, fontWeight: 600, display: 'inline-flex', alignItems: 'center', justifyContent: 'center', flex: 'none' }}>{seamNo[sw.id]}</span><span style={{ flex: 1, minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{nm(sw.a)} ↔ {nm(sw.b)}{(sw.a.length > 1 || sw.b.length > 1) && <span className="atl-chip" style={{ marginLeft: 6 }}>{sw.a.length}:{sw.b.length}</span>}</span>
        <span className="atl-mono" style={{ color: diff > 3 ? 'var(--accent)' : 'var(--faint)' }}>{diff > 0.5 ? 'Δ' + AT.fmt(diff, app.units) : '='}</span></div>; })}
      {tab === 'trims' && <>
        <div className="atl-sech">Buttons</div>
        {Object.values(d.buttons).map((b) => <div key={b.id} className={'atl-list' + (sel.kind === 'button' && sel.id === b.id ? ' on' : '')} onClick={() => app.setSel({ kind: 'button', id: b.id })}><span className="atl-sw" style={{ background: b.color, borderRadius: 999 }} />{b.shape} {b.size} mm<span style={{ color: 'var(--faint)', marginLeft: 'auto' }}>{(d.pieces[b.p] || {}).name}{b.fasten ? ' · fastened' : ''}</span></div>)}
        <div className="atl-sech">Buttonholes</div>
        {Object.values(d.holes).map((h) => <div key={h.id} className={'atl-list' + (sel.kind === 'hole' && sel.id === h.id ? ' on' : '')} onClick={() => app.setSel({ kind: 'hole', id: h.id })}><Icon name="hole" size={15} />{h.len} mm<span style={{ color: 'var(--faint)', marginLeft: 'auto' }}>{(d.pieces[h.p] || {}).name}</span></div>)}
        <div style={{ display: 'flex', gap: 6, padding: '8px 12px' }}><button className="atl-btn" onClick={() => app.setTool2d('button')}>Place button</button><button className="atl-btn ghost" onClick={() => app.setTool2d('hole')}>Buttonhole</button><button className="atl-btn ghost" onClick={() => app.setTool2d('fasten')}>Fasten</button></div>
      </>}
      {tab === 'stitch' && <>{Object.values(d.stitches).map((st) => <div key={st.id} className={'atl-list' + (sel.kind === 'stitch' && sel.id === st.id ? ' on' : '')} onClick={() => app.setSel({ kind: 'stitch', id: st.id })}><span style={{ width: 18, borderTop: '2px dashed ' + st.color }} />{(d.pieces[st.p] || {}).name} · {st.seg === '*' ? 'outline' : 'segment'}<span className="atl-mono" style={{ marginLeft: 'auto', color: 'var(--faint)' }}>{st.rows || 1}× {st.off}mm</span></div>)}
        <div style={{ padding: '8px 12px', color: 'var(--muted)' }}>Use <span className="atl-kbd">K</span> and click a segment (Shift = whole outline).</div></>}
      {tab === 'graphic' && <div style={{ padding: '8px 12px' }}>
        <input ref={fileRef} type="file" accept="image/*" style={{ display: 'none' }} onChange={(e) => { const f = e.target.files && e.target.files[0]; e.target.value = ''; if (f) A.uploadGraphic(f); }} />
        <button className="atl-btn primary" onClick={() => fileRef.current && fileRef.current.click()}><Icon name="upload" size={13} />Upload graphic</button>
        {app.pendingGraphic && <div style={{ marginTop: 8, display: 'flex', gap: 8, alignItems: 'center' }}><img src={AT.resolveAsset(app.pendingGraphic)} style={{ width: 40, height: 40, objectFit: 'contain', background: 'var(--panel2)', borderRadius: 6 }} /><span style={{ color: 'var(--muted)' }}>Click on a pattern to place</span></div>}
        <div className="atl-sech" style={{ padding: '12px 0 4px' }}>Placed</div>
        {AT.asArray(d.pieces).flatMap((pc) => (pc.graphics || []).map((g) => <div key={g.id} className={'atl-list' + (sel.kind === 'graphic' && sel.id === g.id ? ' on' : '')} style={{ padding: '0 4px' }} onClick={() => { app.setSel({ kind: 'graphic', piece: pc.id, id: g.id }); app.setTool2d('graphic'); }}><img src={AT.resolveAsset(g)} style={{ width: 22, height: 22, objectFit: 'contain' }} />{g.name || 'Graphic'}<span style={{ marginLeft: 'auto', color: 'var(--faint)' }}>{pc.name}</span></div>))}
      </div>}
      {tab === 'pom' && <>
        {Object.values(d.poms).map((pm) => <div key={pm.id} className={'atl-list' + (sel.kind === 'pom' && sel.id === pm.id ? ' on' : '')} onClick={() => app.setSel({ kind: 'pom', id: pm.id })}><Icon name="ruler" size={15} /><span style={{ flex: 1 }}>{pm.name}</span><span className="atl-mono">{AT.fmt(AT.pomValue(d, pm, d.activeSize), app.units)}</span></div>)}
        <div style={{ padding: '8px 12px', display: 'flex', gap: 6, flexWrap: 'wrap' }}><button className="atl-btn" disabled={sel.kind !== 'segment'} onClick={A.addPOM}><Icon name="plus" size={13} />POM from selected segment</button></div>
        <div style={{ padding: '0 12px', color: 'var(--faint)' }}>Select a segment with <span className="atl-kbd">Z</span>, then add. POMs are graded across all sizes in the tech pack.</div>
      </>}
    </div>
  </div>;
}


function PropertyEditor() {
  const app = useApp(), d = app.doc, sel = app.sel || {}, A = app.actions;
  const body = (() => {
    if (sel.kind === 'pieces' && sel.ids.length) return <PiecePanel ids={sel.ids} />;
    if (sel.kind === 'points') return <PointPanel />;
    if (sel.kind === 'segment') return <SegmentPanel />;
    if (sel.kind === 'internal') return <InternalPanel />;
    if (sel.kind === 'sewing') return <SewingPanel />;
    if (sel.kind === 'fabric') return <FabricPanel id={sel.id} />;
    if (sel.kind === 'button') return <ButtonPanel />;
    if (sel.kind === 'hole') return <HolePanel />;
    if (sel.kind === 'stitch') return <StitchPanel />;
    if (sel.kind === 'graphic') return <GraphicPanel />;
    if (sel.kind === 'pom') return <PomPanel />;
    if (sel.kind === 'comment') return <CommentPanel id={sel.id} />;
    if (sel.kind === 'avatar') return <AvatarPanel />;
    if (sel.kind === 'sim') return <SimPanel />;
    return <ProjectPanel />;
  })();
  const title = { sim: 'Simulation', pieces: 'Pattern', points: 'Point', segment: 'Segment', internal: 'Internal line', sewing: 'Sewing', fabric: 'Fabric', button: 'Button', hole: 'Buttonhole', stitch: 'Topstitch', graphic: 'Graphic', pom: 'Point of measure', comment: 'Comment', avatar: 'Avatar' }[sel.kind] || 'Project';
  return <div style={{ display: 'flex', flexDirection: 'column', height: '100%', minHeight: 0 }}>
    <div style={{ display: 'flex', alignItems: 'center', height: 32, padding: '0 8px 0 12px', borderBottom: '1px solid var(--line)', flex: 'none' }}>
      <span style={{ fontWeight: 600 }}>Property Editor</span><span style={{ color: 'var(--muted)', marginLeft: 8 }}>{title}</span>
      {sel.kind && <div style={{ marginLeft: 'auto' }}><IB sm icon="close" tip="Clear selection" onClick={() => app.setSel(null)} /></div>}
    </div>
    <div style={{ flex: 1, overflowY: 'auto', paddingBottom: 12 }}>{body}</div>
  </div>;
}
function PiecePanel(props) {
  const app = useApp(), d = app.doc, A = app.actions;
  const pcs = props.ids.map((i) => d.pieces[i]).filter(Boolean);
  if (!pcs.length) return null;
  const pc = pcs[0], multi = pcs.length > 1;
  const setAll = (patch, label) => A.patchPieces(props.ids, patch, label);
  const setArr = (patch, label) => { props.ids.forEach((i) => { const p = d.pieces[i]; if (p) p.arr = Object.assign({}, p.arr || {}, patch); }); A.commit(label || 'Arrangement'); A.resetArrangement(props.ids, true); };
  const ol = AT.outline(AT.gradedPts(pc, d.activeSize, d.baseSize));
  const sl = AT.slotById((pc.arr || {}).slot);
  return <>
    <Sec title={multi ? pcs.length + ' patterns' : 'Pattern'}>
      {!multi && <Row label="Name"><Txt value={pc.name} onChange={(v) => setAll({ name: v }, 'Rename')} /></Row>}
      <Row label="Fabric"><Sel value={pc.fabric} options={Object.values(d.fabrics).map((f) => [f.id, f.name])} onChange={(v) => setAll({ fabric: v }, 'Assign fabric')} /></Row>
      <Row label="Quantity"><Num raw value={pc.qty || 1} min={1} max={20} onChange={(v) => setAll({ qty: Math.round(v) }, 'Quantity')} /></Row>
      <Row label="Grain line"><Slide min={-90} max={90} value={pc.grain || 0} fmt={(v) => v + '°'} onChange={(v) => setAll({ grain: v }, 'Grain')} /></Row>
      <Row label="Seam allowance"><Num value={pc.sa || 0} min={0} max={80} onChange={(v) => setAll({ sa: v }, 'Seam allowance')} /></Row>
      {pc.mirror && <Row label="Linked"><span style={{ color: 'var(--muted)' }}>Mirror of {(d.pieces[pc.mirror.src] || {}).name}</span><button className="atl-btn ghost" onClick={() => setAll({ mirror: null }, 'Unlink mirror')}>Unlink</button></Row>}
    </Sec>
    <Sec title="Dimensions">
      {!multi ? (() => { const dm = AT.pieceDims(d, pc, d.activeSize), f = (v) => AT.fmtU(v, app.units); return <>
        <Row label="Width"><span className="atl-mono">{f(dm.w)}</span></Row>
        <Row label="Length"><span className="atl-mono">{f(dm.h)}</span></Row>
        <Row label="Cut size" title="Including seam allowance"><span className="atl-mono">{f(dm.cw)} × {f(dm.ch)}</span></Row>
        <Row label="Perimeter"><span className="atl-mono">{f(dm.perim)}</span></Row>
        <Row label="Area"><span className="atl-mono">{AT.fmtArea(dm.area, app.units)}</span><span style={{ color: 'var(--faint)' }}>cut {AT.fmtArea(dm.cutArea, app.units)}</span></Row>
        <Row label="Points / segments"><span className="atl-mono">{pc.pts.length} / {ol.segs.length}</span></Row>
      </>; })() : pcs.map((p) => { const dm = AT.pieceDims(d, p, d.activeSize); return <Row key={p.id} label={p.name}><span className="atl-mono">{AT.fmtU(dm.w, app.units)} × {AT.fmtU(dm.h, app.units)}</span></Row>; })}
      <div style={{ padding: '4px 12px', color: 'var(--faint)', lineHeight: 1.45 }}>Net size along the sewing line, size {d.activeSize}.</div>
      <div style={{ padding: '4px 12px 2px' }}><button className="atl-btn" onClick={() => A.openPatternPDF(props.ids)}><Icon name="pdf" size={13} />Export PDF…</button></div>
    </Sec>
    <Sec title="Simulation">
      <Row label="Particle distance"><Num value={pc.pd || 20} min={3} max={60} onChange={(v) => setAll({ pd: v }, 'Particle distance')} /></Row>
      <Row label="State"><Seg value={pc.state || 'active'} onChange={(v) => setAll({ state: v }, 'Pattern state')} options={[['active', 'Active'], ['frozen', 'Frozen'], ['off', 'Deactivated']]} /></Row>
      <Row label="Strengthen"><Check value={!!pc.strengthen} onChange={(v) => setAll({ strengthen: v }, 'Strengthen')} /></Row>
      <Row label="Layer"><Num raw value={pc.layer || 0} min={-2} max={6} onChange={(v) => setAll({ layer: Math.round(v) }, 'Layer')} /></Row>
      <Row label="Shrinkage weft"><Slide min={70} max={130} value={pc.shrinkWeft || 100} fmt={(v) => v + '%'} onChange={(v) => setAll({ shrinkWeft: v }, 'Shrinkage')} /></Row>
      <Row label="Shrinkage warp"><Slide min={70} max={130} value={pc.shrinkWarp || 100} fmt={(v) => v + '%'} onChange={(v) => setAll({ shrinkWarp: v }, 'Shrinkage')} /></Row>
      <Row label="Pressure"><Slide min={-20} max={60} value={pc.pressure || 0} onChange={(v) => setAll({ pressure: v }, 'Pressure')} /></Row>
      <Row label="Show in 3D"><Check value={!pc.hide3d} onChange={(v) => setAll({ hide3d: !v }, '3D visibility')} /></Row>
      <Row label="Pins"><span className="atl-mono" style={{ color: 'var(--muted)' }}>{(pc.pins || []).length}</span>{(pc.pins || []).length > 0 && <button className="atl-btn ghost" onClick={() => setAll({ pins: [] }, 'Clear pins')}>Clear</button>}</Row>
    </Sec>
    <Sec title="Arrangement">
      <Row label="Position"><Sel value={(pc.arr || {}).slot || 'front'} options={AT.SLOTS.map((s) => [s.id, s.label])} onChange={(v) => setArr({ slot: v, ang: null, t: null, shift: null }, 'Arrange')} /></Row>
      {sl.kind === 'body' || sl.kind === 'neck' ? <Row label="Height"><Num value={(pc.arr || {}).h != null ? pc.arr.h : 1100} onChange={(v) => setArr({ h: v })} /></Row> : <Row label="Along limb"><Num value={(pc.arr || {}).t || 0} onChange={(v) => setArr({ t: v })} /></Row>}
      <Row label="Angle"><Slide min={-180} max={180} value={(pc.arr || {}).ang != null ? pc.arr.ang : sl.ang} fmt={(v) => v + '°'} onChange={(v) => setArr({ ang: v })} /></Row>
      <Row label="Offset"><Num value={(pc.arr || {}).off == null ? 60 : pc.arr.off} min={0} max={800} onChange={(v) => setArr({ off: v })} /></Row>
      {sl.kind === 'body' && <Row label="Shift"><Num value={(pc.arr || {}).shift || 0} min={-1500} max={1500} onChange={(v) => setArr({ shift: v || null })} /></Row>}
      <Row label="Wrap"><Check value={!(pc.arr || {}).flat} label="Curve around body" onChange={(v) => setArr({ flat: !v })} /></Row>
      <Row label="Flip"><Check value={!!(pc.arr || {}).flip} label="Inside out" onChange={(v) => setArr({ flip: v })} /></Row>
      <div style={{ padding: '4px 12px', color: 'var(--faint)' }}>Tip: show arrangement points <span className="atl-kbd">⇧F</span> and click one in 3D with this pattern selected.</div>
    </Sec>
    <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap', padding: '8px 12px' }}>
      <button className="atl-btn" onClick={() => A.mirror(true)}><Icon name="mirror" size={13} />Symmetric</button>
      <button className="atl-btn" onClick={A.duplicate}>Duplicate</button>
      <button className="atl-btn ghost" onClick={() => A.flip('h')}>Flip H</button>
      <button className="atl-btn ghost" onClick={() => A.del()}><Icon name="trash" size={13} />Delete</button>
    </div>
  </>;
}
function PointPanel() {
  const app = useApp(), d = app.doc, A = app.actions, sel = app.sel;
  const pc = d.pieces[sel.piece]; if (!pc) return null;
  const pts = sel.ids.map((id) => pc.pts.find((p) => p.id === id)).filter(Boolean);
  if (!pts.length) return null;
  const p = pts[0];
  const setP = (patch) => { const old = AT.clone(pc.pts); pts.forEach((q) => Object.assign(q, patch)); if ('c' in patch) { pts.forEach((q) => { if (!q.c) delete q.c; }); AT.remapRefs(d, pc.id, old); } A.commit('Edit point'); };
  return <>
    <Sec title={pts.length > 1 ? pts.length + ' points' : 'Point'}>
      {pts.length === 1 && <><Row label="X"><Num value={p.x} onChange={(v) => setP({ x: v })} /></Row><Row label="Y"><Num value={p.y} onChange={(v) => setP({ y: v })} /></Row></>}
      <Row label="Type"><Seg value={p.c ? 'curve' : 'corner'} onChange={(v) => setP({ c: v === 'curve' ? 1 : 0 })} options={[['corner', 'Corner'], ['curve', 'Curve point']]} /></Row>
      <div style={{ padding: '6px 12px' }}><button className="atl-btn ghost" onClick={A.del}><Icon name="trash" size={13} />Delete point{pts.length > 1 ? 's' : ''}</button></div>
    </Sec>
    {d.sizes.length > 1 && pts.length === 1 && <GradeRule pc={pc} pid={p.id} />}
  </>;
}
function GradeRule(props) {
  const app = useApp(), d = app.doc, A = app.actions, pc = props.pc;
  const r = (pc.grade && pc.grade[props.pid]) || {};
  const set = (sz, k, v) => { pc.grade = pc.grade || {}; const g = pc.grade[props.pid] = pc.grade[props.pid] || {}; g[sz] = Object.assign({ dx: 0, dy: 0 }, g[sz] || {}, { [k]: v }); A.commit('Grade rule'); };
  return <Sec title="Grade rule (Δ from base)">
    <div style={{ display: 'grid', gridTemplateColumns: '54px 1fr 1fr', gap: 6, padding: '2px 12px', alignItems: 'center' }}>
      <span style={{ color: 'var(--faint)', fontSize: 10.5 }}>SIZE</span><span style={{ color: 'var(--faint)', fontSize: 10.5 }}>ΔX</span><span style={{ color: 'var(--faint)', fontSize: 10.5 }}>ΔY</span>
      {d.sizes.map((sz) => <React.Fragment key={sz}><span style={{ fontWeight: sz === d.baseSize ? 700 : 400, color: sz === d.activeSize ? 'var(--accent)' : undefined }}>{sz}{sz === d.baseSize ? ' ●' : ''}</span>
        {sz === d.baseSize ? <><span className="atl-mono" style={{ color: 'var(--faint)' }}>0</span><span className="atl-mono" style={{ color: 'var(--faint)' }}>0</span></> : <><Num value={(r[sz] || {}).dx || 0} onChange={(v) => set(sz, 'dx', v)} /><Num value={(r[sz] || {}).dy || 0} onChange={(v) => set(sz, 'dy', v)} /></>}
      </React.Fragment>)}
    </div>
  </Sec>;
}
function SegmentPanel() {
  const app = useApp(), d = app.doc, A = app.actions, sel = app.sel;
  const pc = d.pieces[sel.piece]; if (!pc) return null;
  const ol = AT.outline(AT.gradedPts(pc, d.activeSize, d.baseSize)), k = ol.byId[sel.seg]; if (k == null) return null;
  const sg = ol.segs[k];
  const setLen = (L) => {
    // scale the segment about its start point along the chord (keeps shape)
    const p0 = pc.pts[sg.i0], n = pc.pts.length; const ids = []; let j = (sg.i0 + 1) % n; while (j !== sg.i1) { ids.push(j); j = (j + 1) % n; } ids.push(sg.i1);
    const f = L / sg.len; ids.forEach((i) => { const q = pc.pts[i]; q.x = p0.x + (q.x - p0.x) * f; q.y = p0.y + (q.y - p0.y) * f; });
    AT.syncMirrors(d); A.commit('Set segment length');
  };
  const curved = sg.ctrl.length > 2;
  return <>
    <Sec title="Segment">
      <Row label="Length"><Num value={sg.len} min={1} onChange={setLen} /></Row>
      <Row label="Shape"><span style={{ color: 'var(--muted)' }}>{curved ? 'Curve (' + (sg.ctrl.length - 2) + ' control pts)' : 'Straight'}</span>{curved && <button className="atl-btn ghost" onClick={() => { const old = AT.clone(pc.pts); const n = pc.pts.length, rm = {}; let j = (sg.i0 + 1) % n; while (j !== sg.i1) { rm[pc.pts[j].id] = 1; j = (j + 1) % n; } pc.pts = pc.pts.filter((p) => !rm[p.id]); AT.remapRefs(d, pc.id, old); A.commit('Straighten'); }}>Straighten</button>}</Row>
      {(() => { const cov = AT.edgeCoverage(d, pc, ol, AT.seamNumbers(d)), s0 = ol.start[k], s1 = s0 + sg.len, T = ol.total; const here = cov.sides.filter((x) => Math.min(s1, x.P1) - Math.max(s0, x.P0) > 0.5 || Math.min(s1 + T, x.P1) - Math.max(s0 + T, x.P0) > 0.5); return <Row label="Sewing">{here.length ? here.map((x, i) => <span key={i} className="atl-mono" style={{ padding: '1px 7px', borderRadius: 999, background: x.sw.color, color: '#fff', fontWeight: 600, fontSize: 11 }}>#{x.n}</span>) : <span style={{ color: 'var(--muted)' }}>Free edge</span>}</Row>; })()}
      <Row label="Edge finish"><Seg value={pc.hem && pc.hem[sg.id] > 0 ? 'hem' : 'seam'} onChange={(v) => A.setHem(pc.id, sg.id, v === 'hem' ? AT.defaultHem(d, pc.mirror && d.pieces[pc.mirror.src] ? d.pieces[pc.mirror.src] : pc) : 0)} options={[['seam', 'Seam allowance'], ['hem', 'Hem']]} /></Row>
      {pc.hem && pc.hem[sg.id] > 0
        ? <Row label="Hem allowance"><Num value={pc.hem[sg.id]} min={5} max={150} onChange={(v) => A.setHem(pc.id, sg.id, v)} /></Row>
        : <Row label="Seam allowance" title="On a sewn edge this sets the whole seam, both sides"><Num value={pc.saSeg && pc.saSeg[sg.id] != null ? pc.saSeg[sg.id] : (pc.sa || 0)} min={0} max={80} onChange={(v) => A.setSegSA(pc.id, sg.id, v)} /></Row>}
      <Row label="Elastic" title="Shrinks this edge in simulation (gathers / elastic casing)"><Slide min={40} max={100} value={(pc.elastic && pc.elastic[sg.id]) || 100} fmt={(v) => v + '%'} onChange={(v) => { pc.elastic = Object.assign({}, pc.elastic || {}); if (v >= 100) delete pc.elastic[sg.id]; else pc.elastic[sg.id] = v; A.commit('Elastic'); }} /></Row>
    </Sec>
    <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap', padding: '8px 12px' }}>
      <button className="atl-btn" onClick={() => { pc.notches = (pc.notches || []).concat([{ id: AT.uid('n'), s: sg.id, t: 0.5 }]); A.commit('Add notch'); }}>Notch at middle</button>
      <button className="atl-btn" onClick={() => { const id = AT.uid('st'); d.stitches[id] = { id: id, p: pc.id, seg: sg.id, off: 4, len: 3, gap: 1.6, w: 0.9, color: '#3b3a36', rows: 1, rowGap: 6 }; app.setSel({ kind: 'stitch', id: id }); A.commit('Add topstitch'); }}>Topstitch</button>
      <button className="atl-btn" onClick={() => { const id = AT.uid('pm'); d.poms[id] = { id: id, name: pc.name + ' segment', refs: [{ p: pc.id, s: sg.id }], tol: 6 }; app.setSel({ kind: 'pom', id: id }); A.commit('Add POM'); }}>Add POM</button>
      <button className="atl-btn ghost" onClick={() => { const old = AT.clone(pc.pts); const id = AT.splitSegment(pc, sg.id, 0.5); AT.remapRefs(d, pc.id, old); app.setSel({ kind: 'points', piece: pc.id, ids: [id] }); A.commit('Split'); }}>Split in half</button>
    </div>
  </>;
}
function InternalPanel() {
  const app = useApp(), d = app.doc, A = app.actions, sel = app.sel;
  const pc = d.pieces[sel.piece], il = pc && (pc.internals || []).find((q) => q.id === sel.id); if (!il) return null;
  const isFold = il.fold != null && il.fold !== '';
  return <Sec title="Internal line">
    <Row label="Fold"><Check value={isFold} label="Fold line" onChange={(v) => { il.fold = v ? 90 : null; A.commit('Fold line'); }} /></Row>
    {isFold && <Row label="Fold angle" title="180° = flat, 0° = fold inward fully, 360° = fold outward fully"><Slide min={0} max={360} value={il.fold} fmt={(v) => v + '°'} onChange={(v) => { il.fold = v; A.commit('Fold angle'); }} /></Row>}
    <Row label="Closed"><Check value={!!il.closed} onChange={(v) => { il.closed = v; A.commit('Internal shape'); }} /></Row>
    <Row label="Points"><span className="atl-mono" style={{ color: 'var(--muted)' }}>{il.pts.length}</span></Row>
    <div style={{ padding: '6px 12px' }}><button className="atl-btn ghost" onClick={A.del}><Icon name="trash" size={13} />Delete</button></div>
  </Sec>;
}
function SewingPanel() {
  const app = useApp(), d = app.doc, A = app.actions, sw = d.sewing[app.sel.id]; if (!sw) return null;
  const L = AT.sewLengths(d)[sw.id] || { a: 0, b: 0 };
  const side = (sd, i, k) => { const pc = d.pieces[sd.p]; const ol = pc ? AT.outline(AT.gradedPts(pc, d.activeSize, d.baseSize)) : null; const r = ol ? AT.sideRange(ol, sd) : null; return <div key={k + i} style={{ display: 'flex', alignItems: 'center', gap: 6, padding: '3px 12px' }}><span className="atl-chip">{k.toUpperCase()}{i + 1}</span><span style={{ flex: 1 }}>{pc ? pc.name : '—'}</span><span className="atl-mono">{r ? AT.fmt(r.L, app.units) : '–'}</span></div>; };
  const diff = L.a - L.b;
  return <>
    <Sec title={'Seam #' + (AT.seamNumbers(d)[sw.id] || '?') + ' · ' + (sw.a.length > 1 || sw.b.length > 1 ? sw.a.length + ':' + sw.b.length : '1:1')}>
      <Row label="Seam allowance" title="Applied to every edge of this seam, on both sides"><Num value={AT.seamSA(d, sw)} min={0} max={80} onChange={(v) => A.setSeamSA(sw.id, v)} /></Row>
      {sw.a.map((s, i) => side(s, i, 'a'))}{sw.b.map((s, i) => side(s, i, 'b'))}
      <Row label="Total A / B"><span className="atl-mono">{AT.fmt(L.a, app.units)} / {AT.fmt(L.b, app.units)}</span></Row>
      <Row label="Difference"><span className="atl-mono" style={{ color: Math.abs(diff) > 3 ? 'var(--accent)' : 'var(--muted)' }}>{diff >= 0 ? '+' : ''}{AT.fmt(diff, app.units)} {AT.unitLabel(app.units)}</span></Row>
      <Row label="Colour"><Color value={sw.color} onChange={(v) => { sw.color = v; A.commit('Sewing colour'); }} /></Row>
    </Sec>
    <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap', padding: '8px 12px' }}>
      <button className="atl-btn" onClick={A.flipSewing}>Flip side B</button>
      <button className="atl-btn ghost" onClick={A.del}><Icon name="trash" size={13} />Delete</button>
    </div>
  </>;
}
function FabricPanel(props) {
  const app = useApp(), d = app.doc, A = app.actions;
  const f = d.fabrics[props.id]; if (!f) return null;
  const cwIds = AT.asArray(d.colorways).sort((a, b) => (a.order || 0) - (b.order || 0)).map((c) => c.id);
  const isBaseCw = d.activeColorway === cwIds[0];
  const ef = AT.effFabric(d, f.id);
  const setLook = (patch, live) => {
    if (isBaseCw) Object.assign(f, patch);
    else { const cw = d.colorways[d.activeColorway]; cw.over = cw.over || {}; cw.over[f.id] = Object.assign({}, cw.over[f.id] || {}, patch); }
    if (live) A.liveMaterials(); else A.commit('Fabric appearance');
  };
  const setPhys = (k, v, live) => { f.phys = Object.assign({}, f.phys || {}, { [k]: v }); if (live) A.liveSim(); else A.commit('Fabric physics'); };
  const fileRef = React.useRef(null);
  return <>
    <Sec title="Fabric">
      <Row label="Name"><Txt value={f.name} onChange={(v) => { f.name = v; A.commit('Rename fabric'); }} /></Row>
      <Row label="Preset" title="Loads measured physical properties"><Sel value={f.preset || ''} options={[['', 'Custom']].concat(Object.keys(AT.FABRIC_PRESETS).map((k) => [k, AT.FABRIC_PRESETS[k].name]))} onChange={(v) => { const p = AT.FABRIC_PRESETS[v]; if (!p) return; f.preset = v; f.phys = AT.clone(p.phys); f.weave = p.weave; f.rough = p.rough; f.sheen = p.sheen; A.commit('Fabric preset'); }} /></Row>
      <Row label="Content"><Txt value={f.content} placeholder="e.g. 100% cotton" onChange={(v) => { f.content = v; A.commit('Fabric content'); }} /></Row>
    </Sec>
    <Sec title={'Appearance' + (isBaseCw ? '' : ' · ' + d.colorways[d.activeColorway].name)}>
      <Row label="Colour"><Color value={ef.color} onLive={(v) => setLook({ color: v }, true)} onChange={(v) => setLook({ color: v })} /></Row>
      <Row label="Weave / print"><Sel value={ef.weave || 'plain'} options={AT.WEAVES} onChange={(v) => setLook({ weave: v, scale: AT.PRINT_WEAVES[v] ? 40 : (f.scale > 30 ? 8 : f.scale || 8) })} /></Row>
      {(AT.PRINT_WEAVES[ef.weave] || ef.weave === 'denim') && <Row label="Second colour"><Color value={ef.color2 || '#ffffff'} onLive={(v) => setLook({ color2: v }, true)} onChange={(v) => setLook({ color2: v })} /></Row>}
      <Row label="Repeat size"><Num value={ef.scale || 10} min={1} max={500} onChange={(v) => setLook({ scale: v })} /></Row>
      <Row label="Roughness"><Slide min={0} max={1} step={0.01} value={ef.rough == null ? 0.85 : ef.rough} fmt={(v) => v.toFixed(2)} onLive={(v) => setLook({ rough: v }, true)} onChange={(v) => setLook({ rough: v })} /></Row>
      <Row label="Opacity"><Slide min={0.2} max={1} step={0.01} value={ef.opacity || 1} fmt={(v) => Math.round(v * 100) + '%'} onLive={(v) => setLook({ opacity: v }, true)} onChange={(v) => setLook({ opacity: v })} /></Row>
      <Row label="Texture image">
        <input ref={fileRef} type="file" accept="image/*" style={{ display: 'none' }} onChange={(e) => { const file = e.target.files && e.target.files[0]; e.target.value = ''; if (file) A.uploadTexture(f.id, file); }} />
        {ef.texture && (ef.texture.path || ef.texture.url) ? <><img src={AT.resolveAsset(ef.texture)} style={{ width: 24, height: 24, objectFit: 'cover', borderRadius: 4 }} /><button className="atl-btn ghost" onClick={() => setLook({ texture: null })}>Remove</button></> : <button className="atl-btn" onClick={() => fileRef.current && fileRef.current.click()}><Icon name="upload" size={13} />Upload</button>}
      </Row>
      {ef.texture && (ef.texture.path || ef.texture.url) && <Row label="Texture width"><Num value={ef.texture.w || 100} min={5} onChange={(v) => setLook({ texture: Object.assign({}, ef.texture, { w: v }) })} /></Row>}
    </Sec>
    <Sec title="Physical properties">
      {AT.PHYS_KEYS.map((pk) => { const k = pk[0]; const isD = k === 'density', isT = k === 'thickness'; return <Row key={k} label={pk[1]}>
        <Slide min={isD ? 20 : isT ? 0.1 : 0} max={isD ? 900 : isT ? 5 : 100} step={isT ? 0.05 : 1} value={(f.phys || {})[k]} fmt={(v) => isT ? v.toFixed(2) : String(Math.round(v))} onLive={(v) => setPhys(k, v, true)} onChange={(v) => setPhys(k, v)} />
      </Row>; })}
      <div style={{ padding: '2px 12px', color: 'var(--faint)' }}>Stretch {AT.E2D((f.phys || {}).stretchWeft).toFixed(0)} / {AT.E2D((f.phys || {}).stretchWarp).toFixed(0)} N/m · limit strain {(AT.limitStrain(((f.phys || {}).stretchWeft + (f.phys || {}).stretchWarp) / 2) * 100).toFixed(0)}%</div>
    </Sec>
    <Sec title="Costing">
      <Row label="Width"><Num value={f.width || 1450} min={300} max={3200} onChange={(v) => { f.width = v; A.commit('Fabric width'); }} /></Row>
      <Row label="Price / m"><Num raw value={f.price || 0} min={0} onChange={(v) => { f.price = v; A.commit('Fabric price'); }} suffix="" /></Row>
      <Row label="Marker loss"><Slide min={0} max={60} value={Math.round((f.waste == null ? 0.25 : f.waste) * 100)} fmt={(v) => v + '%'} onChange={(v) => { f.waste = v / 100; A.commit('Marker loss'); }} /></Row>
    </Sec>
    <div style={{ display: 'flex', gap: 6, padding: '8px 12px' }}>{app.selPieces.length > 0 && <button className="atl-btn primary" onClick={() => A.applyFabric(f.id)}>Apply to selected</button>}</div>
  </>;
}
function ButtonPanel() {
  const app = useApp(), d = app.doc, A = app.actions, b = d.buttons[app.sel.id]; if (!b) return null;
  const set = (patch, label) => { Object.assign(b, patch); A.commit(label || 'Button'); };
  return <Sec title="Button">
    <Row label="Shape"><Sel value={b.shape || 'round'} options={[['round', 'Round'], ['square', 'Square'], ['shank', 'Shank']]} onChange={(v) => set({ shape: v })} /></Row>
    <Row label="Size"><Num value={b.size || 15} min={5} max={60} onChange={(v) => set({ size: v })} /></Row>
    <Row label="Holes"><Sel value={String(b.holes == null ? 4 : b.holes)} options={['0', '2', '4']} onChange={(v) => set({ holes: parseInt(v, 10) })} /></Row>
    <Row label="Colour"><Color value={b.color} onChange={(v) => set({ color: v })} /></Row>
    <Row label="Finish"><Seg value={b.finish || 'plastic'} onChange={(v) => set({ finish: v })} options={[['plastic', 'Matte'], ['metal', 'Metal']]} /></Row>
    <Row label="Fastened to"><Sel value={b.fasten || ''} options={[['', '— none —']].concat(Object.values(d.holes).map((h) => [h.id, ((d.pieces[h.p] || {}).name || '') + ' · ' + h.len + 'mm']))} onChange={(v) => set({ fasten: v || null }, 'Fasten')} /></Row>
    <div style={{ padding: '6px 12px' }}><button className="atl-btn ghost" onClick={A.del}><Icon name="trash" size={13} />Delete</button></div>
  </Sec>;
}
function HolePanel() {
  const app = useApp(), d = app.doc, A = app.actions, h = d.holes[app.sel.id]; if (!h) return null;
  const set = (patch) => { Object.assign(h, patch); A.commit('Buttonhole'); };
  return <Sec title="Buttonhole">
    <Row label="Length"><Num value={h.len || 18} min={5} max={60} onChange={(v) => set({ len: v })} /></Row>
    <Row label="Angle"><Slide min={-90} max={90} value={h.ang || 0} fmt={(v) => v + '°'} onChange={(v) => set({ ang: v })} /></Row>
    <Row label="Thread"><Color value={h.color} onChange={(v) => set({ color: v })} /></Row>
    <div style={{ padding: '6px 12px' }}><button className="atl-btn ghost" onClick={A.del}><Icon name="trash" size={13} />Delete</button></div>
  </Sec>;
}
function StitchPanel() {
  const app = useApp(), d = app.doc, A = app.actions, st = d.stitches[app.sel.id]; if (!st) return null;
  const set = (patch) => { Object.assign(st, patch); A.commit('Topstitch'); };
  return <Sec title="Topstitch">
    <Row label="Applies to"><Seg value={st.seg === '*' ? 'all' : 'seg'} onChange={(v) => { if (v === 'all') set({ seg: '*' }); }} options={[['seg', 'Segment'], ['all', 'Whole outline']]} /></Row>
    <Row label="Distance from edge"><Num value={st.off || 3} min={0.5} max={60} onChange={(v) => set({ off: v })} /></Row>
    <Row label="Rows"><Num raw value={st.rows || 1} min={1} max={4} onChange={(v) => set({ rows: Math.round(v) })} /></Row>
    {(st.rows || 1) > 1 && <Row label="Row spacing"><Num value={st.rowGap || 6} min={1} max={30} onChange={(v) => set({ rowGap: v })} /></Row>}
    <Row label="Stitch length"><Num value={st.len || 3} min={0.5} max={15} onChange={(v) => set({ len: v })} /></Row>
    <Row label="Gap"><Num value={st.gap || 1.5} min={0.2} max={10} onChange={(v) => set({ gap: v })} /></Row>
    <Row label="Thread width"><Num value={st.w || 0.8} min={0.2} max={4} onChange={(v) => set({ w: v })} /></Row>
    <Row label="Thread colour"><Color value={st.color} onChange={(v) => set({ color: v })} /></Row>
    <div style={{ padding: '6px 12px' }}><button className="atl-btn ghost" onClick={A.del}><Icon name="trash" size={13} />Delete</button></div>
  </Sec>;
}
function GraphicPanel() {
  const app = useApp(), d = app.doc, A = app.actions, sel = app.sel, pc = d.pieces[sel.piece];
  const g = pc && (pc.graphics || []).find((q) => q.id === sel.id); if (!g) return null;
  const set = (patch, live) => { Object.assign(g, patch); if (live) A.liveMaterials(); else A.commit('Graphic'); };
  return <Sec title="Graphic">
    <div style={{ padding: '4px 12px 8px' }}><img src={AT.resolveAsset(g)} style={{ width: '100%', maxHeight: 120, objectFit: 'contain', background: 'var(--panel2)', borderRadius: 8 }} /></div>
    <Row label="Width"><Num value={g.w} min={2} onChange={(v) => set({ w: v, h: v * g.h / g.w })} /></Row>
    <Row label="Height"><Num value={g.h} min={2} onChange={(v) => set({ h: v, w: v * g.w / g.h })} /></Row>
    <Row label="Rotation"><Slide min={-180} max={180} value={g.rot || 0} fmt={(v) => v + '°'} onLive={(v) => set({ rot: v }, true)} onChange={(v) => set({ rot: v })} /></Row>
    <Row label="Opacity"><Slide min={0} max={1} step={0.01} value={g.opacity == null ? 1 : g.opacity} fmt={(v) => Math.round(v * 100) + '%'} onLive={(v) => set({ opacity: v }, true)} onChange={(v) => set({ opacity: v })} /></Row>
    <div style={{ padding: '6px 12px' }}><button className="atl-btn ghost" onClick={A.del}><Icon name="trash" size={13} />Delete</button></div>
  </Sec>;
}
function PomPanel() {
  const app = useApp(), d = app.doc, A = app.actions, pm = d.poms[app.sel.id]; if (!pm) return null;
  return <Sec title="Point of measure">
    <Row label="Name"><Txt value={pm.name} onChange={(v) => { pm.name = v; A.commit('POM'); }} /></Row>
    <Row label="Tolerance ±"><Num value={pm.tol == null ? 6 : pm.tol} min={0} max={50} onChange={(v) => { pm.tol = v; A.commit('POM tolerance'); }} /></Row>
    <div style={{ padding: '4px 12px' }}><table className="atl-tbl"><thead><tr><th>Size</th><th className="r">Value</th><th className="r">Δ base</th></tr></thead><tbody>
      {d.sizes.map((sz) => { const v = AT.pomValue(d, pm, sz), b = AT.pomValue(d, pm, d.baseSize); return <tr key={sz}><td style={{ fontWeight: sz === d.baseSize ? 600 : 400 }}>{sz}</td><td className="r atl-mono">{AT.fmt(v, app.units)}</td><td className="r atl-mono" style={{ color: 'var(--muted)' }}>{sz === d.baseSize ? '—' : (v - b >= 0 ? '+' : '') + AT.fmt(v - b, app.units)}</td></tr>; })}
    </tbody></table></div>
    <div style={{ padding: '6px 12px' }}><button className="atl-btn ghost" onClick={A.del}><Icon name="trash" size={13} />Delete</button></div>
  </Sec>;
}
function AvatarPanel() {
  const app = useApp(), d = app.doc, A = app.actions, m = d.avatar, info = app.avInfo || {}, meas = info.measured || {};
  const [sizeName, setSizeName] = React.useState('');
  const isMH = info.kind === 'makehuman';
  const chart = AT.SIZE_CHARTS[m.sex === 'male' ? 'male' : 'female'];
  const errs = AT.MEAS.filter((x) => m[x.k] != null && meas[x.k] != null).map((x) => Math.abs(meas[x.k] - m[x.k]));
  const maxErr = errs.length ? Math.max.apply(null, errs) : 0;
  const status = isMH ? 'MakeHuman body · fitted in ' + Math.round(info.ms || 0) + ' ms · largest deviation ' + AT.fmt(maxErr, app.units) + ' ' + AT.unitLabel(app.units)
    : app.mhState === 'loading' || app.mhState === 'idle' ? 'Loading the MakeHuman body…' : 'Built-in mannequin (MakeHuman library unavailable)';
  const groups = ['Body', 'Neck & torso', 'Arms', 'Legs'];
  const sizes = Object.entries(app.avSizes || {}).map(([k, v]) => Object.assign({ id: k }, v)).sort((x, y) => (y.at || 0) - (x.at || 0));
  return <>
    <Sec title="Body">
      <Row label="Base body"><Seg value={m.sex === 'male' ? 'male' : 'female'} onChange={A.setAvatarSex} options={[['female', 'Female'], ['male', 'Male']]} /></Row>
      <Row label="Size chart"><div style={{ display: 'flex', gap: 4, flexWrap: 'wrap' }}>{chart.map((c) => <button key={c.name} className="atl-btn ghost" style={{ height: 22, padding: '0 8px' }} onClick={() => A.applySizeChart(c)}>{c.name}</button>)}</div></Row>
      <div style={{ padding: '2px 12px 4px', color: isMH && maxErr > 3 ? 'var(--accent)' : 'var(--faint)', fontSize: 11 }}>{status}</div>
    </Sec>
    {groups.map((g) => <Sec key={g} title={g}>
      <div style={{ display: 'grid', gridTemplateColumns: 'minmax(0,1fr) 70px 46px 34px', gap: 6, padding: '0 12px 2px', color: 'var(--faint)', fontSize: 10, textTransform: 'uppercase', letterSpacing: '.05em' }}><span /><span>Target</span><span style={{ textAlign: 'right' }}>Body</span><span style={{ textAlign: 'right' }}>Δ</span></div>
      {AT.MEAS.filter((x) => x.grp === g).map((x) => {
        const want = m[x.k], got = meas[x.k], err = want != null && got != null ? got - want : null;
        return <div key={x.k} onMouseEnter={() => A.focusTape(x.k)} onMouseLeave={() => A.focusTape(null)} style={{ display: 'grid', gridTemplateColumns: 'minmax(0,1fr) 70px 46px 34px', gap: 6, alignItems: 'center', padding: '2px 12px', minHeight: 28 }}>
          <span style={{ color: 'var(--muted)', fontSize: 11.5, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }} title={x.label}>{x.label}</span>
          <Num allowEmpty value={want == null ? null : want} placeholder="auto" onChange={(v) => A.setAvatarField(x.k, v, 'Avatar ' + x.label)} onFocus={() => A.focusTape(x.k)} />
          <span className="atl-mono" style={{ textAlign: 'right', color: 'var(--text)' }}>{got != null ? AT.fmt(got, app.units) : '–'}</span>
          <span className="atl-mono" style={{ textAlign: 'right', fontSize: 10.5, color: err != null && Math.abs(err) > 3 ? 'var(--accent)' : 'var(--faint)' }}>{err == null ? 'auto' : (Math.abs(err) < 0.5 ? '0' : (err > 0 ? '+' : '') + AT.fmt(err, app.units))}</span>
        </div>;
      })}
    </Sec>)}
    <Sec title="Shape">
      <Row label="Body fat"><Check value={m.fat == null} label="Solve from measurements" onChange={(v) => A.setAvatarField('fat', v ? null : (info.fit ? Math.round(info.fit.weight * 100) / 100 : 0.5), 'Avatar body fat')} /></Row>
      {m.fat != null && <Row label=" "><Slide min={0} max={1} step={0.01} value={m.fat} fmt={(v) => Math.round(v * 100) + '%'} onChange={(v) => A.setAvatarField('fat', v, 'Avatar body fat')} /></Row>}
      {m.fat == null && info.fit && <div style={{ padding: '0 12px 4px', color: 'var(--faint)', fontSize: 11 }}>Solved body fat {Math.round(info.fit.weight * 100)}%</div>}
      <Row label="Muscle"><Slide min={0} max={1} step={0.01} value={m.muscle == null ? 0.5 : m.muscle} fmt={(v) => Math.round(v * 100) + '%'} onChange={(v) => A.setAvatarField('muscle', v, 'Avatar muscle')} /></Row>
    </Sec>
    <Sec title="Pose">
      <Row label="Preset"><div style={{ display: 'flex', gap: 4, flexWrap: 'wrap' }}>{[['A-pose', 45], ['Relaxed', 30], ['Open', 60], ['T-pose', 82]].map((p) => <button key={p[0]} className={'atl-btn' + ((m.armAngle == null ? 45 : m.armAngle) === p[1] ? ' primary' : ' ghost')} style={{ height: 22, padding: '0 8px' }} onClick={() => A.setAvatarField('armAngle', p[1], 'Pose ' + p[0])}>{p[0]}</button>)}</div></Row>
      <Row label="Arm angle"><Slide min={15} max={85} value={m.armAngle == null ? 45 : m.armAngle} fmt={(v) => v + '°'} onChange={(v) => A.setAvatarField('armAngle', v, 'Avatar arm angle')} /></Row>
      <Row label="Leg stance"><Slide min={0} max={15} value={m.legSpread == null ? 6 : m.legSpread} fmt={(v) => v + '°'} onChange={(v) => A.setAvatarField('legSpread', v, 'Avatar leg stance')} /></Row>
    </Sec>
    <Sec title="Display">
      <Row label="Measurement tapes"><Check value={!!app.disp3.tapes} onChange={(v) => app.setDisp3({ tapes: v })} /></Row>
      <Row label="Skin"><div style={{ display: 'flex', gap: 5 }}>{['#b4b1ac', '#cfc9c1', '#e8cdb6', '#c69c7b', '#8d5b3e', '#5a3a28', '#9aa0a6'].map((c) => <button key={c} onClick={() => A.setAvatarField('skin', c, 'Avatar skin')} style={{ width: 20, height: 20, borderRadius: 999, background: c, border: m.skin === c ? '2px solid var(--accent)' : '1px solid var(--line2)', cursor: 'pointer' }} />)}</div></Row>
    </Sec>
    <Sec title="Avatar sizes">
      {sizes.map((sz) => <div key={sz.id} className="atl-list" onClick={() => A.applyAvatarSize(sz)}><Icon name="avatar" size={14} /><span style={{ flex: 1, minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{sz.name}</span><span style={{ color: 'var(--faint)', fontSize: 10.5 }}>{sz.spec && sz.spec.sex}</span><IB sm icon="trash" tip="Delete size" onClick={(e) => { e.stopPropagation(); A.deleteAvatarSize(sz.id); }} /></div>)}
      <div style={{ display: 'flex', gap: 6, padding: '6px 12px' }}><input className="atl-in" placeholder="Name this size…" value={sizeName} onChange={(e) => setSizeName(e.target.value)} onKeyDown={(e) => { e.stopPropagation(); if (e.key === 'Enter') { A.saveAvatarSize(sizeName.trim()); setSizeName(''); } }} /><button className="atl-btn" onClick={() => { A.saveAvatarSize(sizeName.trim()); setSizeName(''); }}>Save</button></div>
      <div style={{ display: 'flex', gap: 6, padding: '0 12px 6px' }}><button className="atl-btn ghost" onClick={A.exportAvatar}><Icon name="download" size={13} />Export</button><button className="atl-btn ghost" onClick={A.importAvatar}><Icon name="upload" size={13} />Import</button></div>
    </Sec>
    <div style={{ padding: '6px 12px 10px', color: 'var(--faint)', lineHeight: 1.5, fontSize: 11 }}>Body model: MakeHuman base mesh, morph targets and skin weights (CC0 1.0, public domain). Girths read like a tape: the convex perimeter in the measuring plane. Leave a field on auto to let the body find a natural value.</div>
  </>;
}
function ProjectPanel() {
  const app = useApp(), d = app.doc, A = app.actions;
  const pcs = AT.asArray(d.pieces);
  return <>
    <Sec title="Project">
      <Row label="Name"><Txt value={d.meta.name} onChange={A.rename} /></Row>
      <Row label="Units"><Seg value={app.units} onChange={A.setUnits} options={[['mm', 'mm'], ['cm', 'cm'], ['in', 'in']]} /></Row>
      <Row label="Patterns"><span className="atl-mono">{pcs.length}</span></Row>
      <Row label="Sewing lines"><span className="atl-mono">{Object.keys(d.sewing).length}</span></Row>
    </Sec>
    <SimPanel compact />
    <Sec title="Avatar"><div style={{ padding: '2px 12px' }}><button className="atl-btn" onClick={() => app.setSel({ kind: 'avatar' })}><Icon name="avatar" size={13} />Edit avatar</button></div></Sec>
    <div style={{ padding: '8px 12px', color: 'var(--faint)', lineHeight: 1.5 }}>Select a pattern, point, segment, sewing line or fabric to edit its properties. Press <span className="atl-kbd">Space</span> to simulate.</div>
  </>;
}
// ---------- simulation settings ----------
const SIM_DT = [[1 / 30, '1/30'], [1 / 60, '1/60'], [1 / 120, '1/120'], [1 / 240, '1/240']];
function SimPanel(props) {
  const app = useApp(), d = app.doc, A = app.actions;
  const st = AT.simSettings(d), v = st.v, pre = AT.SIM_PRESETS[st.preset];
  const [, setTick] = React.useState(0);
  React.useEffect(() => { const t = setInterval(() => setTick((n) => n + 1), 900); return () => clearInterval(t); }, []);
  const live = (typeof window !== 'undefined' && window.__atelierSim) || {};
  const nPieces = Object.keys(d.pieces || {}).length;
  const est = React.useMemo(() => AT.estimateParticles(d, v.pdScale), [v.pdScale, nPieces, d.activeSize]);
  const set = (k) => (x) => A.setSimParam(k, x);
  const dot = (k) => st.over[k] != null ? <span title="Changed from the preset" style={{ width: 5, height: 5, borderRadius: 3, background: 'var(--accent)', display: 'inline-block', marginLeft: 5, verticalAlign: 'middle' }} /> : null;
  const L = (txt, k) => <>{txt}{dot(k)}</>;
  const muted = { color: 'var(--faint)', fontSize: 11, lineHeight: 1.5, padding: '2px 12px 6px' };
  const gpu = AT.GPU.state === 'ready';
  const heavy = !gpu && (v.pdScale < 0.9 || v.substeps > 12);
  const ms = live.gpuMsPerFrame;
  return <>
    <Sec title="Simulation" right={st.modified ? <button className="atl-btn ghost" style={{ height: 20, padding: '0 8px' }} onClick={(e) => { e.stopPropagation(); A.resetSimParams(); }}>Reset</button> : null}>
      <div style={{ padding: '4px 12px 2px' }}><Seg value={st.preset} onChange={A.setSimPreset} options={Object.keys(AT.SIM_PRESETS).map((k) => [k, AT.SIM_PRESETS[k].name])} /></div>
      <div style={muted}>{pre.desc}{st.modified ? ' Modified: ' + Object.keys(st.over).length + ' setting' + (Object.keys(st.over).length > 1 ? 's' : '') + ' changed (marked with a dot).' : ''}</div>
      <Row label="Solver"><span className="atl-mono" style={{ color: gpu ? 'var(--text)' : 'var(--muted)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }} title={AT.GPU.error || ''}>{AT.GPU.label()}</span></Row>
      <Row label="Load"><span className="atl-mono" style={{ color: 'var(--muted)' }}>{(live.particles || 0).toLocaleString()} particles{ms ? ' · ' + ms.toFixed(1) + ' ms/frame' : ''}{live.simulating && live.fps ? ' · ' + Math.round(live.fps) + ' fps' : ''}{live.crossings != null ? ' · ' + live.crossings + ' crossings' : ''}</span></Row>
      {heavy && <div style={Object.assign({}, muted, { color: 'var(--accent)' })}>WebGPU is not available, so this runs on the CPU and will be slow. Draft or Normal is recommended.</div>}
    </Sec>
    <Sec title="Quality" open={!props.compact}>
      <Row label={L('Mesh density', 'pdScale')} title="Multiplies every pattern's particle distance. Lower = finer mesh, smaller wrinkles, slower."><Slide min={0.4} max={2.5} step={0.05} value={v.pdScale} fmt={(x) => '×' + x.toFixed(2)} onChange={set('pdScale')} /></Row>
      <div style={muted}>≈ {est.toLocaleString()} particles at this density. Pieces keep their own particle distance; this scales all of them.</div>
      <Row label={L('Time step', 'dt')} title="Simulated time per frame. Smaller is more stable and slower to drape."><Seg value={SIM_DT.reduce((b, o) => Math.abs(o[0] - v.dt) < Math.abs(b[0] - v.dt) ? o : b)[0]} onChange={set('dt')} options={SIM_DT.map((o) => [o[0], o[1] + ' s'])} /></Row>
      <Row label={L('Substeps', 'substeps')} title="Solver substeps per frame. More = stiffer, more accurate cloth and collisions."><Slide min={2} max={40} step={1} value={v.substeps} onChange={set('substeps')} /></Row>
      <Row label={L('Iterations', 'iterations')} title="Stretch and seam constraint passes per substep. More = less stretch, tighter seams."><Slide min={1} max={12} step={1} value={v.iterations} onChange={set('iterations')} /></Row>
      <Row label={L('Speed-up', 'framesPerStep')} title="Simulated frames per screen refresh. Drapes faster; each refresh takes longer."><Slide min={1} max={6} step={1} value={v.framesPerStep} fmt={(x) => '×' + x} onChange={set('framesPerStep')} /></Row>
    </Sec>
    <Sec title="Collision" open={!props.compact}>
      <Row label={L('Self-collision', 'selfCollision')} title="Cloth against cloth: within a piece, between pieces and between layers."><Check value={v.selfCollision} onChange={A.toggleSelfCollision} /></Row>
      <Row label={L('Detect every', 'ccEveryGPU')} title="How often cloth contacts are re-detected. 1 = every substep (most robust)."><Slide min={1} max={6} step={1} value={v.ccEveryGPU} fmt={(x) => x + (x > 1 ? ' sub' : ' sub')} onChange={set('ccEveryGPU')} /></Row>
      <Row label={L('Contact passes', 'ccIters')} title="Contact resolution passes per substep. More = fewer layers passing through each other."><Slide min={1} max={5} step={1} value={v.ccIters} onChange={set('ccIters')} /></Row>
      <Row label={L('Thickness', 'clothThickness')} title="Gap kept between cloth surfaces, in mm."><Slide min={0.5} max={8} step={0.5} value={v.clothThickness} fmt={(x) => x.toFixed(1) + ' mm'} onChange={set('clothThickness')} /></Row>
      <Row label={L('Cloth friction', 'ccFric')} title="Friction between cloth surfaces, times each fabric's own friction. Higher keeps outer layers from sliding."><Slide min={0} max={5} step={0.1} value={v.ccFric} fmt={(x) => '×' + x.toFixed(1)} onChange={set('ccFric')} /></Row>
      <Row label={L('Untangle', 'untangle')} title="Finds cloth that has passed through other cloth (for example after a fast drag) and pushes it back to its own side."><Check value={v.untangle} onChange={(x) => A.setSimParam('untangle', x)} /></Row>
      <Row label={L('Friction model', 'fricModel')} title="Grip: touching cloth holds together, like most fabrics at rest. Slide: friction depends on how hard cloth presses, so layers worn over each other can slide."><Seg value={v.fricModel} onChange={set('fricModel')} options={[['grip', 'Grip'], ['slide', 'Slide']]} /></Row>
      <Row label={L('Layer repair', 'ccRepair')} title="When an outer layer ends up inside an inner one, push it back to its own side."><Check value={v.ccRepair} onChange={(x) => A.setSimParam('ccRepair', x)} /></Row>
    </Sec>
    <Sec title="Forces" open={!props.compact}>
      <Row label={L('Gravity', 'gravity')}><Slide min={0} max={2} step={0.05} value={v.gravity} fmt={(x) => x.toFixed(2) + 'g'} onChange={set('gravity')} /></Row>
      <Row label={L('Air damping', 'air')} title="Slows motion. Higher settles faster but can look floaty."><Slide min={0} max={3} step={0.1} value={v.air} fmt={(x) => x.toFixed(1)} onChange={set('air')} /></Row>
      <Row label={L('Drag speed limit', 'grabSpeed')} title="Fastest the grabbed cloth follows the mouse. Lower stops quick drags from pushing cloth through other cloth."><Slide min={100} max={3000} step={50} value={v.grabSpeed} fmt={(x) => (x / 1000).toFixed(2) + ' m/s'} onChange={set('grabSpeed')} /></Row>
      <Row label={L('Sewing speed', 'sewSpeed')} title="How fast open seams are pulled together while sewing."><Slide min={200} max={6000} step={100} value={v.sewSpeed} fmt={(x) => (x / 1000).toFixed(1) + ' m/s'} onChange={set('sewSpeed')} /></Row>
    </Sec>
    <Sec title="Run" open={!props.compact}>
      <Row label={L('Stop when settled', 'autoStop')} title="Space starts a run that stops by itself once the garment stops moving."><Check value={v.autoStop} onChange={(x) => A.setSimParam('autoStop', x)} /></Row>
      <Row label={L('Settle at', 'settle')} title="A run stops when motion falls below this, or once it stops decreasing."><Slide min={2} max={60} step={1} value={v.settle} onChange={set('settle')} /></Row>
      <Row label={L('Max duration', 'maxTime')} title="Stop an auto-stopping run after this much simulated time."><Slide min={2} max={120} step={1} value={v.maxTime} fmt={(x) => x + ' s'} onChange={set('maxTime')} /></Row>
    </Sec>
  </>;
}
function CommentPanel(props) {
  const app = useApp(), d = app.doc, A = app.actions, c = d.comments[props.id];
  const [reply, setReply] = React.useState('');
  if (!c) return null;
  return <Sec title={'Comment #' + (c.n || '')}>
    <div style={{ padding: '4px 12px' }}><div style={{ color: 'var(--muted)', fontSize: 11 }}>{c.byName} · {new Date(c.at).toLocaleString()} · {(d.pieces[c.p] || {}).name}</div><div style={{ marginTop: 4, whiteSpace: 'pre-wrap', userSelect: 'text' }}>{c.text}</div></div>
    {AT.asArray(c.replies).map((r, i) => <div key={i} style={{ padding: '6px 12px 0 22px' }}><div style={{ color: 'var(--muted)', fontSize: 11 }}>{r.byName} · {new Date(r.at).toLocaleString()}</div><div style={{ whiteSpace: 'pre-wrap', userSelect: 'text' }}>{r.text}</div></div>)}
    <div style={{ display: 'flex', gap: 6, padding: '8px 12px' }}><input className="atl-in" placeholder="Reply…" value={reply} onChange={(e) => setReply(e.target.value)} onKeyDown={(e) => { e.stopPropagation(); if (e.key === 'Enter' && reply.trim()) { A.replyComment(c.id, reply.trim()); setReply(''); } }} /></div>
    <div style={{ display: 'flex', gap: 6, padding: '0 12px' }}><button className="atl-btn" onClick={() => { c.resolved = !c.resolved; A.commit(c.resolved ? 'Resolve comment' : 'Reopen comment'); }}>{c.resolved ? 'Reopen' : 'Resolve'}</button><button className="atl-btn ghost" onClick={A.del}><Icon name="trash" size={13} />Delete</button></div>
  </Sec>;
}
function GradingPanel() {
  const app = useApp(), d = app.doc, A = app.actions;
  const [newSize, setNewSize] = React.useState('');
  const [wPct, setW] = React.useState(4), [lPct, setL] = React.useState(2);
  const sel = app.sel || {};
  return <div style={{ display: 'flex', flexDirection: 'column', height: '100%', minHeight: 0 }}>
    <div style={{ display: 'flex', alignItems: 'center', height: 32, padding: '0 12px', borderBottom: '1px solid var(--line)', fontWeight: 600 }}>Grading</div>
    <div style={{ flex: 1, overflowY: 'auto' }}>
      <Sec title="Size group">
        {d.sizes.map((sz, i) => <div key={sz} className={'atl-list' + (sz === d.activeSize ? ' on' : '')} onClick={() => A.setActiveSize(sz)}>
          <span style={{ width: 10, height: 10, borderRadius: 3, background: AT.SIZE_NEST_COLORS[i % AT.SIZE_NEST_COLORS.length] }} /><span style={{ fontWeight: sz === d.baseSize ? 700 : 500, flex: 1 }}>{sz}</span>
          {sz === d.baseSize ? <span className="atl-chip">base</span> : <button className="atl-btn ghost" style={{ height: 20, padding: '0 6px' }} onClick={(e) => { e.stopPropagation(); A.setBaseSize(sz); }}>Set base</button>}
          <IB sm icon="trash" tip="Remove size" disabled={sz === d.baseSize} onClick={(e) => { e.stopPropagation(); A.removeSize(sz); }} />
        </div>)}
        <div style={{ display: 'flex', gap: 6, padding: '6px 12px' }}><input className="atl-in" placeholder="Add size (e.g. XXL, 42)" value={newSize} onChange={(e) => setNewSize(e.target.value)} onKeyDown={(e) => { e.stopPropagation(); if (e.key === 'Enter' && newSize.trim()) { A.addSize(newSize.trim()); setNewSize(''); } }} /><button className="atl-btn" onClick={() => { if (newSize.trim()) { A.addSize(newSize.trim()); setNewSize(''); } }}>Add</button></div>
      </Sec>
      <Sec title="Proportional grading">
        <Row label="Width / step"><Num raw value={wPct} min={0} max={20} onChange={setW} suffix="%" /></Row>
        <Row label="Length / step"><Num raw value={lPct} min={0} max={20} onChange={setL} suffix="%" /></Row>
        <div style={{ display: 'flex', gap: 6, padding: '4px 12px' }}><button className="atl-btn primary" onClick={() => A.autoGrade(app.selPieces.length ? app.selPieces : null, wPct, lPct)}>Grade {app.selPieces.length ? 'selected' : 'all'}</button><button className="atl-btn ghost" onClick={() => A.clearGrade(app.selPieces.length ? app.selPieces : null)}>Clear rules</button></div>
        <div style={{ padding: '4px 12px', color: 'var(--faint)', lineHeight: 1.5 }}>Writes per-point grade rules from each piece's top-centre. Fine-tune any point: select it with <span className="atl-kbd">Z</span> and edit the rule table, or switch to a non-base size and drag the point.</div>
      </Sec>
      {sel.kind === 'points' && d.pieces[sel.piece] && sel.ids.length === 1 && <GradeRule pc={d.pieces[sel.piece]} pid={sel.ids[0]} />}
      <Sec title="Graded measurements">
        <div style={{ padding: '0 8px', overflowX: 'auto' }}><table className="atl-tbl"><thead><tr><th>POM</th>{d.sizes.map((s) => <th key={s} className="r">{s}</th>)}</tr></thead><tbody>
          {Object.values(d.poms).map((pm) => <tr key={pm.id}><td>{pm.name}</td>{d.sizes.map((s) => <td key={s} className="r atl-mono" style={{ fontWeight: s === d.baseSize ? 600 : 400 }}>{AT.fmt(AT.pomValue(d, pm, s), app.units)}</td>)}</tr>)}
        </tbody></table></div>
      </Sec>
    </div>
  </div>;
}
function ColorwayPanel() {
  const app = useApp(), d = app.doc, A = app.actions;
  const cws = AT.asArray(d.colorways).sort((a, b) => (a.order || 0) - (b.order || 0));
  const fids = Object.keys(d.fabrics);
  return <div style={{ display: 'flex', flexDirection: 'column', height: '100%', minHeight: 0 }}>
    <div style={{ display: 'flex', alignItems: 'center', height: 32, padding: '0 8px 0 12px', borderBottom: '1px solid var(--line)' }}><span style={{ fontWeight: 600 }}>Colorways</span><div style={{ marginLeft: 'auto', display: 'flex', gap: 4 }}><button className="atl-btn ghost" onClick={A.renderColorways}><Icon name="camera" size={13} />Render all</button><button className="atl-btn" onClick={A.addColorway}><Icon name="plus" size={13} />New</button></div></div>
    <div style={{ flex: 1, overflowY: 'auto', padding: 8, display: 'grid', gap: 8 }}>
      {cws.map((cw, i) => <div key={cw.id} onClick={() => A.setColorway(cw.id)} style={{ borderRadius: 10, border: '1px solid ' + (cw.id === d.activeColorway ? 'var(--accent)' : 'var(--line)'), background: 'var(--panel2)', padding: 8, cursor: 'pointer' }}>
        <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
          {app.cwThumbs[cw.id] ? <img src={app.cwThumbs[cw.id]} style={{ width: 44, height: 64, objectFit: 'cover', borderRadius: 6, background: 'var(--panel)' }} /> : <div style={{ width: 44, height: 64, borderRadius: 6, background: 'var(--panel)', display: 'flex', alignItems: 'center', justifyContent: 'center', color: 'var(--faint)' }}><Icon name="palette" size={16} /></div>}
          <div style={{ flex: 1, minWidth: 0 }} onClick={(e) => e.stopPropagation()}><Txt value={cw.name} onChange={(v) => { cw.name = v; A.commit('Rename colorway'); }} /><div style={{ color: 'var(--faint)', fontSize: 10.5, marginTop: 3 }}>{i === 0 ? 'Base colorway — edits change the fabric itself' : 'Overrides fabric colours'}</div></div>
          {i > 0 && <IB sm icon="trash" tip="Delete colorway" onClick={(e) => { e.stopPropagation(); A.deleteColorway(cw.id); }} />}
        </div>
        <div style={{ display: 'grid', gap: 4, marginTop: 8 }} onClick={(e) => e.stopPropagation()}>
          {fids.map((fid) => { const f = d.fabrics[fid], c = (cw.over && cw.over[fid] && cw.over[fid].color) || f.color; return <div key={fid} style={{ display: 'flex', alignItems: 'center', gap: 6 }}><span style={{ flex: 1, minWidth: 0, color: 'var(--muted)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{f.name}</span><div style={{ width: 120 }}><Color value={c} onChange={(v) => A.setColorwayColor(cw.id, fid, v, i === 0)} /></div></div>; })}
        </div>
      </div>)}
    </div>
  </div>;
}
function ReviewPanel() {
  const app = useApp(), d = app.doc, A = app.actions;
  const [show, setShow] = React.useState('open');
  const list = AT.asArray(d.comments).filter((c) => show === 'all' || (show === 'open' ? !c.resolved : c.resolved)).sort((a, b) => (b.at || 0) - (a.at || 0));
  return <div style={{ display: 'flex', flexDirection: 'column', height: '100%', minHeight: 0 }}>
    <div style={{ display: 'flex', alignItems: 'center', height: 32, padding: '0 8px 0 12px', borderBottom: '1px solid var(--line)', gap: 8 }}><span style={{ fontWeight: 600 }}>Review</span><div style={{ marginLeft: 'auto' }}><Seg value={show} onChange={setShow} options={[['open', 'Open'], ['resolved', 'Resolved'], ['all', 'All']]} /></div></div>
    <div style={{ padding: '8px 12px', color: 'var(--muted)', borderBottom: '1px solid var(--line)' }}>Pin comments with the comment tool in 2D or 3D. Everyone with access sees them live.</div>
    <div style={{ flex: 1, overflowY: 'auto' }}>
      {!list.length && <div style={{ padding: 16, color: 'var(--faint)' }}>No {show === 'all' ? '' : show} comments.</div>}
      {list.map((c) => <div key={c.id} className={'atl-list' + (app.sel && app.sel.id === c.id ? ' on' : '')} style={{ alignItems: 'flex-start', padding: '8px 12px', minHeight: 0 }} onClick={() => { app.setSel({ kind: 'comment', id: c.id }); A.focusComment(c); }}>
        <span style={{ width: 20, height: 20, borderRadius: 999, background: c.resolved ? 'var(--faint)' : 'var(--accent)', color: '#fff', fontSize: 10, fontWeight: 700, display: 'flex', alignItems: 'center', justifyContent: 'center', flex: 'none' }}>{c.n}</span>
        <div style={{ minWidth: 0, flex: 1 }}><div style={{ color: 'var(--muted)', fontSize: 10.5 }}>{c.byName} · {(d.pieces[c.p] || {}).name} · {new Date(c.at).toLocaleDateString()}</div><div style={{ marginTop: 2, whiteSpace: 'pre-wrap' }}>{c.text}</div>{AT.asArray(c.replies).length > 0 && <div style={{ color: 'var(--faint)', marginTop: 3 }}>{AT.asArray(c.replies).length} repl{AT.asArray(c.replies).length > 1 ? 'ies' : 'y'}</div>}</div>
      </div>)}
    </div>
  </div>;
}
function TPHeading(props) { return <div style={{ display: 'flex', alignItems: 'baseline', gap: 10, margin: '28px 0 10px' }}><div style={{ fontSize: 15, fontWeight: 650, letterSpacing: '-.01em' }}>{props.children}</div>{props.right}</div>; }
function TechPackView() {
  const app = useApp(), d = app.doc, A = app.actions;
  const D = React.useMemo(() => AT.techData(d), [app.rev, d]);
  const tp = d.techpack || {};
  const setTp = (k, v) => { d.techpack = Object.assign({}, d.techpack || {}, { [k]: v }); A.commit('Tech pack'); };
  const imgs = app.tpImages || {};
  React.useEffect(() => { if (!imgs.front) A.captureTechImages(); }, []);
  const H = TPHeading;
  return <div style={{ height: '100%', overflowY: 'auto', background: 'var(--bg)' }}>
    <div style={{ maxWidth: 1080, margin: '0 auto', padding: '24px 28px 60px' }}>
      <div style={{ display: 'flex', alignItems: 'flex-start', gap: 12, flexWrap: 'wrap' }}>
        <div style={{ flex: 1, minWidth: 260 }}>
          <div style={{ fontSize: 10.5, fontWeight: 600, letterSpacing: '.08em', textTransform: 'uppercase', color: 'var(--muted)' }}>Tech pack</div>
          <div style={{ fontSize: 26, fontWeight: 650, letterSpacing: '-.02em', marginTop: 4 }}>{d.meta.name}</div>
          <div style={{ color: 'var(--muted)', marginTop: 4 }}>Size range {d.sizes.join(' · ')} — base {d.baseSize} — {(d.colorways[d.activeColorway] || {}).name}</div>
        </div>
        <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap' }}>
          <button className="atl-btn" onClick={A.captureTechImages}><Icon name="camera" size={13} />Refresh images</button>
          <button className="atl-btn" onClick={A.exportCSV}>CSV</button>
          <button className="atl-btn" onClick={() => A.openPatternPDF([])}><Icon name="pdf" size={13} />Pattern PDF…</button>
          <button className="atl-btn primary" onClick={A.exportPDF}><Icon name="download" size={13} />Export PDF</button>
        </div>
      </div>
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(150px, 1fr))', gap: 12, marginTop: 20 }}>
        {['front', 'back', 'side'].map((k) => <div key={k} style={{ aspectRatio: '3 / 4.4', borderRadius: 12, background: 'var(--panel)', border: '1px solid var(--line)', overflow: 'hidden', position: 'relative' }}>
          {imgs[k] ? <img src={imgs[k]} style={{ width: '100%', height: '100%', objectFit: 'cover' }} /> : <div style={{ position: 'absolute', inset: 0, display: 'flex', alignItems: 'center', justifyContent: 'center', color: 'var(--faint)' }}>Rendering…</div>}
          <span className="atl-chip" style={{ position: 'absolute', left: 8, bottom: 8 }}>{k}</span>
        </div>)}
        <div style={{ display: 'grid', gap: 8, alignContent: 'start' }}>
          {[['season', 'Season'], ['designer', 'Designer'], ['currency', 'Currency']].map((f) => <label key={f[0]} style={{ display: 'grid', gap: 3 }}><span style={{ color: 'var(--muted)', fontSize: 10.5, textTransform: 'uppercase', letterSpacing: '.06em', fontWeight: 600 }}>{f[1]}</span><Txt value={tp[f[0]] || (f[0] === 'designer' ? app.userName : '')} onChange={(v) => setTp(f[0], v)} /></label>)}
          <label style={{ display: 'grid', gap: 3 }}><span style={{ color: 'var(--muted)', fontSize: 10.5, textTransform: 'uppercase', letterSpacing: '.06em', fontWeight: 600 }}>Notes</span><Txt area rows={4} value={tp.notes} onChange={(v) => setTp('notes', v)} /></label>
        </div>
      </div>
      <H right={<span style={{ color: 'var(--muted)' }}>Est. fabric cost {D.totalCost ? D.totalCost.toFixed(2) + ' ' + (tp.currency || '') : '—'}</span>}>Bill of materials</H>
      <table className="atl-tbl"><thead><tr><th></th><th>Material</th><th>Placement</th><th className="r">g/m²</th><th className="r">Width</th><th className="r">Cut area m²</th><th className="r">Consumption m</th><th className="r">Price/m</th><th className="r">Cost</th></tr></thead><tbody>
        {D.fabrics.map((f) => <tr key={f.id}><td><span className="atl-sw" style={{ background: f.color, display: 'inline-block' }} /></td><td style={{ fontWeight: 500 }}>{f.name}<div style={{ color: 'var(--faint)' }}>{f.content}</div></td><td style={{ color: 'var(--muted)' }}>{f.placement}</td><td className="r">{f.weight}</td><td className="r">{f.width}</td><td className="r">{f.areaGross.toFixed(3)}</td><td className="r">{f.linear.toFixed(2)}</td><td className="r">{f.price || '—'}</td><td className="r">{f.cost ? f.cost.toFixed(2) : '—'}</td></tr>)}
        {D.trims.map((t, i) => <tr key={'t' + i}><td>{t.color ? <span className="atl-sw" style={{ background: t.color, display: 'inline-block', borderRadius: 999 }} /> : null}</td><td>{t.name}</td><td></td><td></td><td></td><td></td><td className="r">{t.qty} {t.unit}</td><td></td><td></td></tr>)}
      </tbody></table>
      <H>Points of measure <span style={{ color: 'var(--muted)', fontWeight: 400, fontSize: 12 }}>({AT.unitLabel(app.units)})</span></H>
      {D.poms.length ? <div style={{ overflowX: 'auto' }}><table className="atl-tbl"><thead><tr><th>POM</th><th className="r">Tol ±</th>{d.sizes.map((s) => <th key={s} className="r">{s}{s === d.baseSize ? ' ●' : ''}</th>)}</tr></thead><tbody>
        {D.poms.map((p) => <tr key={p.id}><td>{p.name}</td><td className="r">{AT.fmt(p.tol, app.units)}</td>{d.sizes.map((s) => <td key={s} className="r" style={{ fontWeight: s === d.baseSize ? 600 : 400 }}>{AT.fmt(p.vals[s], app.units)}</td>)}</tr>)}
      </tbody></table></div> : <div style={{ color: 'var(--faint)' }}>No points of measure yet — add them from the POM tab.</div>}
      <H>Pattern pieces</H>
      <table className="atl-tbl"><thead><tr><th>Piece</th><th>Fabric</th><th className="r">Qty</th><th className="r">Net cm²</th><th className="r">Cut cm²</th><th className="r">Perimeter</th><th className="r">SA</th><th className="r">Grain</th></tr></thead><tbody>
        {D.patterns.map((p, i) => <tr key={i}><td style={{ fontWeight: 500 }}>{p.name}</td><td style={{ color: 'var(--muted)' }}>{p.fabric}</td><td className="r">{p.qty}</td><td className="r">{p.net.toFixed(0)}</td><td className="r">{p.gross.toFixed(0)}</td><td className="r">{AT.fmt(p.perim, app.units)}</td><td className="r">{p.sa}</td><td className="r">{p.grain}°</td></tr>)}
      </tbody></table>
      <H>Sewing</H>
      <table className="atl-tbl"><thead><tr><th></th><th>#</th><th>Side A</th><th>Side B</th><th className="r">A</th><th className="r">B</th><th className="r">Δ</th></tr></thead><tbody>
        {D.sewing.map((s) => <tr key={s.n}><td><span style={{ display: 'inline-block', width: 4, height: 14, borderRadius: 2, background: s.color }} /></td><td>{s.n}</td><td>{s.a}</td><td>{s.b}</td><td className="r">{AT.fmt(s.la, app.units)}</td><td className="r">{AT.fmt(s.lb, app.units)}</td><td className="r" style={{ color: Math.abs(s.diff) > 3 ? 'var(--accent)' : 'var(--muted)' }}>{AT.fmt(s.diff, app.units)}</td></tr>)}
      </tbody></table>
      <H>Colorways</H>
      <table className="atl-tbl"><thead><tr><th>Material</th>{AT.asArray(d.colorways).sort((a, b) => (a.order || 0) - (b.order || 0)).map((c) => <th key={c.id}>{c.name}</th>)}</tr></thead><tbody>
        {Object.values(d.fabrics).map((f) => <tr key={f.id}><td>{f.name}</td>{AT.asArray(d.colorways).sort((a, b) => (a.order || 0) - (b.order || 0)).map((c) => { const col = (c.over && c.over[f.id] && c.over[f.id].color) || f.color; return <td key={c.id}><span style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}><span className="atl-sw" style={{ background: col }} /><span className="atl-mono" style={{ color: 'var(--muted)' }}>{col.toUpperCase()}</span></span></td>; })}</tr>)}
      </tbody></table>
    </div>
  </div>;
}


function StatusBar() {
  const app = useApp(), d = app.doc;
  const [st, setSt] = React.useState({});
  React.useEffect(() => { const t = setInterval(() => setSt(Object.assign({}, app.statsRef.current)), 500); return () => clearInterval(t); }, []);
  const cur = st.cursor;
  return <div style={{ display: 'flex', alignItems: 'center', gap: 14, height: 26, padding: '0 10px', borderTop: '1px solid var(--line)', background: 'var(--panel)', color: 'var(--muted)', fontSize: 11, flex: 'none', whiteSpace: 'nowrap', overflow: 'hidden' }}>
    <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}><span style={{ width: 7, height: 7, borderRadius: 999, background: app.simOn ? 'var(--accent)' : 'var(--faint)', animation: app.simOn ? 'atlpulse 1.2s ease-in-out infinite' : 'none' }} />{app.simOn ? 'Simulating' : 'Idle'}</span>
    <span className="atl-mono">{st.particles ? st.particles.toLocaleString() + ' particles' : ''}</span>
    <span className="atl-mono">{st.fps ? Math.round(st.fps) + ' fps' : ''}</span>
    {st.solver && <span title={AT.GPU.state === 'ready' ? 'Cloth simulation runs as WebGPU compute shaders on the GPU' : (AT.GPU.error || 'WebGPU unavailable')} style={{ display: 'inline-flex', alignItems: 'center', gap: 5, color: AT.GPU.state === 'ready' && !(AT.GPU.info || {}).fallback ? 'var(--text)' : 'var(--muted)' }}><span style={{ width: 6, height: 6, borderRadius: 2, background: AT.GPU.state === 'ready' ? '#4ade80' : 'var(--faint)' }} />{st.solver}{st.gpuMs ? ' · ' + st.gpuMs.toFixed(1) + ' ms/frame' : ''}</span>}
    {app.simOn && st.gap > 3 && <span className="atl-mono">sewing gap {AT.fmt(st.gap, app.units)} {AT.unitLabel(app.units)}</span>}
    <div style={{ flex: 1 }} />
    {cur && <span className="atl-mono">x {AT.fmt(cur.x, app.units)}  y {AT.fmt(cur.y, app.units)}</span>}
    <span>Size <b style={{ color: 'var(--text)' }}>{d.activeSize}</b></span>
    <span>{(d.colorways[d.activeColorway] || {}).name}</span>
    <span>{AT.unitLabel(app.units)}</span>
  </div>;
}
function TemplatePicker(props) {
  const app = useApp();
  const [sex, setSex] = React.useState('female');
  const sil = {
    tee: 'M22 10l10-4h16l10 4 12 10-7 9-7-5v38H24V24l-7 5-7-9z',
    dress: 'M30 6h20l2 10 8 10-6 6v8l10 30H16l10-30v-8l-6-6 8-10z',
    skirt: 'M24 14h32l2 6 10 42H12l10-42z',
    labadeh: 'M34 8l-10 3-12 29 6 2 8-20-6 50h40l-6-50 8 20 6-2-12-29-10-3z M34 8V4h12v4 M40 8v64',
    aba: 'M33 6L8 31l4 39h56l4-39L47 6z M37 8l-2 62 M43 8l2 62 M10 33l2 12 M70 33l-2 12',
    blank: 'M20 12h40v56H20z'
  };
  return <div>
    <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 14 }}><span style={{ color: 'var(--muted)' }}>Avatar</span><Seg value={sex} onChange={setSex} options={[['female', 'Female'], ['male', 'Male']]} /></div>
    <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(150px, 1fr))', gap: 10 }}>
      {AT.TEMPLATES.map((t) => <button key={t.id} onClick={() => app.actions.createProject(t.id, t.sex || sex)} style={{ textAlign: 'left', padding: 12, borderRadius: 12, border: '1px solid var(--line)', background: 'var(--panel2)', color: 'var(--text)', cursor: 'pointer', font: 'inherit' }}>
        <svg width="100%" height="92" viewBox="0 0 80 74" style={{ display: 'block', marginBottom: 8 }}><path d={sil[t.id]} fill="none" stroke="var(--accent)" strokeWidth="1.4" strokeLinejoin="round" /></svg>
        <div style={{ fontWeight: 600 }}>{t.name}</div><div style={{ color: 'var(--muted)', fontSize: 11, marginTop: 2 }}>{t.desc}</div>
      </button>)}
    </div>
  </div>;
}
function StartScreen() {
  const app = useApp();
  return <div style={{ flex: 1, overflowY: 'auto' }}>
    <div style={{ maxWidth: 860, margin: '0 auto', padding: '48px 28px' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
        <svg width="26" height="26" viewBox="0 0 24 24" fill="none"><path d="M12 3c2.4 3.6 6 4.8 6 9.4A6 6 0 0 1 12 18.5 6 6 0 0 1 6 12.4C6 7.8 9.6 6.6 12 3z" stroke="var(--accent)" strokeWidth="1.5" /><path d="M12 9v12" stroke="var(--accent)" strokeWidth="1.5" strokeLinecap="round" /></svg>
        <div style={{ fontSize: 22, fontWeight: 650, letterSpacing: '-.02em' }}>Atelier</div>
      </div>
      <div style={{ color: 'var(--muted)', marginTop: 6, maxWidth: 560, lineHeight: 1.5 }}>Pattern drafting, physical cloth simulation, grading, colorways and tech packs — in one live, shared workspace.</div>
      {app.missing && <div style={{ marginTop: 16, padding: '10px 12px', borderRadius: 10, background: 'var(--accentSoft)', color: 'var(--text)' }}>That project no longer exists. Start a new one or open another.</div>}
      <div style={{ fontSize: 10.5, fontWeight: 600, letterSpacing: '.08em', textTransform: 'uppercase', color: 'var(--muted)', margin: '32px 0 12px' }}>New garment</div>
      <TemplatePicker />
      <div style={{ fontSize: 10.5, fontWeight: 600, letterSpacing: '.08em', textTransform: 'uppercase', color: 'var(--muted)', margin: '32px 0 8px' }}>Recent</div>
      <div style={{ borderRadius: 12, border: '1px solid var(--line)', background: 'var(--panel)', overflow: 'hidden' }}><ProjectList /></div>
    </div>
  </div>;
}
AT.COVERAGE = [
  ['Pattern drafting', 'built', 'Polygon, rectangle, ellipse, internal polygon/line/ellipse, notches, curve points, edit curvature, add point / split line (exact curve subdivision), transform (move/rotate/flip), linked symmetric pattern, per-piece and per-segment seam allowance, grain line, segment length entry, point & grid snapping, mm/cm/in.'],
  ['Sewing', 'built', 'Segment sewing, free sewing, M:N sewing, edit/flip/delete, live length-mismatch flags, 2D and 3D sewing lines.'],
  ['Cloth simulation', 'built', 'Runs on the GPU as WebGPU compute shaders (hardware-accelerated; graph-coloured parallel XPBD), with the CPU solver as fallback when WebGPU is unavailable. XPBD solver: anisotropic weft/warp/bias stretch from 2D modulus, bending, buckling (soft compression), density, thickness, friction, internal damping, shrinkage weft/warp, elastic segments, pressure, fold angle on internal lines, layers, freeze/deactivate/strengthen, pins, cloth dragging, avatar collision, inter-piece collision with cloth-on-cloth friction, per-garment gravity hold while its own seams close, Fast/Normal/Complete.'],
  ['Arrangement', 'built', 'Arrangement points around body, arms, legs and neck; offset, angle, height, lateral shift, wrap/flat, flip; reset 3D arrangement (all/selected).'],
  ['Avatar', 'built', 'MakeHuman female and male bodies (CC0 1.0 base mesh, morph targets and skin weights) fitted to 19 tailor measurements plus height by least squares, with measured value and deviation per field, auto fields, body fat and muscle, size charts, saved and imported avatar sizes, measurement tapes drawn on the body, A-pose / relaxed / open / T-pose arm angle and leg stance, skin tones, OBJ export of the body. Not yet: poses beyond arm and leg angles, motion, hair and shoes, imported third-party avatars.'],
  ['Materials', 'built', '13 physically-tuned fabric presets, 10 editable physical properties, 15 procedural weaves/prints with relief, image textures, repeat size, roughness, opacity, colorway overrides.'],
  ['Trims & details', 'partial', 'Buttons (4 styles), buttonholes, button fastening, multi-row topstitch, graphics/prints. Not yet: zippers, puckering, drawstrings, 3D trims library.'],
  ['Fit analysis', 'built', 'Strain map (%), stress map (kPa), fit map with Tight / Can\u2019t-wear coverage, pressure points.'],
  ['Grading', 'built', 'Size group, base size, per-point grade rules, proportional auto-grade, nest view, per-size 3D drape, graded POM table.'],
  ['Pattern dimensions', 'built', 'Net width × length drawn on every piece in 2D (toggle in the 2D view bar or Display menu), listed in the Object Browser, with cut size, perimeter and area in the Property Editor.'],
  ['Hems & seam numbers', 'built', 'Per-edge hem allowance (Segment → Edge finish, or Sewing → Mark hems automatically) with folded hem corners; every seam has a stable number shown on both edges in 2D, in the Object Browser and in PDFs; edges are classed as sewn or free.'],
  ['Pattern PDF', 'built', 'Piece per page (scaled to fit, small pieces at 1:1, with scale bar), overview sheet, full-scale tiled print (assembly map, test square, edge labels) and plotter roll; cut line, shaded seam and hem allowances with hem fold lines, stitching line on sewn edges and finished line on free edges, seam numbers on both edges of every seam, notches, grain, internal lines, nested sizes, a piece schedule and a sewing schedule. CAD dimensioning: every edge with an aligned or arc-length dimension (notch positions chained), ordinates to every corner from a datum, overall size, lettered corners with interior angles, right-angle and centre-line marks, a title block per sheet and dimension tables (X, Y, angle, length, chord, seam, finish). Persian and other non-Latin names are rendered correctly.'],
  ['Tech pack', 'built', 'BOM with consumption & costing, trims, graded POMs, pattern list, sewing table, colorways, review notes, front/back/side renders; PDF and CSV.'],
  ['Import / export', 'built', 'DXF (AAMA-style blocks: cut, sew, internal, grain, notches) export and import, SVG 1:1, OBJ + MTL with pattern UVs (garment ± avatar), project JSON, PNG snapshots, turntable video.'],
  ['Collaboration', 'built', 'Live multi-user editing, presence, comments pinned in 2D/3D with replies & resolve, version history, undo/redo.'],
  ['Self-collision', 'built', 'Vertex-triangle self-collision and collision between pieces and layers on the GPU: a spatial hash of all cloth triangles is rebuilt every other substep; each particle keeps a collision thickness on the side it came from, layers keep an extra gap sized to the coarser mesh, and contacts carry cloth-on-cloth friction (fabric friction × 2) so an outer garment rests on the one beneath it. Where an outer layer is clearly nearer the body than the layer it touches, it is pushed back outside. Edge-edge crossings of very coarse meshes are not tested.'],
  ['Rendering', 'partial', 'Real-time PBR with soft shadows and tone mapping. Not yet: path-traced offline renderer.'],
  ['Pattern tools (advanced)', 'planned', 'Dart and pleat tools, trace, walk patterns, slash & spread, offset, blend curves, unfold half patterns.'],
  ['Animation', 'planned', 'Avatar motion / animation mode, hanger presets, garment-on-hanger.'],
  ['3D tools (advanced)', 'planned', 'Sewing in the 3D window, 3D pen, auto-fit to other avatars, UV editor, marker / print layout, modular configurator, zprj import (proprietary format).']
];
function PField(props) {
  return <div style={{ display: 'grid', gridTemplateColumns: '92px 1fr', alignItems: 'center', gap: 10 }}><span style={{ color: 'var(--muted)' }}>{props.label}</span><div style={{ minWidth: 0 }}>{props.children}</div></div>;
}
function PatternPDFDialog(props) {
  const app = useApp(), d = app.doc, A = app.actions;
  const selIds = props.ids && props.ids.length ? props.ids : null;
  const [o, setO] = React.useState(() => { let sv = {}; try { sv = JSON.parse(localStorage.getItem('atl-ppdf') || '{}'); } catch (e) { } return Object.assign({ layout: 'pieces', paper: 'a4', plotter: 914, sizes: 'active', sa: true, dims: true, lengths: false }, sv, { scope: selIds ? 'sel' : 'all' }); });
  const [busy, setBusy] = React.useState(false);
  const set = (p) => setO(Object.assign({}, o, p));
  const opts = Object.assign({}, o, { ids: o.scope === 'sel' ? selIds : null, units: app.units });
  let plan; try { plan = AT.ppdfPlan(d, opts); } catch (e) { plan = { items: [], pages: 0, note: e.message }; }
  const run = async () => {
    setBusy(true);
    try { const keep = Object.assign({}, o); delete keep.scope; localStorage.setItem('atl-ppdf', JSON.stringify(keep)); } catch (e) { }
    const ok = await A.exportPatternPDF(opts);
    setBusy(false); if (ok) props.close();
  };
  return <Modal title="Export pattern PDF" onClose={props.close} width={600} footer={<><button className="atl-btn ghost" onClick={props.close}>Cancel</button><button className="atl-btn primary" disabled={busy || !plan.items.length} onClick={run}><Icon name="pdf" size={13} />{busy ? 'Building…' : 'Export PDF'}</button></>}>
    <div style={{ display: 'grid', gap: 12 }}>
      <PField label="Layout"><Seg value={o.layout} onChange={(v) => set({ layout: v })} options={[['pieces', 'Piece per page'], ['overview', 'Overview'], ['tiled', 'Full scale · tiled'], ['plotter', 'Full scale · plotter']]} /></PField>
      {o.layout !== 'plotter'
        ? <PField label="Paper"><Seg value={o.paper} onChange={(v) => set({ paper: v })} options={[['a4', 'A4'], ['a3', 'A3'], ['letter', 'Letter'], ['tabloid', 'Tabloid']]} /></PField>
        : <PField label="Roll width"><Seg value={o.plotter} onChange={(v) => set({ plotter: v })} options={AT.PLOTTER} /></PField>}
      {selIds && <PField label="Pieces"><Seg value={o.scope} onChange={(v) => set({ scope: v })} options={[['sel', 'Selected · ' + selIds.length], ['all', 'All']]} /></PField>}
      {d.sizes.length > 1 && <PField label="Sizes"><Seg value={o.sizes} onChange={(v) => set({ sizes: v })} options={[['active', 'Size ' + d.activeSize], ['all', 'All sizes nested']]} /></PField>}
      <PField label="Hems">{(() => { const n = AT.asArray(d.pieces).reduce((t, p) => t + Object.values(p.hem || {}).filter((v) => v > 0).length, 0); return <div style={{ display: 'flex', alignItems: 'center', gap: 10, flexWrap: 'wrap' }}><span style={{ color: n ? 'var(--text)' : 'var(--muted)' }}>{n ? n + ' hem edge' + (n > 1 ? 's' : '') + ' marked' : 'None marked'}</span><button className="atl-btn ghost" onClick={A.detectHems}>Mark hems automatically</button></div>; })()}</PField>
      <PField label="Include"><div style={{ display: 'flex', gap: 16, flexWrap: 'wrap' }}><Check value={o.sa} label="Seam allowance" onChange={(v) => set({ sa: v })} /><Check value={o.dims} label="CAD dimensions" onChange={(v) => set({ dims: v })} />{!o.dims && <Check value={o.lengths} label="Segment lengths" onChange={(v) => set({ lengths: v })} />}</div></PField>
      <div style={{ padding: '10px 12px', borderRadius: 10, background: 'var(--panel2)', border: '1px solid var(--line)', color: 'var(--muted)', lineHeight: 1.5 }}>
        <span style={{ color: 'var(--text)', fontWeight: 600 }}>{plan.items.length} piece{plan.items.length === 1 ? '' : 's'} · {plan.pages} page{plan.pages === 1 ? '' : 's'}</span> — {plan.note} Lengths in {AT.unitLabel(app.units)}.
      </div>
    </div>
  </Modal>;
}
function Dialogs() {
  const app = useApp(), dlg = app.dialog;
  if (!dlg) return null;
  const close = () => app.openDialog(null);
  if (dlg.name === 'open') return <Modal title="Open garment" onClose={close} pad={false} width={560}><ProjectList /></Modal>;
  if (dlg.name === 'new') return <Modal title="New garment" onClose={close} width={720}><TemplatePicker /></Modal>;
  if (dlg.name === 'confirm') return <Modal title={dlg.title || 'Confirm'} onClose={close} width={420} footer={<><button className="atl-btn ghost" onClick={close}>Cancel</button><button className="atl-btn primary" onClick={() => { close(); dlg.run(); }}>{dlg.ok || 'Confirm'}</button></>}><div style={{ lineHeight: 1.5 }}>{dlg.text}</div></Modal>;
  if (dlg.name === 'saveVersion') return <SaveVersion close={close} />;
  if (dlg.name === 'versions') return <Versions close={close} />;
  if (dlg.name === 'sa') return <SAAll close={close} />;
  if (dlg.name === 'patternPdf') return <PatternPDFDialog close={close} ids={dlg.ids} />;
  if (dlg.name === 'shortcuts') return <Modal title="Keyboard shortcuts" onClose={close} width={640}>
    <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '4px 24px' }}>
      {[['Simulate', 'Space'], ['Select / Move (3D)', 'Q'], ['Pin (3D)', 'W'], ['Transform pattern', 'A'], ['Edit pattern', 'Z'], ['Edit curvature', 'C'], ['Edit curve point', 'V'], ['Add point / split', 'X'], ['Polygon', 'H'], ['Rectangle', 'S'], ['Ellipse', 'E'], ['Internal polygon / line', 'G'], ['Internal ellipse', 'R'], ['Segment sewing', 'N'], ['Free sewing', 'M'], ['Edit sewing', 'B'], ['Segment topstitch', 'K'], ['Edit topstitch', 'J'], ['Graphic', 'T'], ['Library', 'Y'], ['Show avatar', '⇧A'], ['Arrangement points', '⇧F'], ['Hide selected 3D', '⇧Q'], ['Show all 3D', '⇧W'], ['Stress / strain / fit / pressure', '⌥6 ⌥7 ⌥8 ⌥9'], ['Front / back / left / right', '2 8 6 4'], ['¾ views / top / bottom', '3 1 5 0'], ['Focus', 'F'], ['Undo / redo', '⌘Z  ⇧⌘Z'], ['Copy / paste / duplicate', '⌘C ⌘V ⌘D'], ['Mirror paste (linked)', '⌘R'], ['Freeze / deactivate / strengthen', '⌘K ⌘J ⌘H'], ['Nudge (⇧ = 10 mm)', '← → ↑ ↓'], ['Finish polygon / M:N side', 'Enter'], ['Cancel', 'Esc'], ['Pan 2D / orbit 3D', 'right-drag'], ['Pan 3D', 'middle-drag'], ['Zoom', 'wheel / pinch']].map((r) => <div key={r[0]} style={{ display: 'flex', justifyContent: 'space-between', padding: '4px 0', borderBottom: '1px solid var(--line)' }}><span>{r[0]}</span><span className="atl-kbd">{r[1]}</span></div>)}
    </div></Modal>;
  if (dlg.name === 'coverage') return <Modal title="CLO feature coverage" onClose={close} width={760}>
    <div style={{ color: 'var(--muted)', marginBottom: 12, lineHeight: 1.5 }}>An honest map of what Atelier does today against CLO's feature set. Built = working end-to-end; partial = working with stated limits; planned = not yet available.</div>
    <table className="atl-tbl"><tbody>{AT.COVERAGE.map((r) => <tr key={r[0]}><td style={{ fontWeight: 600, width: 170 }}>{r[0]}</td><td style={{ width: 80 }}><span className="atl-chip" style={{ color: r[1] === 'built' ? '#10B981' : r[1] === 'partial' ? '#F59E0B' : 'var(--muted)', borderColor: 'currentColor' }}>{r[1]}</span></td><td style={{ color: 'var(--muted)', lineHeight: 1.45 }}>{r[2]}</td></tr>)}</tbody></table>
  </Modal>;
  if (dlg.name === 'help') return <Modal title="Quick start" onClose={close} width={560}>
    <ol style={{ margin: 0, paddingLeft: 18, lineHeight: 1.7 }}>
      <li>Start from a template in <b>Library → Garments</b>, or draw a pattern with <span className="atl-kbd">H</span> / <span className="atl-kbd">S</span> / <span className="atl-kbd">E</span>.</li>
      <li>Edit shapes with <span className="atl-kbd">Z</span> (points, segments), <span className="atl-kbd">C</span> (curvature) and <span className="atl-kbd">X</span> (split).</li>
      <li>Sew with <span className="atl-kbd">N</span>: click near the start of each segment so directions match. Hold Shift for M:N, then Enter.</li>
      <li>Place pieces: select a pattern, press <span className="atl-kbd">⇧F</span>, click an arrangement point.</li>
      <li>Press <span className="atl-kbd">Space</span> to simulate. Drag cloth in 3D, pin with <span className="atl-kbd">W</span>.</li>
      <li>Check fit with the strain / stress / fit maps, set fabrics in the Object Browser, then grade sizes and export the tech pack.</li>
    </ol></Modal>;
  return null;
}
function SaveVersion(props) {
  const app = useApp(); const [name, setName] = React.useState('Version ' + new Date().toLocaleString());
  return <Modal title="Save version" onClose={props.close} width={420} footer={<><button className="atl-btn ghost" onClick={props.close}>Cancel</button><button className="atl-btn primary" onClick={() => { app.actions.writeVersion(name); props.close(); }}>Save</button></>}>
    <input className="atl-in" autoFocus value={name} onChange={(e) => setName(e.target.value)} onKeyDown={(e) => { e.stopPropagation(); if (e.key === 'Enter') { app.actions.writeVersion(name); props.close(); } }} />
  </Modal>;
}
function Versions(props) {
  const app = useApp(); const [list, setList] = React.useState(null);
  React.useEffect(() => { const r = FS.ref(FS.db, app.base + '/versions/' + app.pid); const un = FS.onValue(r, (s) => setList(s.val() || {}), () => setList({})); return () => un(); }, [app.pid]);
  const items = list ? Object.entries(list).map(([k, v]) => Object.assign({ id: k }, v)).sort((a, b) => b.at - a.at) : [];
  return <Modal title="Version history" onClose={props.close} width={520} pad={false}>
    {!list ? <div style={{ padding: 16, color: 'var(--muted)' }}>Loading…</div> : !items.length ? <div style={{ padding: 16, color: 'var(--muted)' }}>No saved versions yet. Use File → Save version.</div> :
      items.map((v) => <div key={v.id} className="atl-list" style={{ minHeight: 46 }}><Icon name="history" size={16} /><div style={{ flex: 1, minWidth: 0 }}><div style={{ fontWeight: 500 }}>{v.name}</div><div style={{ color: 'var(--faint)', fontSize: 10.5 }}>{v.byName} · {new Date(v.at).toLocaleString()}</div></div>
        <button className="atl-btn" onClick={() => { app.actions.restoreVersion(v); props.close(); }}>Restore</button><IB sm icon="trash" tip="Delete" onClick={() => FS.remove(FS.ref(FS.db, app.base + '/versions/' + app.pid + '/' + v.id))} /></div>)}
  </Modal>;
}
function SAAll(props) {
  const app = useApp(); const [v, setV] = React.useState(10);
  return <Modal title="Seam allowance on all patterns" onClose={props.close} width={380} footer={<><button className="atl-btn ghost" onClick={props.close}>Cancel</button><button className="atl-btn primary" onClick={() => { app.actions.setSAAll(v); props.close(); }}>Apply</button></>}>
    <Row label="Width"><Num value={v} min={0} max={80} onChange={setV} /></Row>
  </Modal>;
}
function ContextMenu() {
  const app = useApp(), c = app.ctx;
  React.useEffect(() => { const h = () => app.setCtx(null); window.addEventListener('pointerdown', h); return () => window.removeEventListener('pointerdown', h); }, []);
  if (!c) return null;
  const left = Math.min(c.x, (app.width || 1200) - 230), top = Math.min(c.y, (app.height || 800) - Math.min(420, c.items.length * 27 + 12));
  return <MenuList items={c.items} onClose={() => app.setCtx(null)} style={{ left: left, top: Math.max(4, top), maxHeight: 440, overflowY: 'auto' }} />;
}
function Toasts() {
  const app = useApp();
  return <div style={{ position: 'absolute', left: '50%', bottom: 38, transform: 'translateX(-50%)', display: 'grid', gap: 6, zIndex: 90, pointerEvents: 'none' }}>
    {app.toasts.map((t) => <div key={t.id} className="atl-pop" style={{ padding: '7px 12px', borderRadius: 999, background: 'var(--text)', color: 'var(--bg)', fontWeight: 500, boxShadow: 'var(--shadow)', whiteSpace: 'nowrap' }}>{t.text}</div>)}
  </div>;
}
function CommentComposer() {
  const app = useApp(), c = app.composer; const [txt, setTxt] = React.useState('');
  if (!c) return null;
  const send = () => { if (txt.trim()) app.actions.createComment(c, txt.trim()); app.setComposer(null); };
  return <Modal title="New comment" onClose={() => app.setComposer(null)} width={420} footer={<><button className="atl-btn ghost" onClick={() => app.setComposer(null)}>Cancel</button><button className="atl-btn primary" onClick={send}>Post</button></>}>
    <div style={{ color: 'var(--muted)', marginBottom: 6 }}>On {(app.doc.pieces[c.p] || {}).name}</div>
    <textarea className="atl-in" rows={4} autoFocus value={txt} onChange={(e) => setTxt(e.target.value)} onKeyDown={(e) => { e.stopPropagation(); if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) send(); }} placeholder="What should change here?" />
  </Modal>;
}


const COLL = ['pieces', 'sewing', 'fabrics', 'buttons', 'holes', 'stitches', 'poms', 'comments', 'colorways'];
const AV_KEYS = ['sex', 'fat', 'muscle', 'armAngle', 'legSpread'].concat(AT.MEAS.map(function (d) { return d.k; }));
const PALETTE = {
  dark: { canvas2d: '#202125', grid: 'rgba(255,255,255,0.035)', gridMajor: 'rgba(255,255,255,0.08)', ink: '#d9d9de', inkHover: '#ffffff', accent: '#E8654A', muted: '#7d7e86', sa: 'rgba(220,220,225,0.45)', internal: '#a8a9b0', label: '#e6e6ea', text: '#e6e6ea', chip: 'rgba(30,31,35,0.9)', dim: '#8fb0e0', pointFill: '#202125', hover: '#E8654A', boxFill: 'rgba(232,101,74,0.08)', panel: '#1e1f23' },
  light: { canvas2d: '#fbfbfb', grid: 'rgba(0,0,0,0.035)', gridMajor: 'rgba(0,0,0,0.08)', ink: '#2a2a2e', inkHover: '#000000', accent: '#E8654A', muted: '#8a8a92', sa: 'rgba(40,40,46,0.4)', internal: '#5b5b63', label: '#1f1f23', text: '#1f1f23', chip: 'rgba(255,255,255,0.92)', dim: '#3f6296', pointFill: '#ffffff', hover: '#E8654A', boxFill: 'rgba(232,101,74,0.08)', panel: '#ffffff' }
};
function UserDiv() {
  const { theme } = useTheme();
  const dark = theme === 'dark';
  const base = relPathAddress();
  const assets = typeof useStorageUrl === 'function' ? useStorageUrl('assets') : null;
  const yj = typeof useYjs === 'function' ? useYjs('presence', { ephemeral: true }) : null;
  const darkRef = React.useRef(dark); darkRef.current = dark;
  AT.assetBase = assets && assets.url ? assets.url : null;
  AT.encodePath = assets && assets.encodePath ? assets.encodePath : function (p) { return p.split('/').map(encodeURIComponent).join('/'); };
  const userName = (FS.auth && FS.auth.currentUser && (FS.auth.currentUser.displayName || (FS.auth.currentUser.email || '').split('@')[0])) || 'Designer';
  const myColor = React.useMemo(() => { let h = 0; String(userId || 'x').split('').forEach((c) => { h = (h * 31 + c.charCodeAt(0)) >>> 0; }); return 'hsl(' + (h % 360) + ',55%,52%)'; }, []);

  const readPid = () => { try { return new URLSearchParams(window.location.search).get('p'); } catch (e) { return null; } };
  const [pid, setPid] = React.useState(readPid);
  const docRef = React.useRef(null);
  const [rev, setRev] = React.useState(0);
  const bump = () => setRev((r) => r + 1);
  const [missing, setMissing] = React.useState(false);
  const [index, setIndex] = React.useState({});
  const [libsReady, setLibsReady] = React.useState(!!(window.THREE && window.Delaunator));
  const [libErr, setLibErr] = React.useState(null);
  const [sel, _setSel] = React.useState(null); const selRef = React.useRef(null);
  const [tool2d, _setTool2d] = React.useState('transform'); const tool2dRef = React.useRef('transform');
  const [tool3d, _setTool3d] = React.useState('select'); const tool3dRef = React.useRef('select');
  const [mode, _setMode] = React.useState('sim');
  const [simOn, _setSimOn] = React.useState(false); const simOnRef = React.useRef(false);
  const [disp3, _setDisp3] = React.useState({ avatar: true, textures: true, wire: false, xray: false, seams: true, arrPts: false, pins: true, buttons: true, fit: null, shadows: true, tapes: false });
  const [disp2, _setDisp2] = React.useState({ grid: true, sa: true, sewing: true, lengths: false, dims: true, texture: false, names: true, grain: true, comments: true, grading: false, resolved: false });
  const disp2Ref = React.useRef(disp2);
  const [libOpen, setLibOpen] = React.useState(true);
  const [layout, setLayout] = React.useState('split');
  const [split, setSplit] = React.useState(0.5);
  const [obTab, setObTab] = React.useState('fabric');
  const [dialog, setDialog] = React.useState(null);
  const [ctx, setCtx] = React.useState(null);
  const [toasts, setToasts] = React.useState([]);
  const [snap, setSnap] = React.useState(0); const snapRef = React.useRef(0);
  const [pendingGraphic, _setPendingGraphic] = React.useState(null); const pendingGraphicRef = React.useRef(null);
  const [busy, setBusy] = React.useState(null);
  const [cwThumbs, setCwThumbs] = React.useState({});
  const [tpImages, setTpImages] = React.useState({});
  const [composer, setComposer] = React.useState(null);
  const [saving, setSaving] = React.useState(false);
  const [fitSummary, setFitSummary] = React.useState(null);
  const [size, setSize] = React.useState({ w: 1200, h: 800 });
  const [rightOpen, setRightOpen] = React.useState(true);
  const [sceneReady, setSceneReady] = React.useState(false);
  const [mhState, setMhState] = React.useState(AT.MH.model ? 'ready' : 'idle');
  const [avInfo, setAvInfo] = React.useState(null);
  const [avSizes, setAvSizes] = React.useState({});

  const rootRef = React.useRef(null), v3Ref = React.useRef(null), v2Ref = React.useRef(null), fileRef = React.useRef(null), fileMode = React.useRef(null);
  const sceneRef = React.useRef(null), editorRef = React.useRef(null), simRef = React.useRef(null), avRef = React.useRef(null), avMeshRef = React.useRef(null), avKeyRef = React.useRef(null);
  const undoRef = React.useRef([]), redoRef = React.useRef([]), lastStableRef = React.useRef(null), lastDocRef = React.useRef(null);
  const simDirtyRef = React.useRef(false), lastBuildRef = React.useRef(0), draggingRef = React.useRef(false), arrangeRef = React.useRef(null), autoStopRef = React.useRef(null);
  const clipRef = React.useRef(null), btnPresetRef = React.useRef(null), statsRef = React.useRef({}), tickRef = React.useRef(() => {}), pidRef = React.useRef(pid), unitsRef = React.useRef('cm');
  pidRef.current = pid;
  const d = docRef.current;
  unitsRef.current = d ? d.units : 'cm';

  const toast = React.useCallback((text) => { const id = Math.random(); setToasts((t) => t.concat([{ id: id, text: text }]).slice(-3)); setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), 2600); }, []);
  const setSel = (s) => { selRef.current = s; _setSel(s); };
  const selPieces = (s) => { s = s || selRef.current; return s && s.kind === 'pieces' ? s.ids : s && (s.kind === 'points' || s.kind === 'segment' || s.kind === 'internal') ? [s.piece] : []; };
  const setTool2d = (t) => { tool2dRef.current = t; _setTool2d(t); if (editorRef.current) editorRef.current.cancel(); };
  const setTool3d = (t) => { tool3dRef.current = t; _setTool3d(t); };
  const setSimOn = (v) => { simOnRef.current = v; _setSimOn(v); if (!v) autoStopRef.current = null; };
  const setDisp3 = (patch) => { _setDisp3((p) => { const n = Object.assign({}, p, patch); if (sceneRef.current) sceneRef.current.setDisplay(n); return n; }); };
  const setDisp2 = (patch) => { _setDisp2((p) => { const n = Object.assign({}, p, patch); disp2Ref.current = n; return n; }); };
  const setPendingGraphic = (g) => { pendingGraphicRef.current = g; _setPendingGraphic(g); };
  const modeRef = React.useRef('sim');
  const setMode = (m) => {
    if (modeRef.current === 'review' && m !== 'review') { setTool2d('transform'); setTool3d('select'); }
    modeRef.current = m;
    _setMode(m);
    if (m === 'grading') { setDisp2({ grading: true }); }
    if (m === 'sim') { setDisp2({ grading: false }); }
    if (m === 'review') { setTool2d('comment'); setTool3d('comment'); setDisp2({ comments: true }); }
    if (yj && yj.setPresence) try { yj.setPresence({ name: userName, color: myColor, mode: m }); } catch (e) {}
  };

  // ---------- libraries ----------
  React.useEffect(() => {
    if (libsReady) return;
    let dead = false;
    Promise.all([AT.loadScript(AT.LIBS.three[0], 'THREE'), AT.loadScript(AT.LIBS.delaunator[0], 'Delaunator')]).then(() => { if (!dead) setLibsReady(true); }).catch((e) => { if (!dead) setLibErr(e.message); });
    return () => { dead = true; };
  }, []);
  // ---------- size ----------
  React.useEffect(() => {
    const el = rootRef.current; if (!el) return;
    const ro = new ResizeObserver(() => { const w = el.clientWidth, h = el.clientHeight; setSize({ w: w, h: h }); });
    ro.observe(el); return () => ro.disconnect();
  }, []);
  const narrow = size.w < 1020, tiny = size.w < 760;
  React.useEffect(() => { if (narrow) setLibOpen(false); if (tiny) setLayout('3d'); }, [narrow, tiny]);
  // ---------- presence ----------
  React.useEffect(() => { if (yj && yj.setPresence) try { yj.setPresence({ name: userName, color: myColor, mode: 'sim' }); } catch (e) {} }, [yj && yj.status]);
  const peers = React.useMemo(() => (yj && yj.peers ? (Array.isArray(yj.peers) ? yj.peers : Object.values(yj.peers)) : []).map((p) => (p && (p.presence || p.state || p.user)) || p).filter((p) => p && p.name), [yj && yj.peers]);
  // ---------- URL sync ----------
  React.useEffect(() => { const h = () => setPid(readPid()); window.addEventListener('popstate', h); return () => window.removeEventListener('popstate', h); }, []);
  const goProject = (id) => {
    const u = new URL(window.location.href);
    if (id) u.searchParams.set('p', id); else u.searchParams.delete('p');
    window.history.pushState({}, '', u.toString());
    setPid(id);
  };
  // ---------- Firebase: index + project ----------
  React.useEffect(() => { if (!base) return; const r = FS.ref(FS.db, base + '/index'); const un = FS.onValue(r, (s) => setIndex(s.val() || {}), () => {}); return () => un(); }, [base]);
  React.useEffect(() => {
    if (!base || !pid) { docRef.current = null; bump(); return; }
    setMissing(false);
    docRef.current = null; lastStableRef.current = null; lastDocRef.current = null; undoRef.current = []; redoRef.current = [];
    if (simRef.current) AT.GPU.release(simRef.current);
    simRef.current = null; if (sceneRef.current) sceneRef.current.setSim(null, null); setSel(null); bump();
    const r = FS.ref(FS.db, base + '/projects/' + pid);
    let first = true;
    const un = FS.onValue(r, (snap) => {
      const v = snap.val();
      if (!v) { if (first) { setMissing(true); docRef.current = null; goProject(null); } first = false; return; }
      const nd = AT.normalize(v), st = AT.stable(nd);
      if (st === lastStableRef.current) return;
      if (draggingRef.current) return;
      docRef.current = nd; lastStableRef.current = st; lastDocRef.current = JSON.parse(JSON.stringify(nd));
      simDirtyRef.current = true;
      if (first) { first = false; autoStopRef.current = { n: 0 }; setSimOn(true); setTimeout(() => editorRef.current && editorRef.current.fit(), 60); }
      bump();
    }, (e) => toast('Could not load project: ' + e.message));
    return () => un();
  }, [base, pid]);
  // ---------- persistence ----------
  const writeDiff = (prev, next) => {
    if (!pidRef.current) return;
    const up = {};
    Object.keys(next).forEach((k) => {
      if (COLL.indexOf(k) >= 0) {
        const a = (prev && prev[k]) || {}, b = next[k] || {};
        Object.keys(b).forEach((id) => { if (AT.stable(a[id]) !== AT.stable(b[id])) up[k + '/' + id] = AT.clean(b[id]); });
        Object.keys(a).forEach((id) => { if (!(id in b)) up[k + '/' + id] = null; });
      } else if (AT.stable(prev ? prev[k] : null) !== AT.stable(next[k])) up[k] = AT.clean(next[k]);
    });
    if (prev) Object.keys(prev).forEach((k) => { if (!(k in next)) up[k] = null; });
    if (!Object.keys(up).length) return;
    setSaving(true);
    FS.update(FS.ref(FS.db, base + '/projects/' + pidRef.current), up).then(() => setSaving(false)).catch((e) => { setSaving(false); toast('Save failed: ' + e.message); });
    FS.update(FS.ref(FS.db, base + '/index/' + pidRef.current), { name: next.meta.name, updatedAt: next.meta.updatedAt || Date.now(), pieces: Object.keys(next.pieces || {}).length }).catch(() => {});
  };
  const commit = (label, opts) => {
    const doc = docRef.current; if (!doc) return;
    opts = opts || {};
    AT.syncMirrors(doc);
    if (AT.stable(doc) === lastStableRef.current) { bump(); if (editorRef.current) editorRef.current.invalidate(); return; }
    doc.meta.updatedAt = Date.now();
    const next = AT.stable(doc);
    undoRef.current.push({ label: typeof label === 'string' ? label : 'Edit', s: lastStableRef.current });
    if (undoRef.current.length > 150) undoRef.current.shift();
    redoRef.current = [];
    writeDiff(lastDocRef.current, doc);
    lastStableRef.current = next; lastDocRef.current = JSON.parse(JSON.stringify(doc));
    draggingRef.current = false;
    if (opts.look && sceneRef.current) sceneRef.current.refreshMaterials(doc); else simDirtyRef.current = true;
    bump(); if (editorRef.current) editorRef.current.invalidate();
  };
  const restoreFrom = (s, label, stackFrom, stackTo) => {
    const nd = AT.normalize(JSON.parse(s));
    stackTo.push({ label: label, s: lastStableRef.current });
    writeDiff(lastDocRef.current, nd);
    docRef.current = nd; lastStableRef.current = AT.stable(nd); lastDocRef.current = JSON.parse(JSON.stringify(nd));
    simDirtyRef.current = true; bump(); if (editorRef.current) editorRef.current.invalidate();
  };
  const undo = () => { const u = undoRef.current.pop(); if (!u) return; restoreFrom(u.s, u.label, undoRef.current, redoRef.current); toast('Undo · ' + u.label); };
  const redo = () => { const r = redoRef.current.pop(); if (!r) return; restoreFrom(r.s, r.label, redoRef.current, undoRef.current); toast('Redo · ' + r.label); };
  const live = (ids, rigid) => { draggingRef.current = true; simDirtyRef.current = true; };

  // ---------- simulation tick ----------
  tickRef.current = (dt) => {
    const scene = sceneRef.current, doc = docRef.current, av = avRef.current;
    if (!scene || !doc || !av) return;
    const now = performance.now();
    if (simDirtyRef.current && now - lastBuildRef.current > (draggingRef.current ? 160 : 40)) {
      simDirtyRef.current = false; lastBuildRef.current = now;
      try {
        const params = AT.simParams(AT.simSettings(doc));
        const sim = AT.buildSim(doc, av, simRef.current, { params: params, rearrangeIds: arrangeRef.current });
        if (simRef.current && simRef.current.gScale != null && !arrangeRef.current) sim.gScale = simRef.current.gScale;
        arrangeRef.current = null;
        if (simRef.current && simRef.current !== sim) AT.GPU.release(simRef.current);
        simRef.current = sim; scene.setSim(sim, doc); scene.setSelection(selPieces());
      } catch (e) { console.error(e); toast('Simulation build failed: ' + e.message); }
    }
    const sim = simRef.current;
    if (simOnRef.current && sim) {
      const sv = AT.simSettings(doc).v;
      AT.simStep(sim, sv.dt);
      const as = autoStopRef.current;
      if (as) {
        if (as.t0 == null) { as.t0 = sim.time; as.tr = AT.settleTracker(); }
        const t = sim.time - as.t0;
        if (as.tr.push(t, sim.energy, sim.sewGap || 0, sv.settle)) { setSimOn(false); toast('Draped · settled after ' + t.toFixed(1) + ' s simulated'); }
        else if (t > sv.maxTime) { setSimOn(false); toast('Stopped after ' + Math.round(t) + ' s simulated (max duration)'); }
      }
    }
    if (now - (statsRef.current.t || 0) > 450) {
      statsRef.current = Object.assign({}, statsRef.current, { t: now, fps: scene.fps, particles: sim ? sim.N : 0, energy: sim ? sim.energy : 0, gap: sim ? sim.sewGap || 0 : 0, solver: AT.GPU.label(), gpuMs: sim && sim._g ? sim._g.ms : 0 });
      window.__atelierSim = { solver: AT.GPU.label(), state: AT.GPU.state, info: AT.GPU.info, error: AT.GPU.error || null, backend: sim ? sim.backend || null : null, particles: sim ? sim.N : 0, frames: sim ? sim.frames : 0, gpuMsPerFrame: sim && sim._g ? +sim._g.ms.toFixed(2) : null, crossings: sim && sim._ut && sim._ut.stats ? sim._ut.stats.crossings : null, untangling: sim && sim._g ? sim._g.tangled || 0 : 0, sewGap: sim ? sim.sewGap || 0 : 0, energy: sim ? sim.energy || 0 : 0, simulating: !!simOnRef.current, fps: scene.fps };
      if (scene.disp.fit === 'fit' && scene.fitSummary) setFitSummary(scene.fitSummary);
    }
  };
  // ---------- scene + editor creation ----------
  const appRef = React.useRef(null);
  React.useEffect(() => {
    if (!libsReady || !d || sceneRef.current || !v3Ref.current || !v2Ref.current) return;
    const cb = {
      tick: (dt) => tickRef.current(dt), tool: () => tool3dRef.current, simulating: () => simOnRef.current,
      focus: () => { if (rootRef.current && document.activeElement !== rootRef.current && !(document.activeElement && /INPUT|TEXTAREA|SELECT/.test(document.activeElement.tagName))) rootRef.current.focus({ preventScroll: true }); },
      onSelect: (id, shift) => { if (!id) { if (!shift) setSel(null); return; } const cur = selPieces(); setSel({ kind: 'pieces', ids: shift ? Array.from(new Set(cur.concat([id]))) : [id] }); },
      onPin: (hit) => appRef.current.actions.togglePin(hit), onComment: (hit) => setComposer({ p: hit.r.id, x: hit.x2, y: hit.y2 }),
      onArrange: (ap) => appRef.current.actions.arrangeTo(ap),
      onContext: (e, hit) => { if (hit) { setSel({ kind: 'pieces', ids: [hit.r.id] }); appRef.current.showCtx(e, appRef.current.pieceMenu([hit.r.id])); } }
    };
    try {
      const sc = new AT.Scene3D(v3Ref.current, cb); sc.setTheme(dark); sc.setDisplay(disp3); sceneRef.current = sc;
    } catch (e) { setLibErr('3D view failed to start: ' + e.message); return; }
    const ed = new AT.Editor2D(v2Ref.current, {
      doc: () => docRef.current, sel: () => selRef.current, setSel: (s) => setSel(s), tool: () => tool2dRef.current, selPieces: () => selPieces(),
      commit: (l) => commit(l), live: (ids, rigid) => live(ids, rigid), toast: toast, units: () => unitsRef.current, disp2d: () => disp2Ref.current,
      palette: () => PALETTE[darkRef.current ? 'dark' : 'light'],
      status: (c) => { statsRef.current.cursor = c; }, onContext2D: (e, H) => appRef.current.ctx2D(e, H), gridSnap: () => snapRef.current,
      focus: () => cb.focus(), pendingGraphic: () => pendingGraphicRef.current, setPendingGraphic: (g) => setPendingGraphic(g), newComment: (p, x, y) => setComposer({ p: p, x: x, y: y })
    });
    editorRef.current = ed;
    setSceneReady(true);
    return undefined;
  }, [libsReady, !!d]);
  React.useEffect(() => { AT.GPU.init(); }, []);
  React.useEffect(() => () => { if (simRef.current) AT.GPU.release(simRef.current); if (sceneRef.current) sceneRef.current.dispose(); if (editorRef.current) editorRef.current.dispose(); sceneRef.current = null; editorRef.current = null; }, []);
  React.useEffect(() => { if (sceneRef.current) sceneRef.current.setTheme(dark); if (editorRef.current) editorRef.current.invalidate(); }, [dark]);
  React.useEffect(() => { if (editorRef.current) editorRef.current.invalidate(); }, [rev, sel, tool2d, disp2, mode, snap, layout, split, libOpen, rightOpen]);
  React.useEffect(() => { if (sceneRef.current) sceneRef.current.setSelection(selPieces()); }, [sel]);
  // MakeHuman (CC0) avatar library
  React.useEffect(() => {
    if (AT.MH.model) { setMhState('ready'); return; }
    if (assets && assets.error) { setMhState('error'); return; }
    if (!assets || !assets.url) return;
    setMhState('loading');
    AT.mhLoad(AT.resolveAsset({ path: AT.MH_URL + '.gz' }), AT.resolveAsset({ path: AT.MH_URL })).then(() => setMhState('ready')).catch((e) => { console.error(e); setMhState('error'); toast('Avatar library unavailable — using the built-in mannequin'); });
  }, [assets && assets.url, assets && assets.error]);
  React.useEffect(() => { const t = setTimeout(() => { if (!AT.MH.model) setMhState((s) => (s === 'idle' ? 'error' : s)); }, 15000); return () => clearTimeout(t); }, []);
  React.useEffect(() => { if (!base) return; const r = FS.ref(FS.db, base + '/avatarSizes'); const un = FS.onValue(r, (s) => setAvSizes(s.val() || {}), () => {}); return () => un(); }, [base]);
  // avatar rebuild
  const avKey = d ? AT.stable(AV_KEYS.map((k) => d.avatar[k])) + '|' + mhState : null;
  React.useEffect(() => {
    if (!avKey || !sceneReady || !sceneRef.current) return;
    if (mhState === 'idle' || mhState === 'loading') { setBusy('Loading avatar…'); return; }
    if (avKeyRef.current === avKey) return;
    setBusy(mhState === 'ready' ? 'Fitting avatar to measurements…' : 'Building avatar…');
    const t = setTimeout(() => {
      try {
        const doc = docRef.current;
        let av, mesh;
        if (mhState === 'ready') { av = AT.buildAvatarMH(AT.MH.model, doc.avatar); mesh = av.mesh; }
        else { av = AT.buildAvatar(doc.avatar); mesh = AT.avatarMesh(av); av.kind = 'parametric'; }
        avRef.current = av; avMeshRef.current = mesh; avKeyRef.current = avKey;
        sceneRef.current.setAvatar(av, mesh, doc.avatar.skin);
        setAvInfo({ kind: av.kind, measured: av.measured || null, fit: av.fit || null, pose: av.pose || null, ms: (av.tFit || 0) + (av.tSDF || 0) });
        simDirtyRef.current = true;
      } catch (e) { console.error(e); toast('Avatar failed: ' + e.message); }
      setBusy(null);
    }, 40);
    return () => clearTimeout(t);
  }, [avKey, sceneReady]);
  React.useEffect(() => { if (sceneRef.current) sceneRef.current.highlightTape(null); }, [sel]);
  React.useEffect(() => { if (sceneRef.current && d) sceneRef.current.setSkin(d.avatar.skin || '#cfc9c1'); }, [d && d.avatar.skin, sceneReady]);
  React.useEffect(() => { if (sceneRef.current && d) { const s = sceneRef.current; s.renderer.shadowMap.enabled = disp3.shadows !== false; s.key.castShadow = disp3.shadows !== false; } }, [disp3.shadows, sceneReady]);

  // ---------- actions ----------
  const doc = () => docRef.current;
  const confirm = (title, text, run, ok) => setDialog({ name: 'confirm', title: title, text: text, run: run, ok: ok });
  const pickFile = (modeName, accept) => { fileMode.current = modeName; const el = fileRef.current; if (!el) return; el.accept = accept; el.click(); };
  const uploadAsset = async (file, folder) => {
    if (!assets || !assets.upload) throw new Error('Storage unavailable');
    const name = folder + '/' + Date.now().toString(36) + '-' + file.name.replace(/[^a-z0-9._-]+/gi, '_');
    await assets.upload(name, file);
    return { path: name, url: AT.resolveAsset({ path: name }) };
  };
  const imageRatio = (url) => new Promise((res) => { const im = new Image(); im.onload = () => res(im.naturalWidth / Math.max(1, im.naturalHeight)); im.onerror = () => res(1); im.src = url; });
  const A = {
    commit: commit, undo: undo, redo: redo,
    liveMaterials: () => { if (sceneRef.current) sceneRef.current.refreshMaterials(doc()); if (editorRef.current) { editorRef.current.tileCache = {}; editorRef.current.invalidate(); } },
    liveSim: () => { simDirtyRef.current = true; },
    createProject: (tid, sex) => {
      let TP = null;
      try { if (AT.MH.model) TP = AT.profileFromAvatar(AT.buildAvatarMH(AT.MH.model, Object.assign({}, AT.AVATAR_PRESETS[sex === 'male' ? 'male' : 'female']), { sdf: false })); } catch (e) { console.error(e); }
      const nd = AT.fromTemplate(tid, sex, null, TP);
      const id = 'g' + Date.now().toString(36) + Math.random().toString(36).slice(2, 5);
      nd.meta.createdBy = userName;
      FS.set(FS.ref(FS.db, base + '/projects/' + id), AT.clean(nd)).then(() => {
        FS.set(FS.ref(FS.db, base + '/index/' + id), { name: nd.meta.name, updatedAt: Date.now(), pieces: Object.keys(nd.pieces).length, template: tid });
        setDialog(null); goProject(id);
      }).catch((e) => toast('Could not create: ' + e.message));
    },
    newProject: () => setDialog({ name: 'new' }),
    openProject: (id) => { setDialog(null); goProject(id); },
    deleteProject: (id) => confirm('Delete garment', 'Delete “' + ((index[id] || {}).name || 'garment') + '” and its version history? This cannot be undone.', () => {
      FS.update(FS.ref(FS.db, base), { ['projects/' + id]: null, ['index/' + id]: null, ['versions/' + id]: null }); if (id === pidRef.current) goProject(null);
    }, 'Delete'),
    newFromTemplate: (tid) => A.createProject(tid, doc() ? doc().avatar.sex : 'female'),
    addTemplate: (tid) => {
      const D0 = doc(); const t = AT.TEMPLATES.find((x) => x.id === tid); if (!t || !t.fn) return;
      const tmp = AT.newProject('tmp', D0.avatar.sex); tmp.avatar = AT.clone(D0.avatar); tmp.fabrics = {}; tmp.fabrics.fab_main = AT.makeFabric('jersey', { id: 'fab_main' }); AT[t.fn](tmp, { TP: avRef.current && avRef.current.kind === 'makehuman' ? AT.profileFromAvatar(avRef.current) : null });
      let all = []; AT.asArray(D0.pieces).forEach((pc) => { all = all.concat(pc.pts); });
      const bbA = all.length ? AT.bbox(all) : { x1: 0, y0: 0 }; let tall = []; AT.asArray(tmp.pieces).forEach((pc) => { tall = tall.concat(pc.pts); }); const bbT = AT.bbox(tall);
      const dx = (all.length ? bbA.x1 + 250 : 0) - bbT.x0, dy = (all.length ? bbA.y0 : 0) - bbT.y0;
      const fmap = {}; Object.keys(tmp.fabrics).forEach((fid) => { const nf = Object.assign({}, tmp.fabrics[fid], { id: AT.uid('f') }); fmap[fid] = nf.id; D0.fabrics[nf.id] = nf; });
      AT.asArray(tmp.pieces).forEach((pc) => { pc.fabric = fmap[pc.fabric] || pc.fabric; pc.order = Object.keys(D0.pieces).length; D0.pieces[pc.id] = pc; AT.transformPiece(Object.assign({}, tmp, { pieces: { [pc.id]: pc } }), pc.id, (x, y) => [x + dx, y + dy]); });
      ['sewing', 'stitches', 'poms', 'buttons', 'holes'].forEach((c) => Object.keys(tmp[c] || {}).forEach((k) => { D0[c][k] = tmp[c][k]; }));
      commit('Add ' + t.name); setTimeout(() => editorRef.current && editorRef.current.fit(), 50); toast(t.name + ' added — press Space to drape');
    },
    rename: (v) => { if (!v || !v.trim()) return; doc().meta.name = v.trim(); commit('Rename'); },
    setUnits: (u) => { doc().units = u; commit('Units'); },
    saveVersion: () => setDialog({ name: 'saveVersion' }),
    writeVersion: (name) => { const vid = 'v' + Date.now().toString(36); FS.set(FS.ref(FS.db, base + '/versions/' + pidRef.current + '/' + vid), { name: name, at: Date.now(), by: userId, byName: userName, doc: AT.clean(doc()) }).then(() => toast('Version saved')).catch((e) => toast(e.message)); },
    restoreVersion: (v) => { const nd = AT.normalize(AT.clone(v.doc)); const cur = docRef.current; docRef.current = nd; nd.meta.name = nd.meta.name || cur.meta.name; undoRef.current.push({ label: 'Restore version', s: lastStableRef.current }); writeDiff(lastDocRef.current, nd); lastStableRef.current = AT.stable(nd); lastDocRef.current = JSON.parse(JSON.stringify(nd)); simDirtyRef.current = true; bump(); toast('Restored “' + v.name + '”'); },
    importDXF: () => pickFile('dxf', '.dxf,.DXF'), importJSON: () => pickFile('json', '.json,application/json'),
    exportDXF: () => AT.download(AT.slug(doc().meta.name) + '-' + doc().activeSize + '.dxf', AT.exportDXF(doc()), 'application/dxf'),
    exportSVG: () => AT.download(AT.slug(doc().meta.name) + '.svg', AT.exportSVG(doc()), 'image/svg+xml'),
    exportJSON: () => AT.download(AT.slug(doc().meta.name) + '.atelier.json', JSON.stringify(AT.clean(doc()), null, 1), 'application/json'),
    exportOBJ: (withAv) => { if (!simRef.current) return toast('Nothing to export yet'); const o = AT.exportOBJ(doc(), simRef.current, avMeshRef.current, withAv); AT.download(AT.slug(doc().meta.name) + '.obj', o.obj.replace('mtllib garment.mtl', 'mtllib ' + AT.slug(doc().meta.name) + '.mtl'), 'text/plain'); setTimeout(() => AT.download(AT.slug(doc().meta.name) + '.mtl', o.mtl, 'text/plain'), 400); },
    exportCSV: () => AT.download(AT.slug(doc().meta.name) + '-techpack.csv', AT.techCSV(doc()), 'text/csv'),
    exportPDF: async () => {
      try {
        toast('Building tech pack…');
        await AT.loadScript(AT.LIBS.jspdf[0], 'jspdf');
        const imgs = Object.keys(tpImages).length ? tpImages : A.captureTechImages();
        const tp = doc().techpack || {};
        const pdf = AT.techPDF(doc(), imgs, { season: tp.season, designer: tp.designer || userName, notes: tp.notes, currency: tp.currency });
        pdf.save(AT.fileName(doc().meta.name) + ' - tech pack.pdf');
      } catch (e) { toast('PDF failed: ' + e.message); }
    },
    exportPatternPDF: async (opts) => {
      try {
        toast('Building pattern PDF…');
        await AT.loadScript(AT.LIBS.jspdf[0], 'jspdf');
        const pdf = AT.patternPDF(doc(), opts);
        const n = pdf.getNumberOfPages();
        pdf.save(AT.fileName(doc().meta.name) + ' - pattern ' + doc().activeSize + '.pdf');
        toast('Pattern PDF exported · ' + n + ' page' + (n > 1 ? 's' : ''));
        return true;
      } catch (e) { console.error(e); toast('PDF failed: ' + e.message); return false; }
    },
    setHem: (pid, sid, w) => {
      const d0 = doc(); let pc = d0.pieces[pid]; if (!pc) return;
      if (pc.mirror && d0.pieces[pc.mirror.src]) { const srcPc = d0.pieces[pc.mirror.src], s2 = AT.srcSegId(srcPc, sid); if (s2) { pc = srcPc; sid = s2; } }
      const h = Object.assign({}, pc.hem || {}); if (w > 0) h[sid] = w; else delete h[sid];
      if (Object.keys(h).length) pc.hem = h; else delete pc.hem;
      AT.syncMirrors(d0); commit(w > 0 ? 'Hem' : 'Remove hem');
    },
    setSeamSA: (swId, v) => { const n = AT.setSeamSA(doc(), swId, v); if (n) commit('Seam allowance'); },
    setSegSA: (pid, sid, v) => {
      const d0 = doc(), pc = d0.pieces[pid]; if (!pc) return;
      const ol = AT.outline(pc.pts), cov = AT.edgeCoverage(d0, pc, ol, null), k = ol.byId[sid]; if (k == null) return;
      const s0 = ol.start[k], s1 = s0 + ol.segs[k].len, T = ol.total, seams = {};
      cov.sides.forEach((x) => { if (Math.min(s1, x.P1) - Math.max(s0, x.P0) > 0.5 || Math.min(s1 + T, x.P1) - Math.max(s0 + T, x.P0) > 0.5) seams[x.sw.id] = 1; });
      const ids = Object.keys(seams);
      if (ids.length) ids.forEach((id) => AT.setSeamSA(d0, id, v));
      else {
        let tp = pc, ts = sid;
        if (pc.mirror && d0.pieces[pc.mirror.src]) { const s2 = AT.srcSegId(d0.pieces[pc.mirror.src], sid); if (s2) { tp = d0.pieces[pc.mirror.src]; ts = s2; } }
        tp.saSeg = Object.assign({}, tp.saSeg || {}, { [ts]: v }); AT.syncMirrors(d0);
      }
      commit(ids.length ? 'Seam allowance (both sides)' : 'Segment seam allowance');
    },
    detectHems: () => {
      const d0 = doc(), r = AT.detectHems(d0); let n = 0;
      Object.keys(r).forEach((id) => { const pc = d0.pieces[id], h = Object.assign({}, pc.hem || {}); Object.keys(r[id]).forEach((k) => { if (h[k] == null) { h[k] = r[id][k]; n++; } }); pc.hem = h; });
      AT.syncMirrors(d0);
      if (n) { commit('Mark hems'); toast(n + ' hem edge' + (n > 1 ? 's' : '') + ' marked — edit widths on each segment'); } else toast('No new hem edges found (unsewn edges at the bottom of a piece)');
    },
    openPatternPDF: (ids) => setDialog({ name: 'patternPdf', ids: ids || selPieces() }),
    captureTechImages: () => { const s = sceneRef.current; if (!s) return {}; const bg = '#f3f2ef'; const imgs = { front: s.snapshotView('front', 1.5, bg), back: s.snapshotView('back', 1.5, bg), side: s.snapshotView('left', 1.5, bg) }; setTpImages(imgs); return imgs; },
    snapshot: () => { const s = sceneRef.current; if (!s) return; const url = s.snapshot(2); fetch(url).then((r) => r.blob()).then((b) => AT.download(AT.slug(doc().meta.name) + '-' + Date.now().toString(36) + '.png', b, 'image/png')); toast('Snapshot saved'); },
    turntable: (sec) => { const s = sceneRef.current; if (!s) return; toast('Recording ' + sec + ' s turntable…'); s.recordTurntable(sec, (blob, err) => { if (!blob) return toast(err || 'Recording failed'); AT.download(AT.slug(doc().meta.name) + '-turntable.' + (blob.type.indexOf('mp4') >= 0 ? 'mp4' : 'webm'), blob); }); },
    renderColorways: () => {
      const s = sceneRef.current, D0 = doc(); if (!s) return;
      const keep = D0.activeColorway, out = {};
      AT.asArray(D0.colorways).forEach((cw) => { D0.activeColorway = cw.id; s.refreshMaterials(D0); out[cw.id] = s.snapshotView('front', 0.7, dark ? '#25262b' : '#f2f2f4'); });
      D0.activeColorway = keep; s.refreshMaterials(D0); setCwThumbs(out); toast('Colorways rendered');
    },
    fit: (m) => { const v = disp3.fit === m ? null : m; setDisp3({ fit: v }); },
    focus3d: () => { const s = sceneRef.current, sim = simRef.current; if (!s) return; const ids = selPieces(); if (!sim || !ids.length) { s.focus(null); return; } const b = [1e9, 1e9, 1e9, -1e9, -1e9, -1e9]; sim.recs.forEach((r) => { if (ids.indexOf(r.id) < 0) return; for (let i = 0; i < r.n; i++) { const k = (r.off + i) * 3; for (let c = 0; c < 3; c++) { b[c] = Math.min(b[c], sim.x[k + c]); b[c + 3] = Math.max(b[c + 3], sim.x[k + c]); } } }); s.focus(b); },
    copy: () => { const ids = selPieces(); if (!ids.length) return; clipRef.current = ids.map((i) => AT.clone(doc().pieces[i])).filter(Boolean); toast('Copied ' + ids.length + ' pattern' + (ids.length > 1 ? 's' : '')); },
    paste: () => { const cl = clipRef.current; if (!cl || !cl.length) return toast('Clipboard is empty'); const D0 = doc(), ids = []; cl.forEach((src) => { const tmpDoc = { pieces: { [src.id]: AT.clone(src) }, buttons: {}, holes: {}, comments: {} }; const pc = AT.duplicatePiece(Object.assign({}, D0, { pieces: Object.assign({}, D0.pieces, tmpDoc.pieces) }), src.id, false); if (pc) { D0.pieces[pc.id] = pc; ids.push(pc.id); } }); setSel({ kind: 'pieces', ids: ids }); arrangeRef.current = ids; commit('Paste'); },
    duplicate: () => { const ids = selPieces(); if (!ids.length) return; const nids = ids.map((i) => { const pc = AT.duplicatePiece(doc(), i, false); return pc && pc.id; }).filter(Boolean); setSel({ kind: 'pieces', ids: nids }); arrangeRef.current = nids; commit('Duplicate'); },
    mirror: (linked) => { const ids = selPieces().filter((i) => !doc().pieces[i].mirror); if (!ids.length) return; const nids = ids.map((i) => { const pc = AT.duplicatePiece(doc(), i, true, linked); return pc && pc.id; }).filter(Boolean); AT.syncMirrors(doc()); setSel({ kind: 'pieces', ids: nids }); arrangeRef.current = nids; commit('Symmetric pattern'); toast('Linked mirror created — edits to the original update it'); },
    selectAll: () => setSel({ kind: 'pieces', ids: Object.keys(doc().pieces) }),
    del: () => {
      const s = selRef.current, D0 = doc(); if (!s) return;
      if (s.kind === 'pieces') { AT.deletePieces(D0, s.ids); setSel(null); commit('Delete pattern'); return; }
      if (s.kind === 'points') { const pc = D0.pieces[s.piece]; if (!pc) return; const old = AT.clone(pc.pts); const left = pc.pts.filter((p) => s.ids.indexOf(p.id) < 0); if (left.filter((p) => !p.c).length < 3) return toast('A pattern needs at least 3 corner points'); pc.pts = left; AT.remapRefs(D0, pc.id, old); setSel(null); commit('Delete point'); return; }
      if (s.kind === 'sewing') { delete D0.sewing[s.id]; setSel(null); commit('Delete sewing'); return; }
      if (s.kind === 'internal') { const pc = D0.pieces[s.piece]; if (pc) pc.internals = pc.internals.filter((q) => q.id !== s.id); setSel(null); commit('Delete internal line'); return; }
      if (s.kind === 'graphic') { const pc = D0.pieces[s.piece]; if (pc) pc.graphics = pc.graphics.filter((q) => q.id !== s.id); setSel(null); commit('Delete graphic'); return; }
      const map = { button: 'buttons', hole: 'holes', stitch: 'stitches', pom: 'poms', comment: 'comments' };
      if (map[s.kind]) { if (s.kind === 'hole') Object.values(D0.buttons).forEach((b) => { if (b.fasten === s.id) delete b.fasten; }); delete D0[map[s.kind]][s.id]; setSel(null); commit('Delete'); return; }
      if (s.kind === 'fabric') A.delFabric(s.id);
    },
    flip: (axis) => {
      const D0 = doc(); const ids = selPieces().filter((i) => D0.pieces[i] && !D0.pieces[i].mirror); if (!ids.length) return;
      ids.forEach((id) => {
        const pc = D0.pieces[id], bb = AT.bbox(pc.pts), mx = (x, y) => axis === 'h' ? [2 * bb.cx - x, y] : [x, 2 * bb.cy - y];
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
      commit(axis === 'h' ? 'Flip horizontally' : 'Flip vertically');
    },
    rotate90: () => { const D0 = doc(); selPieces().forEach((id) => { const pc = D0.pieces[id]; if (!pc || pc.mirror) return; const bb = AT.bbox(pc.pts); AT.transformPiece(D0, id, (x, y) => [bb.cx - (y - bb.cy), bb.cy + (x - bb.cx)]); }); commit('Rotate 90°'); },
    saAll: () => setDialog({ name: 'sa' }),
    setSAAll: (v) => { Object.values(doc().pieces).forEach((pc) => { pc.sa = v; delete pc.saSeg; }); commit('Seam allowance'); },
    resetArrangement: (ids, silent) => { arrangeRef.current = ids || Object.keys(doc().pieces); simDirtyRef.current = true; if (simRef.current) simRef.current.gScale = 0.03; if (!silent) toast('Re-arranged — press Space to sew'); },
    arrangeTo: (ap) => { const ids = selPieces(); if (!ids.length) return toast('Select a pattern first, then click an arrangement point'); ids.forEach((i) => { const pc = doc().pieces[i]; pc.arr = Object.assign({}, pc.arr || {}, { slot: ap.slot, h: ap.h != null ? ap.h : (pc.arr || {}).h, t: ap.t != null ? ap.t : null, ang: null, shift: null }); }); commit('Arrange'); A.resetArrangement(ids, true); },
    togglePin: (hit) => { const pc = doc().pieces[hit.r.id]; if (!pc) return; const pins = pc.pins || []; const near = pins.findIndex((p) => Math.hypot(p.x - hit.x2, p.y - hit.y2) < 18); if (near >= 0) pins.splice(near, 1); else pins.push({ x: Math.round(hit.x2 * 10) / 10, y: Math.round(hit.y2 * 10) / 10 }); pc.pins = pins; commit(near >= 0 ? 'Unpin' : 'Pin'); },
    toggleState: (st) => { const D0 = doc(), ids = selPieces(); if (!ids.length) return; const on = ids.every((i) => D0.pieces[i].state === st); ids.forEach((i) => { D0.pieces[i].state = on ? 'active' : st; }); commit(st === 'frozen' ? (on ? 'Unfreeze' : 'Freeze') : (on ? 'Activate' : 'Deactivate')); },
    toggleStrengthen: () => { const D0 = doc(), ids = selPieces(); if (!ids.length) return; const on = ids.every((i) => D0.pieces[i].strengthen); ids.forEach((i) => { D0.pieces[i].strengthen = !on; }); commit(on ? 'Unstrengthen' : 'Strengthen'); },
    hide3d: () => { selPieces().forEach((i) => { doc().pieces[i].hide3d = true; }); commit('Hide in 3D'); },
    showAll3d: () => { Object.values(doc().pieces).forEach((pc) => { delete pc.hide3d; }); commit('Show all 3D'); },
    layer: (dl) => { selPieces().forEach((i) => { const pc = doc().pieces[i]; pc.layer = AT.clamp((pc.layer || 0) + dl, -2, 6); }); commit('Layer'); },
    setSimPreset: (key) => { if (!AT.SIM_PRESETS[key]) return; doc().sim = { preset: key }; commit('Simulation: ' + AT.SIM_PRESETS[key].name); toast(AT.SIM_PRESETS[key].name + ' simulation' + (AT.SIM_PRESETS[key].v.pdScale && AT.SIM_PRESETS[key].v.pdScale !== 1 ? ' · re-meshing' : '')); },
    setQuality: (q) => { const m = { fast: 'draft', complete: 'fine' }; doc().sim = { preset: m[q] || q }; commit('Simulation quality'); },
    setSimParam: (k, val) => {
      const st = AT.simSettings(doc()), over = Object.assign({}, st.over, { [k]: val });
      if (val === st.base[k]) delete over[k];
      doc().sim = Object.keys(over).length ? { preset: st.preset, over: over } : { preset: st.preset };
      commit('Simulation setting');
    },
    resetSimParams: () => { const st = AT.simSettings(doc()); doc().sim = { preset: st.preset }; commit('Reset simulation settings'); },
    toggleSelfCollision: () => { const st = AT.simSettings(doc()), over = Object.assign({}, st.over, { selfCollision: !st.v.selfCollision }); if (over.selfCollision === st.base.selfCollision) delete over.selfCollision; doc().sim = Object.keys(over).length ? { preset: st.preset, over: over } : { preset: st.preset }; commit('Self-collision'); },
    checkSewing: () => { const L = AT.sewLengths(doc()); const bad = Object.keys(L).filter((k) => Math.abs(L[k].a - L[k].b) > 3); if (!bad.length) return toast('All ' + Object.keys(L).length + ' sewing lines match within 3 mm'); setSel({ kind: 'sewing', id: bad[0] }); setObTab('sewing'); toast(bad.length + ' sewing line' + (bad.length > 1 ? 's differ' : ' differs') + ' by more than 3 mm'); },
    flipSewing: () => { const s = selRef.current; if (!s || s.kind !== 'sewing') return; const sw = doc().sewing[s.id]; const r = sw.b.slice().reverse().map((sd) => ({ p: sd.p, a: sd.b, b: sd.a, dir: -sd.dir })); sw.b = r; commit('Flip sewing'); },
    deleteAllSewing: () => confirm('Delete all sewing', 'Remove all ' + Object.keys(doc().sewing).length + ' sewing lines from this garment?', () => { doc().sewing = {}; setSel(null); commit('Delete all sewing'); }, 'Delete'),
    addFabric: (preset) => { const f = AT.makeFabric(preset); const D0 = doc(); D0.fabrics[f.id] = f; const ids = selPieces(); ids.forEach((i) => { D0.pieces[i].fabric = f.id; }); setObTab('fabric'); setSel({ kind: 'fabric', id: f.id }); commit('Add fabric'); if (ids.length) toast(f.name + ' applied to ' + ids.length + ' pattern' + (ids.length > 1 ? 's' : '')); },
    dupFabric: (fid) => { const src = doc().fabrics[fid]; if (!src) return; const f = Object.assign(AT.clone(src), { id: AT.uid('f'), name: src.name + ' copy' }); doc().fabrics[f.id] = f; setSel({ kind: 'fabric', id: f.id }); commit('Duplicate fabric'); },
    delFabric: (fid) => { const D0 = doc(); if (Object.values(D0.pieces).some((pc) => pc.fabric === fid)) return toast('Fabric is in use'); if (Object.keys(D0.fabrics).length <= 1) return toast('Keep at least one fabric'); delete D0.fabrics[fid]; setSel(null); commit('Delete fabric'); },
    applyFabric: (fid) => { const ids = selPieces(); if (!ids.length) return toast('Select patterns first'); ids.forEach((i) => { doc().pieces[i].fabric = fid; }); commit('Assign fabric'); },
    setAvatarSex: (sx) => { const D0 = doc(); const keep = { skin: D0.avatar.skin, show: D0.avatar.show, armAngle: D0.avatar.armAngle, legSpread: D0.avatar.legSpread }; D0.avatar = Object.assign({}, AT.AVATAR_PRESETS[sx], keep); commit('Avatar ' + sx); },
    applySizeChart: (c) => { const D0 = doc(); Object.keys(c).forEach((k) => { if (k !== 'name' && AV_KEYS.indexOf(k) >= 0) D0.avatar[k] = c[k]; }); commit('Avatar size ' + c.name); toast('Avatar set to ' + c.name); },
    setAvatarField: (k, v, label) => { const D0 = doc(); if (v == null) delete D0.avatar[k]; else D0.avatar[k] = v; commit(label || ('Avatar ' + k)); },
    focusTape: (k) => { if (sceneRef.current) { if (!disp3.tapes && k) setDisp3({ tapes: true }); sceneRef.current.highlightTape(k); } },
    saveAvatarSize: (name) => { const D0 = doc(), id = 'as' + Date.now().toString(36); const spec = {}; AV_KEYS.forEach((k) => { if (D0.avatar[k] != null) spec[k] = D0.avatar[k]; }); FS.set(FS.ref(FS.db, base + '/avatarSizes/' + id), { name: name || ('Size ' + new Date().toLocaleDateString()), at: Date.now(), by: userName, spec: spec }).then(() => toast('Avatar size saved')); },
    applyAvatarSize: (sz) => { const D0 = doc(); const keep = { skin: D0.avatar.skin, show: D0.avatar.show }; D0.avatar = Object.assign({}, sz.spec, keep); commit('Avatar size ' + sz.name); },
    deleteAvatarSize: (id) => FS.remove(FS.ref(FS.db, base + '/avatarSizes/' + id)),
    exportAvatar: () => { const D0 = doc(); const spec = {}; AV_KEYS.forEach((k) => { if (D0.avatar[k] != null) spec[k] = D0.avatar[k]; }); AT.download(AT.slug(D0.meta.name) + '-avatar.json', JSON.stringify({ format: 'atelier-avatar-size', version: 1, units: 'mm', body: 'MakeHuman (CC0 1.0)', spec: spec }, null, 1), 'application/json'); },
    importAvatar: () => pickFile('avatar', '.json,application/json'),
    setActiveSize: (sz) => { doc().activeSize = sz; commit('Size ' + sz); },
    setBaseSize: (sz) => { const D0 = doc(); if (sz === D0.baseSize) return;
      // re-base: bake target size into base points, re-express all rules relative to it
      Object.values(D0.pieces).forEach((pc) => { if (!pc.grade) return; const g = pc.grade; pc.pts.forEach((p) => { const r = g[p.id] || {}; const off = r[sz] || { dx: 0, dy: 0 }; p.x += off.dx || 0; p.y += off.dy || 0; const nr = {}; D0.sizes.forEach((s) => { if (s === sz) return; const o = s === D0.baseSize ? { dx: 0, dy: 0 } : (r[s] || { dx: 0, dy: 0 }); nr[s] = { dx: (o.dx || 0) - (off.dx || 0), dy: (o.dy || 0) - (off.dy || 0) }; }); g[p.id] = nr; }); });
      D0.baseSize = sz; D0.activeSize = sz; commit('Base size ' + sz); },
    addSize: (name) => { const nm = String(name).replace(/[.#$\[\]\/]/g, '-').slice(0, 12); const D0 = doc(); if (D0.sizes.indexOf(nm) >= 0) return toast('Size exists'); D0.sizes = D0.sizes.concat([nm]); commit('Add size'); },
    removeSize: (sz) => { const D0 = doc(); if (sz === D0.baseSize) return; D0.sizes = D0.sizes.filter((s) => s !== sz); Object.values(D0.pieces).forEach((pc) => { if (pc.grade) Object.values(pc.grade).forEach((r) => { delete r[sz]; }); }); if (D0.activeSize === sz) D0.activeSize = D0.baseSize; commit('Remove size'); },
    autoGrade: (ids, w, l) => { const D0 = doc(); const bi = D0.sizes.indexOf(D0.baseSize); (ids || Object.keys(D0.pieces)).forEach((id) => { const pc = D0.pieces[id]; if (!pc || pc.mirror) return; const bb = AT.bbox(pc.pts); pc.grade = {}; pc.pts.forEach((p) => { const r = {}; D0.sizes.forEach((s, i) => { const k = i - bi; if (!k) return; r[s] = { dx: Math.round((p.x - bb.cx) * w / 100 * k * 10) / 10, dy: Math.round((p.y - bb.y0) * l / 100 * k * 10) / 10 }; }); pc.grade[p.id] = r; }); }); commit('Proportional grading'); setDisp2({ grading: true }); toast('Graded ' + D0.sizes.length + ' sizes'); },
    clearGrade: (ids) => { const D0 = doc(); (ids || Object.keys(D0.pieces)).forEach((id) => { if (D0.pieces[id]) delete D0.pieces[id].grade; }); commit('Clear grading'); },
    addColorway: () => { const D0 = doc(); const cur = D0.colorways[D0.activeColorway]; const id = AT.uid('cw'); const n = Object.keys(D0.colorways).length; D0.colorways[id] = { id: id, name: 'Colorway ' + (n + 1), order: n, over: AT.clone((cur && cur.over) || {}) }; D0.activeColorway = id; commit('New colorway'); },
    setColorway: (id) => { doc().activeColorway = id; commit('Switch colorway', { look: true }); },
    deleteColorway: (id) => { const D0 = doc(); delete D0.colorways[id]; if (D0.activeColorway === id) D0.activeColorway = Object.keys(D0.colorways)[0]; commit('Delete colorway'); },
    setColorwayColor: (cwid, fid, color, isBase) => { const D0 = doc(); if (isBase) D0.fabrics[fid].color = color; else { const cw = D0.colorways[cwid]; cw.over = cw.over || {}; cw.over[fid] = Object.assign({}, cw.over[fid] || {}, { color: color }); } commit('Colorway colour', { look: true }); },
    patchPiece: (id, patch, label) => { const pc = doc().pieces[id]; if (!pc) return; Object.keys(patch).forEach((k) => { if (patch[k] == null) delete pc[k]; else pc[k] = patch[k]; }); commit(label || 'Edit pattern'); },
    patchPieces: (ids, patch, label) => { ids.forEach((id) => { const pc = doc().pieces[id]; if (!pc) return; Object.keys(patch).forEach((k) => { if (patch[k] == null) delete pc[k]; else pc[k] = patch[k]; }); }); commit(label || 'Edit pattern'); },
    addPOM: () => { const s = selRef.current; if (!s || s.kind !== 'segment') return; const pc = doc().pieces[s.piece]; const id = AT.uid('pm'); doc().poms[id] = { id: id, name: (pc ? pc.name : '') + ' measure', refs: [{ p: s.piece, s: s.seg }], tol: 6 }; setSel({ kind: 'pom', id: id }); commit('Add POM'); },
    uploadGraphic: async (file) => { try { toast('Uploading…'); const r = await uploadAsset(file, 'graphics'); const ratio = await imageRatio(r.url); setPendingGraphic({ path: r.path, url: r.url, name: file.name, ratio: ratio }); setTool2d('graphic'); toast('Click on a pattern to place the graphic'); } catch (e) { toast('Upload failed: ' + e.message); } },
    uploadTexture: async (fid, file) => { try { toast('Uploading texture…'); const r = await uploadAsset(file, 'textures'); const f = doc().fabrics[fid]; f.texture = { path: r.path, w: 100 }; commit('Fabric texture'); } catch (e) { toast('Upload failed: ' + e.message); } },
    createComment: (c, text) => { const D0 = doc(); const id = AT.uid('c'); const n = Object.values(D0.comments).reduce((m, x) => Math.max(m, x.n || 0), 0) + 1; D0.comments[id] = { id: id, n: n, p: c.p, x: c.x, y: c.y, text: text, by: userId, byName: userName, at: Date.now(), resolved: false }; setSel({ kind: 'comment', id: id }); commit('Comment'); },
    replyComment: (id, text) => { const c = doc().comments[id]; if (!c) return; c.replies = AT.asArray(c.replies).concat([{ text: text, by: userId, byName: userName, at: Date.now() }]); commit('Reply'); },
    focusComment: (c) => { const ed = editorRef.current; if (!ed) return; ed.view.ox = ed.w / 2 - c.x * ed.view.s; ed.view.oy = ed.h / 2 - c.y * ed.view.s; ed.invalidate(); }
  };
  const pieceMenu = (ids) => {
    const D0 = doc(), pcs = ids.map((i) => D0.pieces[i]).filter(Boolean); if (!pcs.length) return [];
    const frozen = pcs.every((p) => p.state === 'frozen'), off = pcs.every((p) => p.state === 'off');
    return [{ label: 'Copy', k: '⌘C', run: A.copy }, { label: 'Paste', k: '⌘V', run: A.paste }, { label: 'Symmetric pattern (linked)', k: '⌘R', run: () => A.mirror(true) }, { label: 'Duplicate', k: '⌘D', run: A.duplicate }, { label: 'Delete', k: '⌫', run: A.del }, { sep: 1 },
      { label: 'Flip horizontally', run: () => A.flip('h') }, { label: 'Flip vertically', run: () => A.flip('v') }, { label: 'Rotate 90°', run: A.rotate90 }, { sep: 1 },
      { label: frozen ? 'Unfreeze' : 'Freeze', k: '⌘K', run: () => A.toggleState('frozen') }, { label: off ? 'Activate' : 'Deactivate', k: '⌘J', run: () => A.toggleState('off') }, { label: pcs.every((p) => p.strengthen) ? 'Unstrengthen' : 'Strengthen', k: '⌘H', run: A.toggleStrengthen }, { label: 'Hide in 3D', k: '⇧Q', run: A.hide3d }, { sep: 1 },
      { label: 'Reset 3D arrangement', k: '⌘F', run: () => A.resetArrangement(ids) }, { label: 'Layer +1', run: () => A.layer(1) }, { label: 'Layer −1', run: () => A.layer(-1) }];
  };
  const showCtx = (e, items) => { const r = rootRef.current.getBoundingClientRect(); setCtx({ x: e.clientX - r.left, y: e.clientY - r.top, items: items }); };
  const ctx2D = (e, H) => {
    const D0 = doc();
    if (H.point) { const pc = H.point.pc, p = pc.pts.find((q) => q.id === H.point.p.id); setSel({ kind: 'points', piece: pc.id, ids: [H.point.p.id] }); return showCtx(e, [{ label: p && p.c ? 'Convert to corner' : 'Convert to curve point', run: () => { const old = AT.clone(pc.pts); if (p.c) delete p.c; else p.c = 1; AT.remapRefs(D0, pc.id, old); commit('Convert point'); } }, { label: 'Delete point', k: '⌫', run: A.del }]); }
    if (H.sewing) { setSel({ kind: 'sewing', id: H.sewing.id }); return showCtx(e, [{ label: 'Flip side B direction', run: A.flipSewing }, { label: 'Delete sewing', k: '⌫', run: A.del }]); }
    if (H.seg) { const pc = H.seg.pc; setSel({ kind: 'segment', piece: pc.id, seg: H.seg.id }); return showCtx(e, [{ head: 'Segment' }, { label: 'Split here', run: () => { const old = AT.clone(pc.pts); AT.splitSegment(pc, H.seg.id, H.seg.t); AT.remapRefs(D0, pc.id, old); commit('Split'); } }, { label: 'Add notch here', run: () => { pc.notches = (pc.notches || []).concat([{ id: AT.uid('n'), s: H.seg.id, t: H.seg.t }]); commit('Add notch'); } }, { label: 'Add topstitch', run: () => { const id = AT.uid('st'); D0.stitches[id] = { id: id, p: pc.id, seg: H.seg.id, off: 4, len: 3, gap: 1.6, w: 0.9, color: '#3b3a36', rows: 1, rowGap: 6 }; commit('Add topstitch'); } }, { label: 'Add point of measure', run: () => { const id = AT.uid('pm'); D0.poms[id] = { id: id, name: pc.name + ' measure', refs: [{ p: pc.id, s: H.seg.id }], tol: 6 }; commit('Add POM'); } }, { sep: 1 }, { head: 'Pattern' }].concat(pieceMenu([pc.id]))); }
    if (H.piece) { if (selPieces().indexOf(H.piece.id) < 0) setSel({ kind: 'pieces', ids: [H.piece.id] }); return showCtx(e, pieceMenu(selPieces().indexOf(H.piece.id) >= 0 ? selPieces() : [H.piece.id])); }
    showCtx(e, [{ label: 'Paste', k: '⌘V', run: A.paste }, { label: 'Select all', k: '⌘A', run: A.selectAll }, { label: 'Zoom to fit', run: () => editorRef.current && editorRef.current.fit() }, { sep: 1 }, { label: 'Rectangle', k: 'S', run: () => setTool2d('rect') }, { label: 'Polygon', k: 'H', run: () => setTool2d('polygon') }]);
  };
  // ---------- file input ----------
  const onFile = (e) => {
    const f = e.target.files && e.target.files[0]; e.target.value = ''; if (!f) return;
    const rd = new FileReader();
    rd.onload = () => {
      try {
        if (fileMode.current === 'dxf') {
          const pcs = AT.importDXF(String(rd.result)); if (!pcs.length) return toast('No closed patterns found in DXF');
          const D0 = doc(); const fid = Object.keys(D0.fabrics)[0]; const ids = [];
          let all = []; AT.asArray(D0.pieces).forEach((pc) => { all = all.concat(pc.pts); });
          let ox = all.length ? AT.bbox(all).x1 + 200 : 0;
          pcs.forEach((p, i) => { const bb = AT.bbox(p.pts); const pc = AT.newPiece(p.name || ('DXF ' + (i + 1)), p.pts.map((q) => ({ id: AT.uid('p'), x: q.x - bb.x0 + ox, y: q.y - bb.y0 })), { fabric: fid, internals: p.internals.map((il) => Object.assign(il, { pts: il.pts.map((q) => ({ x: q.x - bb.x0 + ox, y: q.y - bb.y0 })) })), arr: { slot: i % 2 ? 'back' : 'front', off: 70, h: D0.avatar.height * 0.6 } }); ox += bb.w + 60; AT.addPiece(D0, pc); ids.push(pc.id); });
          setSel({ kind: 'pieces', ids: ids }); commit('Import DXF'); setTimeout(() => editorRef.current && editorRef.current.fit(), 60); toast('Imported ' + pcs.length + ' pattern' + (pcs.length > 1 ? 's' : '') + ' — sew and arrange them');
        } else if (fileMode.current === 'avatar') {
          const j = JSON.parse(String(rd.result)); const spec = j.spec || j; const D0 = doc(); const keep = { skin: D0.avatar.skin, show: D0.avatar.show };
          const clean = {}; AV_KEYS.forEach((k) => { if (spec[k] != null && (k === 'sex' || isFinite(spec[k]))) clean[k] = k === 'sex' ? (spec[k] === 'male' ? 'male' : 'female') : +spec[k]; });
          D0.avatar = Object.assign({ sex: 'female' }, clean, keep); commit('Import avatar size'); toast('Avatar size imported');
        } else if (fileMode.current === 'json') {
          const nd = AT.normalize(JSON.parse(String(rd.result)));
          const id = 'g' + Date.now().toString(36);
          FS.set(FS.ref(FS.db, base + '/projects/' + id), AT.clean(nd)).then(() => { FS.set(FS.ref(FS.db, base + '/index/' + id), { name: nd.meta.name, updatedAt: Date.now(), pieces: Object.keys(nd.pieces).length }); goProject(id); });
        }
      } catch (err) { toast('Import failed: ' + err.message); }
    };
    rd.readAsText(f);
  };
  // ---------- keyboard ----------
  const onKey = (e) => {
    const t = e.target; if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.tagName === 'SELECT' || t.isContentEditable)) return;
    if (!docRef.current) return;
    const k = e.key, mod = e.metaKey || e.ctrlKey, ed = editorRef.current;
    const stop = () => { e.preventDefault(); e.stopPropagation(); };
    if (mod && (k === 'z' || k === 'Z')) { stop(); if (e.shiftKey) redo(); else undo(); return; }
    if (mod && (k === 'y' || k === 'Y')) { stop(); redo(); return; }
    if (mod && k === 'c') { stop(); A.copy(); return; } if (mod && k === 'v') { stop(); A.paste(); return; } if (mod && k === 'd') { stop(); A.duplicate(); return; }
    if (mod && k === 'r') { stop(); A.mirror(true); return; } if (mod && k === 'a') { stop(); A.selectAll(); return; } if (mod && k === 's') { stop(); A.saveVersion(); return; } if (mod && k === 'o') { stop(); setDialog({ name: 'open' }); return; }
    if (mod && k === 'k') { stop(); A.toggleState('frozen'); return; } if (mod && k === 'j') { stop(); A.toggleState('off'); return; } if (mod && k === 'h') { stop(); A.toggleStrengthen(); return; } if (mod && k === 'f') { stop(); A.resetArrangement(selPieces().length ? selPieces() : null); return; }
    if (e.altKey && /^[6-9]$/.test(e.code.replace('Digit', '').replace('Numpad', ''))) { stop(); A.fit({ '6': 'stress', '7': 'strain', '8': 'fit', '9': 'pressure' }[e.code.slice(-1)]); return; }
    if (mod) return;
    if (k === ' ') { stop(); autoStopRef.current = !simOnRef.current && AT.simSettings(docRef.current).v.autoStop ? { n: 0 } : null; setSimOn(!simOnRef.current); return; }
    if (k === 'Escape') { stop(); if (ed && (ed.draft || ed.sewDraft || ed.measureLine || ed.fastenFrom)) ed.cancel(); else if (selRef.current) setSel(null); else setTool2d('transform'); setCtx(null); return; }
    if (k === 'Enter') { stop(); if (ed && ed.draft) ed.finishDraft(false); else if (ed && ed.sewDraft) ed.advanceSew(); return; }
    if (k === 'Delete' || k === 'Backspace') { stop(); if (ed && ed.draft && ed.draft.pts.length) { ed.draft.pts.pop(); ed.invalidate(); } else A.del(); return; }
    if (k.indexOf('Arrow') === 0) { const step = e.shiftKey ? 10 : 1; const dx = k === 'ArrowLeft' ? -step : k === 'ArrowRight' ? step : 0, dy = k === 'ArrowUp' ? -step : k === 'ArrowDown' ? step : 0; if (ed && ed.keyNudge(dx, dy)) stop(); return; }
    if (e.shiftKey) {
      if (k === 'A') { stop(); setDisp3({ avatar: !disp3.avatar }); return; } if (k === 'F') { stop(); setDisp3({ arrPts: !disp3.arrPts }); return; }
      if (k === 'Q') { stop(); A.hide3d(); return; } if (k === 'W') { stop(); A.showAll3d(); return; } if (k === 'Z') { stop(); ed && ed.fit(); return; } if (k === '?') { stop(); setDialog({ name: 'shortcuts' }); return; }
      return;
    }
    const views = { '2': 'front', '8': 'back', '4': 'right', '6': 'left', '1': 'q3r', '3': 'q3l', '5': 'top', '0': 'bottom' };
    if (views[k] && sceneRef.current) { stop(); sceneRef.current.view(views[k]); return; }
    const lk = k.toLowerCase();
    if (lk === 'q') { stop(); setTool3d('select'); return; } if (lk === 'w') { stop(); setTool3d('pin'); return; } if (lk === 'y') { stop(); setLibOpen(!libOpen); return; } if (lk === 'f') { stop(); A.focus3d(); return; }
    if (k === 'F10') { stop(); A.snapshot(); return; }
    const tool = Object.keys(AT.TOOLS2D).find((tk) => AT.TOOLS2D[tk].key === lk);
    if (tool) { stop(); setTool2d(tool); }
  };
  // ---------- splitter ----------
  const centerRef = React.useRef(null);
  const onSplit = (e) => { e.preventDefault(); e.stopPropagation(); const el = centerRef.current; if (!el) return; const r = el.getBoundingClientRect(); const mv = (ev) => setSplit(AT.clamp((ev.clientX - r.left) / r.width, 0.18, 0.82)); const up = () => { window.removeEventListener('pointermove', mv); window.removeEventListener('pointerup', up); }; window.addEventListener('pointermove', mv); window.addEventListener('pointerup', up); };

  const app = {
    doc: d, rev: rev, base: base, pid: pid, index: index, units: d ? d.units : 'cm', sel: sel, setSel: setSel, selPieces: selPieces(sel),
    tool2d: tool2d, setTool2d: setTool2d, tool3d: tool3d, setTool3d: setTool3d, mode: mode, setMode: setMode, simOn: simOn,
    toggleSim: () => { autoStopRef.current = !simOnRef.current && AT.simSettings(docRef.current).v.autoStop ? { n: 0 } : null; setSimOn(!simOnRef.current); },
    disp3: disp3, setDisp3: setDisp3, disp2: disp2, setDisp2: setDisp2, libOpen: libOpen, setLibOpen: setLibOpen, layout: layout, setLayout: setLayout,
    obTab: obTab, setObTab: setObTab, dialog: dialog, openDialog: (n) => setDialog(n ? { name: n } : null), ctx: ctx, setCtx: setCtx, toasts: toasts, toast: toast,
    actions: A, statsRef: statsRef, scene: sceneRef.current, editor: editorRef.current, snap: snap, setSnap: (v) => { snapRef.current = v; setSnap(v); },
    pendingGraphic: pendingGraphic ? Object.assign({}, pendingGraphic, { url: AT.resolveAsset(pendingGraphic) }) : null, setButtonPreset: (p) => { btnPresetRef.current = p; },
    cwThumbs: cwThumbs, tpImages: tpImages, composer: composer, setComposer: setComposer, saving: saving, fitSummary: fitSummary, peers: peers, userName: userName,
    undoLabel: undoRef.current.length ? undoRef.current[undoRef.current.length - 1].label : null, redoLabel: redoRef.current.length ? redoRef.current[redoRef.current.length - 1].label : null,
    missing: missing, width: size.w, height: size.h, pieceMenu: pieceMenu, showCtx: showCtx, ctx2D: ctx2D, avInfo: avInfo, mhState: mhState, avSizes: avSizes
  };
  appRef.current = app;
  if (typeof window !== 'undefined') window.__atelier = { AT: AT, app: appRef.current, scene: sceneRef.current, editor: editorRef.current, sim: simRef.current, doc: docRef.current, av: avRef.current };
  // button preset hook: apply to newly placed button
  React.useEffect(() => { const D0 = docRef.current, p = btnPresetRef.current; if (!D0 || !p || !sel || sel.kind !== 'button') return; const b = D0.buttons[sel.id]; if (b && !b._preset) { Object.assign(b, p); commit('Button style'); btnPresetRef.current = null; } }, [sel]);

  const v3Bg = dark ? 'radial-gradient(ellipse at 50% 35%, #34363c 0%, #23242a 55%, #18191c 100%)' : 'radial-gradient(ellipse at 50% 35%, #fafafa 0%, #ececee 55%, #dcdce0 100%)';
  const showRight = !narrow || rightOpen;
  return <Ctx.Provider value={app}>
    <div ref={rootRef} tabIndex={0} className={'atl w-full' + (dark ? '' : ' light')} onKeyDown={onKey}
      style={{ height: 'var(--mana-app-height, 82vh)', display: 'flex', flexDirection: 'column', position: 'relative', outline: 'none', overflow: 'hidden', borderRadius: 12, userSelect: 'none' }}>
      <style>{ATL_CSS}</style>
      <input ref={fileRef} type="file" style={{ display: 'none' }} onChange={onFile} />
      {!d && pid && !missing && <div style={{ flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center', color: 'var(--muted)' }}>Opening garment…</div>}
      {!d && (!pid || missing) && <StartScreen />}
      {d && <MenuBar />}
      <div style={{ flex: 1, display: d ? 'flex' : 'none', minHeight: 0, position: 'relative' }}>
        {libOpen && d && <div style={{ width: 240, flex: 'none', borderRight: '1px solid var(--line)', background: 'var(--panel)', minHeight: 0 }}><LibraryPanel /></div>}
        <div ref={centerRef} style={{ flex: 1, display: 'flex', minWidth: 0, position: 'relative' }}>
          <div style={{ flex: layout === 'split' ? split : 1, display: layout === '2d' ? 'none' : 'flex', flexDirection: 'column', minWidth: 0 }}>
            {d && <Toolbar3D />}
            <div style={{ flex: 1, position: 'relative', background: v3Bg, minHeight: 0 }}>
              <div ref={v3Ref} style={{ position: 'absolute', inset: 0 }} />
              {d && <ViewBar3D />}{d && <FitLegend />}
              {(busy || (!libsReady && !libErr)) && <div style={{ position: 'absolute', left: '50%', top: 14, transform: 'translateX(-50%)', padding: '6px 12px', borderRadius: 999, background: 'var(--panel)', border: '1px solid var(--line)', color: 'var(--muted)', zIndex: 6 }}>{busy || 'Loading 3D engine…'}</div>}
              {libErr && <div style={{ position: 'absolute', inset: 0, display: 'flex', alignItems: 'center', justifyContent: 'center', color: 'var(--muted)', padding: 20, textAlign: 'center' }}>{libErr}</div>}
              {d && disp3.arrPts && <div style={{ position: 'absolute', left: '50%', bottom: 12, transform: 'translateX(-50%)', padding: '5px 11px', borderRadius: 999, background: 'var(--panel)', border: '1px solid var(--line)', color: 'var(--muted)', zIndex: 5, whiteSpace: 'nowrap' }}>{app.selPieces.length ? 'Click a blue point to place the selected pattern' : 'Select a pattern, then click a blue point'}</div>}
            </div>
          </div>
          {layout === 'split' && <div onPointerDown={onSplit} style={{ width: 5, cursor: 'col-resize', background: 'var(--line)', flex: 'none', position: 'relative', zIndex: 4 }} />}
          <div style={{ flex: layout === 'split' ? 1 - split : 1, display: layout === '3d' ? 'none' : 'flex', flexDirection: 'column', minWidth: 0 }}>
            {d && <Toolbar2D />}
            <div style={{ flex: 1, position: 'relative', minHeight: 0 }}>
              <div ref={v2Ref} style={{ position: 'absolute', inset: 0 }} />
              {d && <ViewBar2D />}
            </div>
          </div>
          {tiny && d && <div style={{ position: 'absolute', right: 8, top: 44, zIndex: 8 }}><Seg value={layout} onChange={setLayout} options={[['3d', '3D'], ['2d', '2D']]} /></div>}
          {mode === 'techpack' && d && <div style={{ position: 'absolute', inset: 0, zIndex: 20 }}><TechPackView /></div>}
        </div>
        {d && showRight && <div style={{ width: narrow ? 280 : 304, flex: 'none', borderLeft: '1px solid var(--line)', background: 'var(--panel)', display: 'flex', flexDirection: 'column', minHeight: 0, position: narrow ? 'absolute' : 'relative', right: 0, top: 0, bottom: 0, zIndex: 25, boxShadow: narrow ? 'var(--shadow)' : 'none' }}>
          {narrow && <div style={{ position: 'absolute', left: -30, top: 6 }}><IB icon="close" sm tip="Hide panels" onClick={() => setRightOpen(false)} /></div>}
          {mode === 'grading' ? <GradingPanel /> : mode === 'colorway' ? <ColorwayPanel /> : mode === 'review' ? <ReviewPanel /> : mode === 'techpack' ? <PropertyEditor /> : <>
            <div style={{ height: '40%', minHeight: 150, borderBottom: '1px solid var(--line)', flex: 'none' }}><ObjectBrowser /></div>
            <div style={{ flex: 1, minHeight: 0 }}><PropertyEditor /></div>
          </>}
        </div>}
        {d && narrow && !rightOpen && <div style={{ position: 'absolute', right: 8, bottom: 8, zIndex: 9 }}><button className="atl-btn" onClick={() => setRightOpen(true)}><Icon name="settings" size={13} />Panels</button></div>}
      </div>
      {d && <StatusBar />}
      <ContextMenu />
      <Dialogs />
      <CommentComposer />
      <Toasts />
    </div>
  </Ctx.Provider>;
}

