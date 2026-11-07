// Substep: damping, held-back gravity and pressure on the velocity, speed clamp, then the move.
// Fixed particles only record their substep start.
@compute @workgroup_size(128) fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
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
}
