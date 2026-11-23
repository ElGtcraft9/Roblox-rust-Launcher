//! (RustLauncher) Temporal upscaling with AMD FidelityFX Super Resolution 2
//! (FSR 2.2's upscaler as shipped in the FSR SDK 2.4.0,
//! `upscalers/fsr3/include/gpu/fsr2`, MIT licence: Copyright (C) 2026
//! Advanced Micro Devices, Inc.), ported to WGSL compute for wgpu. The passes
//! and their maths follow the SDK's shaders:
//!
//! 1. **reconstruct** (render size, `ffx_fsr2_reconstruct_previous_depth.h`):
//!    nearest depth over 3×3 and its motion vector (dilation), the previous
//!    frame's depth reconstructed by scattering this one's along the motion
//!    (atomic max: reverse-Z), and the lock luma;
//! 2. **luma mip** (render / 32, `ffx_fsr2_compute_luminance_pyramid.h`):
//!    mean log luma, the mip FSR 2 reads shading changes from (level 4);
//! 3. **depth clip** (render size, `ffx_fsr2_depth_clip.h`): disocclusion
//!    from the depth separation test (Ksep = 1.37e-5), motion and depth
//!    divergence (the accumulation mask), the colour in YCoCg;
//! 4. **lock** (render size, `ffx_fsr2_lock.h`): thin features (one-pixel
//!    ridges) locked at the output pixel they land on;
//! 5. **accumulate** (output size, `ffx_fsr2_accumulate.h`,
//!    `ffx_fsr2_reproject.h`, `ffx_fsr2_upsample.h`,
//!    `ffx_fsr2_postprocess_lock_status.h`): Lanczos-2 history reprojection
//!    with deringing, lock lifetime and luma, the 3×3 Lanczos upsample and
//!    its variance box, luma instability over the last four frames, history
//!    rectification and the weighted blend.
//!
//! Then RCAS (finish.rs). Where rustudio differs from what FSR 2 expects:
//! - the input is the finished display image (8-bit, after post), so the
//!   exposure is 1 and nothing is tonemapped (FSR 2's LDR path);
//! - motion vectors are the camera's, from depth and this and the last
//!   frame's view-projection (rustudio draws no per-object velocity); what
//!   moves on its own is left to the rectification box;
//! - there are no reactive or transparency masks (FSR 2 treats them as 0).
//!
//! The view draws with a sub-pixel jitter (Halton 2,3; FSR 2's phase count
//! 8 × (output / render)²) that `jitter` hands out frame by frame.

use egui_wgpu::wgpu;
use glam::{Mat4, Vec3, Vec4};

/// Studs to metres (FSR's depth divergence works in metres).
const METERS_PER_STUD: f32 = 0.28;

/// FSR 2 reads shading changes from luma mip 4: 1 / (2 << 4) of the render size.
const MIP_DIV: u32 = 32;

const SHADER: &str = r#"
struct P {
    render_size: vec2<f32>,
    out_size: vec2<f32>,
    jitter: vec2<f32>,
    frame_index: f32,
    phases: f32,
    inv_vp: mat4x4<f32>,
    prev_vp: mat4x4<f32>,
    near: f32,
    meters: f32,
    kfov: f32,
    pad0: f32,
    mip_size: vec2<f32>,
    pad1: vec2<f32>,
};
@group(0) @binding(0) var<uniform> p: P;

const EPSILON: f32 = 1e-3;                          // FSR2_EPSILON
const FLT_MAX: f32 = 3.402823466e+38;
const RECON_WEIGHT_THRESHOLD: f32 = 0.01;           // fReconstructedDepthBilinearWeightThreshold
const UPSAMPLE_WEIGHT_SCALE: f32 = 1.0 / 12.0;      // fUpsampleLanczosWeightScale
const MAX_ACC_LANCZOS_WEIGHT: f32 = 1.0;            // fMaxAccumulationLanczosWeight
const AVG_LANCZOS_WEIGHT: f32 = 0.74 / 12.0;        // fAverageLanczosWeightPerFrame
const MIP_DIV: f32 = 32.0;
const PI: f32 = 3.141592653589793;

fn rs() -> vec2<i32> { return vec2<i32>(p.render_size); }
fn os() -> vec2<i32> { return vec2<i32>(p.out_size); }
fn clamp_px(q: vec2<i32>, size: vec2<i32>) -> vec2<i32> {
    return clamp(q, vec2<i32>(0), size - vec2<i32>(1));
}
fn on_screen(q: vec2<i32>, size: vec2<i32>) -> bool {
    return all(q >= vec2<i32>(0)) && all(q < size);
}
fn uv_inside(uv: vec2<f32>) -> bool {
    return all(uv >= vec2<f32>(0.0)) && all(uv <= vec2<f32>(1.0));
}
// ClampUv: keep a bilinear footprint inside the texture.
fn clamp_uv(uv: vec2<f32>, size: vec2<f32>) -> vec2<f32> {
    return clamp(uv * size, vec2<f32>(0.5), size - vec2<f32>(0.5)) / size;
}
fn rgb_to_ycocg(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(0.25 * c.r + 0.5 * c.g + 0.25 * c.b, 0.5 * c.r - 0.5 * c.b, -0.25 * c.r + 0.5 * c.g - 0.25 * c.b);
}
fn ycocg_to_rgb(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(c.x + c.y - c.z, c.x + c.z, c.x - c.y - c.z);
}
// The display image is gamma-encoded; luminance wants linear light.
fn linear(c: vec3<f32>) -> vec3<f32> {
    return pow(max(c, vec3<f32>(0.0)), vec3<f32>(2.2));
}
fn rgb_to_luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}
fn perceived_luma(c: vec3<f32>) -> f32 {
    let l = rgb_to_luma(c);
    var pl = 0.0;
    if (l <= 216.0 / 24389.0) {
        pl = l * (24389.0 / 27.0);
    } else {
        pl = pow(l, 1.0 / 3.0) * 116.0 - 16.0;
    }
    return pl * 0.01;
}
fn min_div_max(a: f32, b: f32) -> f32 {
    let m = max(a, b);
    return select(0.0, min(a, b) / m, m != 0.0);
}
fn view_depth(d: f32) -> f32 {
    // Infinite reverse-Z: device depth = NEAR / view depth.
    return min(p.near / max(d, 1e-7), 65504.0);
}
fn lanczos2(x0: f32) -> f32 {
    let x = min(abs(x0), 2.0);
    if (x < EPSILON) { return 1.0; }
    return (sin(PI * x) / (PI * x)) * (sin(0.5 * PI * x) / (0.5 * PI * x));
}
fn lanczos2_approx_sq(x2_in: f32) -> f32 {
    let x2 = min(x2_in, 4.0);
    let a = (2.0 / 5.0) * x2 - 1.0;
    let b = (1.0 / 4.0) * x2 - 1.0;
    return ((25.0 / 16.0) * a * a - (25.0 / 16.0 - 1.0)) * (b * b);
}

struct Bilinear { base: vec2<i32>, w: vec4<f32> };
// GetBilinearSamplingData: the four texels around uv and their weights
// (offsets (0,0), (1,0), (0,1), (1,1)).
fn bilinear(uv: vec2<f32>, size: vec2<f32>) -> Bilinear {
    let pos = uv * size - 0.5;
    let base = floor(pos);
    let f = pos - base;
    return Bilinear(vec2<i32>(base), vec4<f32>((1.0 - f.x) * (1.0 - f.y), f.x * (1.0 - f.y), (1.0 - f.x) * f.y, f.x * f.y));
}
fn corner(i: i32) -> vec2<i32> {
    return vec2<i32>(i & 1, i >> 1);
}

// ------------------------------------------------------------ 1. reconstruct

@group(1) @binding(0) var in_color: texture_2d<f32>;
@group(1) @binding(1) var in_depth: texture_depth_2d;
@group(1) @binding(2) var w_depth: texture_storage_2d<r32float, write>;
@group(1) @binding(3) var w_mv: texture_storage_2d<rg32float, write>;
@group(1) @binding(4) var w_lock_luma: texture_storage_2d<r32float, write>;
@group(1) @binding(5) var<storage, read_write> recon: array<atomic<u32>>;

@compute @workgroup_size(8, 8)
fn reconstruct(@builtin(global_invocation_id) id: vec3<u32>) {
    let px = vec2<i32>(id.xy);
    let size = rs();
    if (!on_screen(px, size)) { return; }
    // FindNearestDepth: reverse-Z, so the nearest is the largest.
    var nearest = textureLoad(in_depth, px, 0);
    var nearest_px = px;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let q = px + vec2<i32>(x, y);
            if (!on_screen(q, size)) { continue; }
            let d = textureLoad(in_depth, q, 0);
            if (d > nearest) { nearest = d; nearest_px = q; }
        }
    }
    // The nearest sample's motion, from the camera: where its surface point
    // was on screen last frame (unjittered), minus where it is now.
    let uv_n = (vec2<f32>(nearest_px) + 0.5) / p.render_size;
    let ndc = vec2<f32>(uv_n.x * 2.0 - 1.0, 1.0 - uv_n.y * 2.0);
    let h = p.inv_vp * vec4<f32>(ndc, max(nearest, 1e-7), 1.0);
    let world = h.xyz / h.w;
    let prev = p.prev_vp * vec4<f32>(world, 1.0);
    var mv = vec2<f32>(0.0);
    if (prev.w > 1e-6) {
        let pn = prev.xy / prev.w;
        let prev_uv = vec2<f32>(pn.x * 0.5 + 0.5, 0.5 - pn.y * 0.5);
        let cur_uv = (vec2<f32>(nearest_px) + 0.5 - p.jitter) / p.render_size;
        mv = prev_uv - cur_uv;
    }
    textureStore(w_depth, px, vec4<f32>(nearest, 0.0, 0.0, 0.0));
    textureStore(w_mv, px, vec4<f32>(mv, 0.0, 0.0));
    // ReconstructPrevDepth: this depth, at the pixels it covered last frame.
    let m = mv * f32(length(mv * p.out_size) > 0.1);
    let b = bilinear((vec2<f32>(px) + 0.5) / p.render_size + m, p.render_size);
    for (var i = 0; i < 4; i++) {
        if (b.w[i] > RECON_WEIGHT_THRESHOLD) {
            let q = b.base + corner(i);
            if (on_screen(q, size)) {
                atomicMax(&recon[q.y * size.x + q.x], bitcast<u32>(nearest));
            }
        }
    }
    // ComputeLockInputLuma.
    let c = linear(textureLoad(in_color, px, 0).rgb);
    textureStore(w_lock_luma, px, vec4<f32>(pow(perceived_luma(c), 1.0 / 6.0), 0.0, 0.0, 0.0));
}

// ------------------------------------------------------------ 2. luma mip

@group(1) @binding(0) var m_color: texture_2d<f32>;
@group(1) @binding(1) var w_mip: texture_storage_2d<r32float, write>;

var<workgroup> m_sum: array<f32, 64>;
var<workgroup> m_count: array<f32, 64>;

// One workgroup per mip texel: the mean log luma of a 32x32 block.
@compute @workgroup_size(8, 8)
fn luma_mip(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>, @builtin(local_invocation_index) li: u32) {
    let size = rs();
    let base = vec2<i32>(wg.xy) * i32(MIP_DIV) + vec2<i32>(lid.xy) * 4;
    var s = 0.0;
    var n = 0.0;
    for (var y = 0; y < 4; y++) {
        for (var x = 0; x < 4; x++) {
            let q = base + vec2<i32>(x, y);
            if (on_screen(q, size)) {
                s += log(max(EPSILON, rgb_to_luma(linear(textureLoad(m_color, q, 0).rgb))));
                n += 1.0;
            }
        }
    }
    m_sum[li] = s;
    m_count[li] = n;
    workgroupBarrier();
    if (li == 0u) {
        var ts = 0.0;
        var tn = 0.0;
        for (var i = 0; i < 64; i++) {
            ts += m_sum[i];
            tn += m_count[i];
        }
        textureStore(w_mip, vec2<i32>(wg.xy), vec4<f32>(select(0.0, ts / tn, tn > 0.0), 0.0, 0.0, 0.0));
    }
}

// ------------------------------------------------------------ 3. depth clip

@group(1) @binding(0) var c_color: texture_2d<f32>;
@group(1) @binding(1) var c_depth: texture_2d<f32>;
@group(1) @binding(2) var c_mv: texture_2d<f32>;
@group(1) @binding(3) var c_prev_mv: texture_2d<f32>;
@group(1) @binding(4) var<storage, read> c_recon: array<u32>;
@group(1) @binding(5) var w_prepared: texture_storage_2d<rgba16float, write>;
@group(1) @binding(6) var w_masks: texture_storage_2d<rgba16float, write>;

fn recon_depth(q: vec2<i32>) -> f32 {
    let size = rs();
    let c = clamp_px(q, size);
    return bitcast<f32>(c_recon[c.y * size.x + c.x]);
}

// ComputeDepthClip: the surface here was hidden last frame when it is
// farther than what the last frame had there, by more than the separation
// FSR 2 asks for at this depth and field of view.
fn depth_clip(uv: vec2<f32>, cur_d: f32) -> f32 {
    let size = rs();
    let cur_z = view_depth(cur_d);
    let b = bilinear(uv, p.render_size);
    var depth = 0.0;
    var wsum = 0.0;
    for (var i = 0; i < 4; i++) {
        let q = b.base + corner(i);
        if (on_screen(q, size)) {
            let w = b.w[i];
            if (w > RECON_WEIGHT_THRESHOLD) {
                let prev_z = view_depth(recon_depth(q));
                let diff = cur_z - prev_z;
                if (diff > 0.0) {
                    let half_viewport = length(p.render_size);
                    let threshold = max(cur_z, prev_z);
                    let required = 1.37e-05 * p.kfov * half_viewport * threshold;
                    let resolution = clamp(length(p.render_size) / length(vec2<f32>(1920.0, 1080.0)), 0.0, 1.0);
                    let power = mix(1.0, 3.0, resolution);
                    depth += pow(clamp(required / diff, 0.0, 1.0), power) * w;
                    wsum += w;
                }
            }
        }
    }
    return select(0.0, clamp(1.0 - depth / wsum, 0.0, 1.0), wsum > 0.0);
}

// EvaluateSurface: a steep slope seen edge-on is not a disocclusion.
fn evaluate_surface(px: vec2<i32>) -> f32 {
    let d0 = view_depth(recon_depth(px + vec2<i32>(0, -1)));
    let d1 = view_depth(recon_depth(px));
    let d2 = view_depth(recon_depth(px + vec2<i32>(0, 1)));
    return 1.0 - f32(((d0 - d1) > (d1 * 0.01)) && ((d1 - d2) > (d2 * 0.01)));
}

fn motion_divergence(px: vec2<i32>) -> f32 {
    let size = rs();
    var min_convergence = 1.0;
    let nucleus = textureLoad(c_mv, px, 0).xy;
    var max_velocity = length(nucleus);
    if (length(nucleus * p.render_size) > 1e-2) {
        for (var y = -1; y <= 1; y++) {
            for (var x = -1; x <= 1; x++) {
                let m = textureLoad(c_mv, clamp_px(px + vec2<i32>(x, y), size), 0).xy;
                var v = length(m);
                max_velocity = max(v, max_velocity);
                v = max(v, max_velocity);
                min_convergence = min(min_convergence, dot(m / v, nucleus / v));
            }
        }
    }
    return clamp(1.0 - min_convergence, 0.0, 1.0) * clamp(max_velocity / 0.01, 0.0, 1.0);
}

fn depth_divergence(px: vec2<i32>) -> f32 {
    let size = rs();
    let max_m = 65504.0 * p.meters;
    var dmax = 0.0;
    var dmin = max_m;
    var sky = false;
    for (var y = -1; y < 2; y++) {
        for (var x = -1; x < 2; x++) {
            let q = px + vec2<i32>(x, y);
            let on = f32(on_screen(q, size));
            let z = view_depth(textureLoad(c_depth, clamp_px(q, size), 0).r) * p.meters * on;
            sky = sky || (z == max_m);
            dmin = min(dmin, z);
            dmax = max(dmax, z);
        }
    }
    return select((1.0 - dmin / dmax) * f32(!sky), 0.0, dmax <= 0.0);
}

fn temporal_motion_divergence(px: vec2<i32>) -> f32 {
    let size = rs();
    let uv = (vec2<f32>(px) + 0.5) / p.render_size;
    let mv = textureLoad(c_mv, px, 0).xy;
    let reproj = clamp_uv(uv + mv, p.render_size);
    let prev_mv = textureLoad(c_prev_mv, clamp_px(vec2<i32>(reproj * p.render_size), size), 0).xy;
    let px_distance = length(mv * p.out_size);
    if (px_distance <= 1.0) { return 0.0; }
    return mix(0.0, 1.0 - clamp(length(prev_mv) / length(mv), 0.0, 1.0), clamp(pow(px_distance / 20.0, 3.0), 0.0, 1.0));
}

@compute @workgroup_size(8, 8)
fn depth_clip_pass(@builtin(global_invocation_id) id: vec3<u32>) {
    let px = vec2<i32>(id.xy);
    let size = rs();
    if (!on_screen(px, size)) { return; }
    let uv = (vec2<f32>(px) + 0.5) / p.render_size;
    var mv = textureLoad(c_mv, px, 0).xy;
    mv *= f32(length(mv * p.out_size) > 0.01);
    let dilated_depth = textureLoad(c_depth, px, 0).r;
    let clip = depth_clip(uv + mv, dilated_depth) * evaluate_surface(px);
    let c = max(textureLoad(c_color, px, 0).rgb, vec3<f32>(0.0));
    textureStore(w_prepared, px, vec4<f32>(rgb_to_ycocg(c), clip));
    // PreProcessReactiveMasks with no reactive or composition masks: the
    // accumulation mask is the motion divergence.
    let temporal = clamp(temporal_motion_divergence(px) - depth_divergence(px), 0.0, 1.0);
    textureStore(w_masks, px, vec4<f32>(0.0, max(temporal, motion_divergence(px)), 0.0, 0.0));
}

// ------------------------------------------------------------ 4. lock

@group(1) @binding(0) var l_luma: texture_2d<f32>;
@group(1) @binding(1) var w_new_locks: texture_storage_2d<r32float, write>;

// ComputeThinFeatureConfidence: a one-pixel ridge (lines, wires, text) the
// history should keep instead of the jittered frame's flicker.
fn thin_feature(px: vec2<i32>) -> bool {
    let size = rs();
    let nucleus = textureLoad(l_luma, px, 0).r;
    var dis_min = FLT_MAX;
    var dis_max = 0.0;
    var mask = 1u << 4u;
    var idx = 0u;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            if (!(x == 0 && y == 0)) {
                let s = textureLoad(l_luma, clamp_px(px + vec2<i32>(x, y), size), 0).r;
                let lo = min(s, nucleus);
                let similar = lo > 0.0 && max(s, nucleus) / lo < 1.05;
                if (similar) {
                    mask |= 1u << idx;
                } else {
                    dis_min = min(dis_min, s);
                    dis_max = max(dis_max, s);
                }
            }
            idx += 1u;
        }
    }
    if (!(nucleus > dis_max || nucleus < dis_min)) { return false; }
    var rejection = array<u32, 4>(
        (1u << 0u) | (1u << 1u) | (1u << 3u) | (1u << 4u),
        (1u << 1u) | (1u << 2u) | (1u << 4u) | (1u << 5u),
        (1u << 3u) | (1u << 4u) | (1u << 6u) | (1u << 7u),
        (1u << 4u) | (1u << 5u) | (1u << 7u) | (1u << 8u));
    for (var i = 0; i < 4; i++) {
        if ((mask & rejection[i]) == rejection[i]) { return false; }
    }
    return true;
}

@compute @workgroup_size(8, 8)
fn lock_pass(@builtin(global_invocation_id) id: vec3<u32>) {
    let px = vec2<i32>(id.xy);
    if (!on_screen(px, rs())) { return; }
    if (thin_feature(px)) {
        // ComputeHrPosFromLrPos.
        let hr = vec2<i32>(floor((vec2<f32>(px) + 0.5 - p.jitter) / p.render_size * p.out_size));
        if (on_screen(hr, os())) {
            textureStore(w_new_locks, hr, vec4<f32>(1.0, 0.0, 0.0, 0.0));
        }
    }
}

// ------------------------------------------------------------ 5. accumulate

@group(1) @binding(0) var a_prepared: texture_2d<f32>;
@group(1) @binding(1) var a_masks: texture_2d<f32>;
@group(1) @binding(2) var a_mv: texture_2d<f32>;
@group(1) @binding(3) var a_history: texture_2d<f32>;
@group(1) @binding(4) var a_lock: texture_2d<f32>;
@group(1) @binding(5) var a_luma_history: texture_2d<f32>;
@group(1) @binding(6) var a_new_locks: texture_2d<f32>;
@group(1) @binding(7) var a_mip: texture_2d<f32>;
@group(1) @binding(8) var w_history: texture_storage_2d<rgba16float, write>;
@group(1) @binding(9) var w_lock: texture_storage_2d<rgba16float, write>;
@group(1) @binding(10) var w_luma_history: texture_storage_2d<rgba8unorm, write>;
@group(1) @binding(11) var w_out: texture_storage_2d<rgba8unorm, write>;
@group(1) @binding(12) var a_sampler: sampler;

// The history at uv: Lanczos-2 over 4x4 texels, clamped to the nearest
// four (FSR 2's reprojection sampler with deringing).
fn history_sample(uv: vec2<f32>) -> vec4<f32> {
    let size = os();
    let pos = uv * p.out_size - 0.5;
    let base = vec2<i32>(floor(pos));
    let f = pos - floor(pos);
    var wx = array<f32, 4>(lanczos2(-1.0 - f.x), lanczos2(-f.x), lanczos2(1.0 - f.x), lanczos2(2.0 - f.x));
    var wy = array<f32, 4>(lanczos2(-1.0 - f.y), lanczos2(-f.y), lanczos2(1.0 - f.y), lanczos2(2.0 - f.y));
    let sx = wx[0] + wx[1] + wx[2] + wx[3];
    let sy = wy[0] + wy[1] + wy[2] + wy[3];
    var s = vec4<f32>(0.0);
    var lo = vec4<f32>(FLT_MAX);
    var hi = vec4<f32>(-FLT_MAX);
    for (var y = 0; y < 4; y++) {
        var row = vec4<f32>(0.0);
        for (var x = 0; x < 4; x++) {
            let c = textureLoad(a_history, clamp_px(base + vec2<i32>(x - 1, y - 1), size), 0);
            row += c * wx[x];
            if ((x == 1 || x == 2) && (y == 1 || y == 2)) {
                lo = min(lo, c);
                hi = max(hi, c);
            }
        }
        s += (row / sx) * wy[y];
    }
    return clamp(s / sy, lo, hi);
}

// The shading-change luma at uv: the luma mip, bilinear.
fn shading_change_luma(uv: vec2<f32>) -> f32 {
    let msize = vec2<i32>(p.mip_size);
    let render_mip = max(vec2<f32>(vec2<i32>(p.render_size / MIP_DIV)), vec2<f32>(1.0));
    let uvc = clamp(uv * render_mip, vec2<f32>(0.5), render_mip - vec2<f32>(0.5)) / p.mip_size;
    let b = bilinear(uvc, p.mip_size);
    var v = 0.0;
    for (var i = 0; i < 4; i++) {
        v += textureLoad(a_mip, clamp_px(b.base + corner(i), msize), 0).r * b.w[i];
    }
    return pow(exp(v), 1.0 / 6.0);
}

@compute @workgroup_size(8, 8)
fn accumulate(@builtin(global_invocation_id) id: vec3<u32>) {
    let hr = vec2<i32>(id.xy);
    if (!on_screen(hr, os())) { return; }
    let size = rs();
    let downscale = p.render_size / p.out_size;

    // InitParams.
    let hr_uv = (vec2<f32>(hr) + 0.5) / p.out_size;
    let lr_uv = clamp_uv(hr_uv + p.jitter / p.render_size, p.render_size);
    let mv = textureLoad(a_mv, clamp_px(vec2<i32>(hr_uv * p.render_size), size), 0).xy;
    let hr_velocity = length(mv * p.out_size);
    let reproj = hr_uv + mv;
    let existing = uv_inside(reproj);
    let depth_clip = clamp(textureSampleLevel(a_prepared, a_sampler, lr_uv, 0.0).w, 0.0, 1.0);
    let masks = textureSampleLevel(a_masks, a_sampler, lr_uv, 0.0);
    let dilated_reactive = masks.x;
    let acc_mask = masks.y;
    let reset = p.frame_index < 0.5;
    let is_new = !existing || reset;

    // ReprojectHistoryColor and ReprojectHistoryLockStatus.
    var hist = vec3<f32>(0.0);
    var lock_status = vec2<f32>(0.0);
    var temporal_reactive = 0.0;
    var in_motion_last = false;
    var new_lock = false;
    if (existing && !reset) {
        let h = history_sample(reproj);
        hist = rgb_to_ycocg(h.rgb);
        temporal_reactive = clamp(abs(h.w), 0.0, 1.0);
        in_motion_last = h.w < 0.0;
        new_lock = textureLoad(a_new_locks, hr, 0).r > 127.0 / 255.0;
        lock_status = textureSampleLevel(a_lock, a_sampler, reproj, 0.0).xy;
    }
    var this_reactive = max(dilated_reactive, temporal_reactive);

    // UpdateLockStatus (x: lifetime remaining, y: temporal luma).
    let sc_luma = shading_change_luma(hr_uv);
    if (lock_status.y == 0.0) { lock_status.y = sc_luma; }
    let luma_diff = 1.0 - min_div_max(lock_status.y, sc_luma);
    if (new_lock) {
        lock_status.y = sc_luma;
        lock_status.x = select(1.0, 2.0, lock_status.x != 0.0);
    } else if (lock_status.x <= 1.0) {
        lock_status.y = mix(lock_status.y, sc_luma, 0.5);
    } else if (luma_diff > 0.1) {
        lock_status.x = 0.0;
    }
    this_reactive = max(this_reactive, clamp((luma_diff - 0.1) * 10.0, 0.0, 1.0));
    lock_status.x *= 1.0 - this_reactive;
    lock_status.x *= clamp(1.0 - acc_mask, 0.0, 1.0);
    lock_status.x *= f32(depth_clip < 0.1);
    let lifetime_contrib = clamp(lock_status.x - 1.0, 0.0, 1.0);
    let shading_contrib = clamp(min_div_max(lock_status.y, sc_luma), 0.0, 1.0);
    let lock_contrib = clamp(clamp(lifetime_contrib * 4.0, 0.0, 1.0) * shading_contrib, 0.0, 1.0);

    // ComputeUpsampledColorAndWeight: Lanczos-2 over the 3x3 jittered
    // samples around this output pixel, and their rectification box.
    let src_out = (vec2<f32>(hr) + 0.5) * downscale;
    let src_in = vec2<i32>(floor(src_out));
    let src_unj = vec2<f32>(src_in) + 0.5 - p.jitter;
    let flip_col = src_unj.x > src_out.x;
    let flip_row = src_unj.y > src_out.y;
    let tl = vec2<i32>(select(-1, -2, flip_col), select(-1, -2, flip_row));
    let base_off = src_unj - src_out;
    let kernel_reactive = max(this_reactive, f32(is_new));
    let max_kernel = min(1.99, 1.0 + (1.0 / downscale.x - 1.0));
    let kb_max = max_kernel * (1.0 - kernel_reactive);
    let kb_min = max(1.0, (1.0 + kb_max) * 0.3);
    let kernel_bias = mix(kb_max, kb_min, max(0.0, max(0.25 * depth_clip, kernel_reactive)));
    let curve_bias = mix(-2.0, -3.0, clamp(hr_velocity / 50.0, 0.0, 1.0));
    var up = vec4<f32>(0.0);
    var box_min = vec3<f32>(0.0);
    var box_max = vec3<f32>(0.0);
    var box_center = vec3<f32>(0.0);
    var box_vec = vec3<f32>(0.0);
    var box_w = 0.0;
    for (var row = 0; row < 3; row++) {
        for (var col = 0; col < 3; col++) {
            let cr = vec2<i32>(select(col, 3 - col, flip_col), select(row, 3 - row, flip_row));
            let q = src_in + tl + cr;
            let off = base_off + vec2<f32>(tl + cr);
            let s = textureLoad(a_prepared, clamp_px(q, size), 0).xyz;
            let kb = off * kernel_bias;
            let w = f32(on_screen(q, size)) * lanczos2_approx_sq(dot(kb, kb));
            up += vec4<f32>(s * w, w);
            let bw = exp(curve_bias * dot(off, off));
            if (row == 0 && col == 0) {
                box_min = s; box_max = s;
                box_center = s * bw; box_vec = s * s * bw; box_w = bw;
            } else {
                box_min = min(box_min, s); box_max = max(box_max, s);
                box_center += s * bw; box_vec += s * s * bw; box_w += bw;
            }
        }
    }
    box_w = select(1.0, box_w, abs(box_w) > EPSILON);
    box_center /= box_w;
    box_vec = sqrt(abs(box_vec / box_w - box_center * box_center));
    up.w *= f32(up.w > EPSILON);
    if (up.w > EPSILON) {
        up = vec4<f32>(clamp(up.xyz / up.w, box_min, box_max), up.w * UPSAMPLE_WEIGHT_SCALE);
    }

    // ComputeLumaInstabilityFactor (the luma of the last four frames).
    let cur_luma = round(box_center.x * 255.0) / 255.0;
    let sample_history = max(max(depth_clip, acc_mask), luma_diff) < 0.1 && !is_new;
    var luma_hist = select(vec4<f32>(0.0), textureSampleLevel(a_luma_history, a_sampler, reproj, 0.0), sample_history);
    var instability = 0.0;
    let d0 = cur_luma - luma_hist.x;
    var lowest = abs(d0);
    if (lowest >= 1.0 / 255.0) {
        for (var i = 1; i < 4; i++) {
            let d1 = cur_luma - luma_hist[i];
            if (sign(d0) == sign(d1)) { lowest = min(lowest, abs(d1)); }
        }
        let box_size_factor = pow(clamp(box_vec.x / 0.1, 0.0, 1.0), 6.0);
        instability = f32(lowest != abs(d0)) * box_size_factor;
        instability = f32(instability > 1.0 / 255.0);
        instability *= 1.0 - max(acc_mask, pow(this_reactive, 1.0 / 6.0));
    }
    luma_hist = vec4<f32>(cur_luma, luma_hist.x, luma_hist.y, luma_hist.z);
    textureStore(w_luma_history, hr, luma_hist);
    instability *= f32(luma_hist.w != 0.0);

    // ComputeBaseAccumulationWeight.
    var base_acc = MAX_ACC_LANCZOS_WEIGHT * f32(existing) * (1.0 - this_reactive) * (1.0 - depth_clip);
    base_acc = min(base_acc, mix(base_acc, up.w * 10.0, max(f32(in_motion_last), clamp(hr_velocity * 10.0, 0.0, 1.0))));
    base_acc = min(base_acc, mix(base_acc, up.w, clamp(hr_velocity / 20.0, 0.0, 1.0)));
    var accumulation = vec3<f32>(base_acc);

    var rgb = vec3<f32>(0.0);
    if (is_new) {
        rgb = ycocg_to_rgb(up.xyz);
    } else {
        // RectifyHistory.
        let scale_influence = min(20.0, pow(1.0 / abs(downscale.x * downscale.y), 3.0));
        let box_t = max(depth_clip, max(acc_mask, clamp(hr_velocity / 20.0, 0.0, 1.0)));
        let scaled = box_vec * mix(scale_influence, 1.0, box_t);
        let lo = max(box_min, box_center - scaled);
        let hi = min(box_max, box_center + scaled);
        if (any(lo > hist) || any(hist > hi)) {
            let contribution = clamp(max(instability, lock_contrib) * (1.0 - sqrt(dilated_reactive)), 0.0, 1.0);
            hist = mix(clamp(hist, lo, hi), hist, contribution);
            accumulation = mix(min(accumulation, vec3<f32>(0.1)), accumulation, contribution);
        }
        // Accumulate.
        accumulation = max(vec3<f32>(EPSILON), accumulation + vec3<f32>(up.w));
        rgb = ycocg_to_rgb(mix(hist, up.xyz, vec3<f32>(up.w) / accumulation));
    }
    rgb = max(rgb, vec3<f32>(0.0));

    // FinalizeLockStatus.
    if (!uv_inside(hr_uv - mv)) {
        lock_status.x = 0.0;
    } else {
        lock_status.x = max(0.0, lock_status.x - up.w / (p.phases * AVG_LANCZOS_WEIGHT));
    }
    textureStore(w_lock, hr, vec4<f32>(lock_status, 0.0, 0.0));

    // ComputeTemporalReactiveFactor (negative: in motion this frame).
    var reactive_next = min(0.99, this_reactive);
    reactive_next = max(reactive_next, mix(reactive_next, 0.4, clamp(hr_velocity, 0.0, 1.0)));
    reactive_next = max(reactive_next * reactive_next, max(depth_clip * 0.1, dilated_reactive));
    if (is_new) { reactive_next = 1.0; }
    if (clamp(hr_velocity * 10.0, 0.0, 1.0) >= 1.0) { reactive_next = -max(EPSILON, reactive_next); }
    textureStore(w_history, hr, vec4<f32>(rgb, reactive_next));
    textureStore(w_out, hr, vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0));
}
"#;

struct Tex {
    _tex: wgpu::Texture,
    view: wgpu::TextureView,
}

fn texture(device: &wgpu::Device, label: &str, size: [u32; 2], format: wgpu::TextureFormat, render: bool) -> Tex {
    let mut usage = wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING;
    if render {
        usage |= wgpu::TextureUsages::RENDER_ATTACHMENT;
    }
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: size[0].max(1), height: size[1].max(1), depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    });
    let view = tex.create_view(&Default::default());
    Tex { _tex: tex, view }
}

struct Targets {
    render_size: [u32; 2],
    out_size: [u32; 2],
    mip_size: [u32; 2],
    depth: Tex,
    /// Ping-ponged: this frame's and the last one's.
    mv: [Tex; 2],
    lock_luma: Tex,
    mip: Tex,
    prepared: Tex,
    masks: Tex,
    /// The previous frame's depth, reconstructed (f32 bits, atomic max).
    recon: wgpu::Buffer,
    new_locks: Tex,
    history: [Tex; 2],
    lock: [Tex; 2],
    luma_history: [Tex; 2],
    out: Tex,
}

/// What the frame was drawn with (`Renderer::set_temporal`).
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    /// The jitter in render pixels (`Fsr2::jitter`).
    pub jitter: [f32; 2],
    /// The view-projection without the jitter (the next frame's "previous").
    pub view_proj: Mat4,
    /// The inverse of the jittered view-projection the frame drew with.
    pub inv_view_proj_jittered: Mat4,
    pub cam_pos: Vec3,
}

pub struct Fsr2 {
    globals_layout: wgpu::BindGroupLayout,
    reconstruct_layout: wgpu::BindGroupLayout,
    mip_layout: wgpu::BindGroupLayout,
    clip_layout: wgpu::BindGroupLayout,
    lock_layout: wgpu::BindGroupLayout,
    accumulate_layout: wgpu::BindGroupLayout,
    reconstruct: wgpu::ComputePipeline,
    mip: wgpu::ComputePipeline,
    clip: wgpu::ComputePipeline,
    lock: wgpu::ComputePipeline,
    accumulate: wgpu::ComputePipeline,
    params: wgpu::Buffer,
    sampler: wgpu::Sampler,
    targets: Option<Targets>,
    frame_index: u32,
    last: Option<Frame>,
}

pub fn halton(mut index: u32, base: u32) -> f32 {
    let (mut f, mut r) = (1.0f32, 0.0f32);
    while index > 0 {
        f /= base as f32;
        r += f * (index % base) as f32;
        index /= base;
    }
    r
}

/// FSR 2's jitter phase count for a render / output width
/// (`ffxFsr2GetJitterPhaseCount`).
pub fn phase_count(render_width: u32, out_width: u32) -> u32 {
    ((8.0 * (out_width as f32 / render_width.max(1) as f32).powi(2)) as u32).max(1)
}

/// How much farther a screen corner is than the centre at the same view
/// depth (FSR 2's Kfov), from the inverse view-projection.
fn kfov(inv_view_proj: Mat4, cam_pos: Vec3) -> f32 {
    let at = |x: f32, y: f32| {
        let h = inv_view_proj * Vec4::new(x, y, 0.5, 1.0);
        h.truncate() / h.w
    };
    let centre = at(0.0, 0.0).distance(cam_pos);
    let corner = at(-1.0, 1.0).distance(cam_pos);
    if centre > 1e-6 && corner.is_finite() { (corner / centre).max(1.0) } else { 1.0 }
}

impl Fsr2 {
    pub fn new(device: &wgpu::Device) -> Self {
        use wgpu::{BindGroupLayoutEntry as E, BindingType as T, ShaderStages as S};
        let tex = |binding, sample_type| E {
            binding,
            visibility: S::COMPUTE,
            ty: T::Texture { sample_type, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
            count: None,
        };
        let unfilterable = wgpu::TextureSampleType::Float { filterable: false };
        let filterable = wgpu::TextureSampleType::Float { filterable: true };
        let store = |binding, format| E {
            binding,
            visibility: S::COMPUTE,
            ty: T::StorageTexture { access: wgpu::StorageTextureAccess::WriteOnly, format, view_dimension: wgpu::TextureViewDimension::D2 },
            count: None,
        };
        let buffer = |binding, read_only| E {
            binding,
            visibility: S::COMPUTE,
            ty: T::Buffer { ty: wgpu::BufferBindingType::Storage { read_only }, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        };
        use wgpu::TextureFormat as F;
        let layout = |label, entries: &[E]| device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some(label), entries });
        let globals_layout = layout(
            "fsr2 params",
            &[E {
                binding: 0,
                visibility: S::COMPUTE,
                ty: T::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        );
        let reconstruct_layout = layout(
            "fsr2 reconstruct",
            &[
                tex(0, unfilterable),
                tex(1, wgpu::TextureSampleType::Depth),
                store(2, F::R32Float),
                store(3, F::Rg32Float),
                store(4, F::R32Float),
                buffer(5, false),
            ],
        );
        let mip_layout = layout("fsr2 luma mip", &[tex(0, unfilterable), store(1, F::R32Float)]);
        let clip_layout = layout(
            "fsr2 depth clip",
            &[
                tex(0, unfilterable),
                tex(1, unfilterable),
                tex(2, unfilterable),
                tex(3, unfilterable),
                buffer(4, true),
                store(5, F::Rgba16Float),
                store(6, F::Rgba16Float),
            ],
        );
        let lock_layout = layout("fsr2 lock", &[tex(0, unfilterable), store(1, F::R32Float)]);
        let accumulate_layout = layout(
            "fsr2 accumulate",
            &[
                tex(0, filterable),
                tex(1, filterable),
                tex(2, unfilterable),
                tex(3, unfilterable),
                tex(4, filterable),
                tex(5, filterable),
                tex(6, unfilterable),
                tex(7, unfilterable),
                store(8, F::Rgba16Float),
                store(9, F::Rgba16Float),
                store(10, F::Rgba8Unorm),
                store(11, F::Rgba8Unorm),
                E { binding: 12, visibility: S::COMPUTE, ty: T::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        );
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fsr2"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline = |entry: &str, pass_layout: &wgpu::BindGroupLayout| {
            let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(entry),
                bind_group_layouts: &[Some(&globals_layout), Some(pass_layout)],
                immediate_size: 0,
            });
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&pl),
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let reconstruct = pipeline("reconstruct", &reconstruct_layout);
        let mip = pipeline("luma_mip", &mip_layout);
        let clip = pipeline("depth_clip_pass", &clip_layout);
        let lock = pipeline("lock_pass", &lock_layout);
        let accumulate = pipeline("accumulate", &accumulate_layout);
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fsr2 params"),
            size: 192,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("fsr2"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            globals_layout,
            reconstruct_layout,
            mip_layout,
            clip_layout,
            lock_layout,
            accumulate_layout,
            reconstruct,
            mip,
            clip,
            lock,
            accumulate,
            params,
            sampler,
            targets: None,
            frame_index: 0,
            last: None,
        }
    }

    /// Forget the history (a new place, a cut).
    pub fn reset(&mut self) {
        self.last = None;
    }

    /// Upscales `color` (render size, the frame drawn with `frame`'s
    /// jitter) to `out_size` with its depth; returns the result (8-bit,
    /// before sharpening).
    #[allow(clippy::too_many_arguments)]
    pub fn run(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        render_size: [u32; 2],
        out_size: [u32; 2],
        frame: Frame,
    ) -> &wgpu::TextureView {
        use wgpu::TextureFormat as F;
        let resized = self.targets.as_ref().is_none_or(|t| t.render_size != render_size || t.out_size != out_size);
        if resized {
            let r = render_size;
            let o = out_size;
            let m = [r[0].div_ceil(MIP_DIV).max(1), r[1].div_ceil(MIP_DIV).max(1)];
            let pair = |label, size, format| [texture(device, label, size, format, false), texture(device, label, size, format, false)];
            self.targets = Some(Targets {
                render_size,
                out_size,
                mip_size: m,
                depth: texture(device, "fsr2 dilated depth", r, F::R32Float, false),
                mv: pair("fsr2 dilated motion", r, F::Rg32Float),
                lock_luma: texture(device, "fsr2 lock luma", r, F::R32Float, false),
                mip: texture(device, "fsr2 luma mip", m, F::R32Float, false),
                prepared: texture(device, "fsr2 prepared colour", r, F::Rgba16Float, false),
                masks: texture(device, "fsr2 reactive masks", r, F::Rgba16Float, false),
                recon: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("fsr2 reconstructed depth"),
                    size: (r[0].max(1) as u64) * (r[1].max(1) as u64) * 4,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                new_locks: texture(device, "fsr2 new locks", o, F::R32Float, true),
                history: pair("fsr2 history", o, F::Rgba16Float),
                lock: pair("fsr2 lock status", o, F::Rgba16Float),
                luma_history: pair("fsr2 luma history", o, F::Rgba8Unorm),
                out: texture(device, "fsr2 output", o, F::Rgba8Unorm, false),
            });
            self.last = None;
        }
        // A cut (the camera jumped) starts the history over.
        let reset = match self.last {
            None => true,
            Some(last) => last.cam_pos.distance(frame.cam_pos) > 40.0,
        };
        if reset {
            self.frame_index = 0;
        }
        let t = self.targets.as_ref().expect("targets");
        let prev_vp = self.last.map_or(frame.view_proj, |l| l.view_proj);
        let mut params = [0f32; 48];
        params[0] = render_size[0] as f32;
        params[1] = render_size[1] as f32;
        params[2] = out_size[0] as f32;
        params[3] = out_size[1] as f32;
        params[4] = frame.jitter[0];
        params[5] = frame.jitter[1];
        params[6] = self.frame_index as f32;
        params[7] = phase_count(render_size[0], out_size[0]) as f32;
        params[8..24].copy_from_slice(&frame.inv_view_proj_jittered.to_cols_array());
        params[24..40].copy_from_slice(&prev_vp.to_cols_array());
        params[40] = rustudio_core::mech::NEAR;
        params[41] = METERS_PER_STUD;
        params[42] = kfov(frame.inv_view_proj_jittered, frame.cam_pos);
        params[44] = t.mip_size[0] as f32;
        params[45] = t.mip_size[1] as f32;
        queue.write_buffer(&self.params, 0, bytemuck::cast_slice(&params));

        let cur = (self.frame_index & 1) as usize;
        let prev = 1 - cur;
        let views = |layout: &wgpu::BindGroupLayout, views: &[&wgpu::TextureView], extra: Vec<wgpu::BindingResource>| {
            let mut entries: Vec<wgpu::BindGroupEntry> = views
                .iter()
                .enumerate()
                .map(|(i, v)| wgpu::BindGroupEntry { binding: i as u32, resource: wgpu::BindingResource::TextureView(v) })
                .collect();
            for r in extra {
                entries.push(wgpu::BindGroupEntry { binding: entries.len() as u32, resource: r });
            }
            device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("fsr2"), layout, entries: &entries })
        };
        let globals = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fsr2 params"),
            layout: &self.globals_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: self.params.as_entire_binding() }],
        });
        let reconstruct_bg = views(
            &self.reconstruct_layout,
            &[color, depth, &t.depth.view, &t.mv[cur].view, &t.lock_luma.view],
            vec![t.recon.as_entire_binding()],
        );
        let mip_bg = views(&self.mip_layout, &[color, &t.mip.view], vec![]);
        let clip_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fsr2 depth clip"),
            layout: &self.clip_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(color) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&t.depth.view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&t.mv[cur].view) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&t.mv[prev].view) },
                wgpu::BindGroupEntry { binding: 4, resource: t.recon.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(&t.prepared.view) },
                wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::TextureView(&t.masks.view) },
            ],
        });
        let lock_bg = views(&self.lock_layout, &[&t.lock_luma.view, &t.new_locks.view], vec![]);
        let accumulate_bg = views(
            &self.accumulate_layout,
            &[
                &t.prepared.view,
                &t.masks.view,
                &t.mv[cur].view,
                &t.history[prev].view,
                &t.lock[prev].view,
                &t.luma_history[prev].view,
                &t.new_locks.view,
                &t.mip.view,
                &t.history[cur].view,
                &t.lock[cur].view,
                &t.luma_history[cur].view,
                &t.out.view,
            ],
            vec![wgpu::BindingResource::Sampler(&self.sampler)],
        );
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("fsr2") });
        // The new locks and the reconstructed depth (0 = far, reverse-Z)
        // start over each frame.
        drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fsr2 clear locks"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &t.new_locks.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        }));
        encoder.clear_buffer(&t.recon, 0, None);
        let groups = |n: u32| n.div_ceil(8);
        let mip_groups = t.mip_size;
        for (pipeline, bg, dispatch) in [
            (&self.reconstruct, &reconstruct_bg, [groups(render_size[0]), groups(render_size[1])]),
            (&self.mip, &mip_bg, mip_groups),
            (&self.clip, &clip_bg, [groups(render_size[0]), groups(render_size[1])]),
            (&self.lock, &lock_bg, [groups(render_size[0]), groups(render_size[1])]),
            (&self.accumulate, &accumulate_bg, [groups(out_size[0]), groups(out_size[1])]),
        ] {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("fsr2"), timestamp_writes: None });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &globals, &[]);
            pass.set_bind_group(1, bg, &[]);
            pass.dispatch_workgroups(dispatch[0], dispatch[1], 1);
        }
        queue.submit([encoder.finish()]);
        self.last = Some(frame);
        self.frame_index = self.frame_index.wrapping_add(1).max(1);
        &self.targets.as_ref().expect("targets").out.view
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jitter_stays_inside_a_pixel_and_covers_it() {
        let mut f = (0..64).map(|i| [halton(i % 8 + 1, 2) - 0.5, halton(i % 8 + 1, 3) - 0.5]);
        assert!(f.all(|j| j.iter().all(|v| (-0.5..=0.5).contains(v))));
        assert_eq!(phase_count(1280, 1280), 8);
        assert_eq!(phase_count(640, 1280), 32);
    }

    #[test]
    fn kfov_grows_with_the_field_of_view() {
        let eye = Vec3::new(0.0, 5.0, 10.0);
        let view = Mat4::look_at_rh(eye, Vec3::ZERO, Vec3::Y);
        let k = |fov: f32| kfov((Mat4::perspective_infinite_reverse_rh(fov, 16.0 / 9.0, 0.1) * view).inverse(), eye);
        let (narrow, wide) = (k(0.5), k(1.4));
        assert!(narrow > 1.0 && wide > narrow, "{narrow} {wide}");
        // sqrt(1 + tan²(fx/2) + tan²(fy/2)) for a 70° vertical field of view.
        let ty = (70f32.to_radians() / 2.0).tan();
        let expected = (1.0 + ty * ty + (ty * 16.0 / 9.0).powi(2)).sqrt();
        assert!((k(70f32.to_radians()) - expected).abs() < 1e-3);
    }
}
