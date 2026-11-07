// Skin vertices against cloth triangles: casts every skin vertex in the triangle's hash cells
// along its normal and pushes the triangle out by the deepest violation of its clearance.
@compute @workgroup_size(128) fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
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
}
