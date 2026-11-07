// Per frame: gravity hold-back while seams are open, globally (ST[0]) and per piece (ST[8 + r]);
// writes each piece's vertical acceleration to ST[80 + r] and clears the gap sums.
@compute @workgroup_size(1) fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
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
}
