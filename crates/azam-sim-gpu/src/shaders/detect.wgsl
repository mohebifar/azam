// Vertex-triangle contact detection (self-collision included). The side a particle keeps is the
// side it was on at the substep start, or is fixed by layer order (the higher layer stays outside
// along the body normal); untangle codes override it for particles that poked through.
@compute @workgroup_size(128) fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
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
    if (layered && F.edgeOn != 0u) {
      // closest point on the triangle (edges and corners too): no gaps along ridges between layers
      let cb = closestBary(pos, xa.xyz, xb.xyz, xc.xyz); bu = cb.x; bv = cb.y; bw = cb.z;
      let cpt = bu * xa.xyz + bv * xb.xyz + bw * xc.xyz; dist = length(pos - cpt); sd = dot(pos - cpt, n);
    } else {
      // same layer: face region only (stable for folds of one garment)
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
    // untangling: this particle was found on the wrong side of this piece (it poked through); push it back
    var untg = 0u;
    if (tang != 0u && tang - 1u == pt) { side = tangWant; untg = 4u; }
    var hx = 0.0;
    if (layq != laya) { let pa = bitcast<f32>(PD[2u * tr.x + 1u].z); hx = 0.3 * max(select(bitcast<f32>(rq.z), 20.0, bitcast<f32>(rq.z) <= 0.0), pa); }
    if (dist > F.H0 + 2.0 + hx + (F.R - F.H0 - 2.0 - F.hxMax)) { continue; }
    let idx = atomicAdd(&CTRL[0], 1u);
    if (idx < F.cap) { CONT[idx] = vec4<u32>(q, t, side | select(0u, 2u, layered && F.edgeOn != 0u) | untg, bitcast<u32>(hx)); }
  }
}
