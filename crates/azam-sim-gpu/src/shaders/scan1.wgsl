// Triangle hash: exclusive prefix sum of the bucket counts within blocks of 256, block totals to BS.
var<workgroup> sh: array<u32, 256>;
@compute @workgroup_size(256) fn main(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>, @builtin(workgroup_id) wid: vec3<u32>) {
  let i = gid.x; var v = 0u; if (i < F.HS) { v = HC[i]; }
  sh[lid.x] = v; workgroupBarrier();
  for (var off = 1u; off < 256u; off = off * 2u) { var t = 0u; if (lid.x >= off) { t = sh[lid.x - off]; } workgroupBarrier(); sh[lid.x] = sh[lid.x] + t; workgroupBarrier(); }
  if (i < F.HS) { HASH[i] = sh[lid.x] - v; }
  if (lid.x == 255u) { BS[wid.x] = sh[255]; }
}
