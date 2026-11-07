// Accumulates a position correction of particle v in fixed point (1/SCALE mm) with a use count.
fn addD(v: u32, d: vec3<f32>) {
  atomicAdd(&DELTA[4u * v], i32(round(d.x * SCALE))); atomicAdd(&DELTA[4u * v + 1u], i32(round(d.y * SCALE)));
  atomicAdd(&DELTA[4u * v + 2u], i32(round(d.z * SCALE))); atomicAdd(&DELTA[4u * v + 3u], 1);
}
