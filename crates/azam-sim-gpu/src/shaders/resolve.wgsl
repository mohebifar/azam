// Contact resolution (every substep): keeps the thickness on the wanted side, plus friction while
// touching. Corrections are accumulated and averaged by the apply pass.
@compute @workgroup_size(128) fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
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
  // not touching: no push and no friction (cloth that is merely close must stay free to slide apart)
  if (sCur >= h) { return; }
  let wq = xq.w; let wa = xa.w * bu; let wb = xb.w * bv; let wc = xc.w * bw;
  let den = wq + wa * bu + wb * bv + wc * bw; if (den <= 0.0) { return; }
  // untangling pushes gently
  let dn = min(h - sCur, select(F.maxStep, F.maxStep * 0.3, (ct.z & 4u) != 0u));
  let f = dirn * (dn / den);
  var dq = f * wq; var da = -f * wa; var db = -f * wb; var dc = -f * wc;
  // friction only while touching. fricMode 0: Coulomb (tangential correction bounded by mu times the
  // normal correction); fricMode 1: remove a fraction mu of this substep's relative sliding (capped by muCap)
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
}
