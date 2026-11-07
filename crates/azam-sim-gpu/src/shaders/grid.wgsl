// Trilinear sample of the body distance grid: gradient in xyz, distance in w (1000 outside).
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
