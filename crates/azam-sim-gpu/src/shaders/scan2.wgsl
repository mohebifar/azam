// Triangle hash: exclusive prefix sum of the 256 block totals; the grand total goes to BS[256].
var<workgroup> sh: array<u32, 256>;
@compute @workgroup_size(256) fn main(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>) {
  let v = BS[lid.x]; sh[lid.x] = v; workgroupBarrier();
  for (var off = 1u; off < 256u; off = off * 2u) { var t = 0u; if (lid.x >= off) { t = sh[lid.x - off]; } workgroupBarrier(); sh[lid.x] = sh[lid.x] + t; workgroupBarrier(); }
  BS[lid.x] = sh[lid.x] - v;
  if (lid.x == 255u) { BS[256] = sh[255]; }
}
