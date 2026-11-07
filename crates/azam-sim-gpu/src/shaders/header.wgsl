// Shared declarations of every pass: the frame parameters, the constraint row, the dispatch
// descriptor and the fixed-point scale of the atomic correction accumulators.

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
  repairMargin: f32, bodyMot: u32, fricMode: u32, muCap: f32,
}

struct Con { i: u32, j: u32, L: f32, r: f32, cm: f32, kind: u32, p0: u32, p1: u32 }

struct DP { off: u32, cnt: u32, kind: u32, flag: u32 }

const SCALE: f32 = 4096.0;

@group(0) @binding(0) var<uniform> F: FP;
