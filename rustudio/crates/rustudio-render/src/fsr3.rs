//! (RustLauncher) Temporal upscaling after AMD FidelityFX Super Resolution
//! 3.1's upscaler (FSR SDK 2.4.0, `upscalers/fsr3`, MIT licence:
//! Copyright (C) 2026 Advanced Micro Devices, Inc.), ported to WGSL compute
//! for wgpu. The passes and their maths follow the SDK's:
//!
//! 1. **prepare** (render size): depth extents over 3×3, the nearest
//!    sample's motion vector (dilation), linear depth, luma;
//! 2. **reactivity** (render size): disocclusion from the depth separation
//!    test (`ComputeDisocclusions`, Ksep = 1.37e-5), shading change,
//!    per-pixel accumulation (`UpdateAccumulation`: +1/3 a frame, down to
//!    -1/3 on disocclusion), thin-feature locks (`ComputeThinFeatureConfidence`);
//! 3. **accumulate** (output size): history reprojected (bicubic), the
//!    current frame upsampled with a Lanczos-2 kernel over 4×4 jittered
//!    samples, a variance rectification box, lock lifetime, weighted blend
//!    (`ComputeUpsampledColorAndWeight`, `RectifyHistory`, `Accumulate`).
//!
//! Then RCAS (finish.rs). Adapted to rustudio, and simpler than the SDK in
//! these places:
//! - the input is the finished display image (8-bit, after post), so the
//!   exposure is 1 and nothing is tonemapped;
//! - motion vectors are the camera's, from depth and this and the last
//!   frame's view-projection (rustudio draws no per-object velocity); what
//!   moves on its own is left to the rectification box;
//! - the previous frame's depth is kept instead of reconstructed, and
//!   shading change compares this frame's luma with the last one's, without
//!   the SDK's luma pyramids and luma instability pass.
//!
//! The jitter comes from `escalado` (Halton 2,3, FSR's phase count).

use crate::fsr2::{Frame, phase_count};
use egui_wgpu::wgpu;

/// Studs to metres (FSR's thresholds are in metres).
const METERS_PER_STUD: f32 = 0.28;

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
    reset: f32,
    pad: f32,
};
@group(0) @binding(0) var<uniform> p: P;

const FP16_MIN: f32 = 6.10e-05;
const EPS: f32 = 6.10e-05;
const LOCK_THRESHOLD: f32 = 1.0;
const LOCK_MAX: f32 = 2.0;
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
fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}
fn rgb_to_ycocg(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(0.25 * c.r + 0.5 * c.g + 0.25 * c.b, 0.5 * c.r - 0.5 * c.b, -0.25 * c.r + 0.5 * c.g - 0.25 * c.b);
}
fn ycocg_to_rgb(c: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(c.x + c.y - c.z, c.x + c.z, c.x - c.y - c.z);
}
fn velocity_4k(mv: vec2<f32>) -> f32 {
    return length(mv * vec2<f32>(3840.0, 2160.0));
}
// FSR: motion under this many 4K pixels counts as none (depth reconstruction).
fn mv_px_threshold(near_m: f32) -> f32 {
    return mix(0.25, 0.75, clamp(near_m / 100.0, 0.0, 1.0));
}
fn view_depth(d: f32) -> f32 {
    // Infinite reverse-Z: device depth = NEAR / view depth.
    return min(p.near / max(d, 1e-7), 65504.0);
}

// ------------------------------------------------------------ 1. prepare

@group(1) @binding(0) var in_color: texture_2d<f32>;
@group(1) @binding(1) var in_depth: texture_depth_2d;
@group(1) @binding(2) var out_mv: texture_storage_2d<rg32float, write>;
@group(1) @binding(3) var out_depth: texture_storage_2d<r32float, write>;
@group(1) @binding(4) var out_luma: texture_storage_2d<r32float, write>;
@group(1) @binding(5) var out_far: texture_storage_2d<r32float, write>;

@compute @workgroup_size(8, 8)
fn prepare(@builtin(global_invocation_id) id: vec3<u32>) {
    let px = vec2<i32>(id.xy);
    let size = rs();
    if (!on_screen(px, size)) { return; }
    // Depth extents (FindDepthExtents): reverse-Z, so the nearest is the largest.
    var nearest = textureLoad(in_depth, px, 0);
    var nearest_px = px;
    var farthest = nearest;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let q = px + vec2<i32>(x, y);
            if (!on_screen(q, size)) { continue; }
            let d = textureLoad(in_depth, q, 0);
            if (d > nearest) { nearest = d; nearest_px = q; }
            farthest = min(farthest, d);
        }
    }
    // The nearest sample's motion (DilateMotionVector), from the camera:
    // where its surface point was on screen last frame, unjittered.
    let uv = (vec2<f32>(nearest_px) + 0.5) / p.render_size;
    let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
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
    textureStore(out_mv, px, vec4<f32>(mv, 0.0, 0.0));
    textureStore(out_depth, px, vec4<f32>(view_depth(nearest), 0.0, 0.0, 0.0));
    textureStore(out_far, px, vec4<f32>(view_depth(farthest) * p.meters, 0.0, 0.0, 0.0));
    let c = max(textureLoad(in_color, px, 0).rgb, vec3<f32>(0.0));
    textureStore(out_luma, px, vec4<f32>(luma(c), 0.0, 0.0, 0.0));
}

// ------------------------------------------------------------ 2. reactivity

@group(1) @binding(0) var r_mv: texture_2d<f32>;
@group(1) @binding(1) var r_depth: texture_2d<f32>;
@group(1) @binding(2) var r_prev_depth: texture_2d<f32>;
@group(1) @binding(3) var r_luma: texture_2d<f32>;
@group(1) @binding(4) var r_prev_luma: texture_2d<f32>;
@group(1) @binding(5) var r_prev_acc: texture_2d<f32>;
@group(1) @binding(6) var r_out_acc: texture_storage_2d<r32float, write>;
@group(1) @binding(7) var r_out_masks: texture_storage_2d<rgba16float, write>;
@group(1) @binding(8) var r_out_locks: texture_storage_2d<r32float, write>;

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
fn load_bilinear(t: texture_2d<f32>, uv: vec2<f32>) -> f32 {
    let size = rs();
    let b = bilinear(uv, p.render_size);
    var s = 0.0;
    for (var i = 0; i < 4; i++) {
        s += textureLoad(t, clamp_px(b.base + corner(i), size), 0).r * b.w[i];
    }
    return s;
}

// ComputeDisocclusions: the surface here was hidden last frame when it is
// farther than what the last frame had there, by more than Ksep x the half
// viewport (in pixels) x the depth.
fn disocclusion(uv: vec2<f32>, mv_in: vec2<f32>, cur_z: f32) -> f32 {
    let near_m = cur_z * p.meters;
    let mv = mv_in * f32(velocity_4k(mv_in) > mv_px_threshold(near_m));
    let b = bilinear(uv + mv, p.render_size);
    let size = rs();
    var dis = 0.0;
    var wsum = 0.0;
    var potential = true;
    for (var i = 0; i < 4; i++) {
        if (!potential) { break; }
        let q = clamp_px(b.base + corner(i), size);
        let w = b.w[i];
        if (w > EPS * 10.0) {
            let prev_z = textureLoad(r_prev_depth, q, 0).r;
            let diff = cur_z - prev_z;
            potential = potential && diff > 1.175494351e-38;
            if (potential) {
                let half_viewport = length(p.render_size * 0.5);
                let required = 1.37e-05 * half_viewport * max(cur_z, prev_z);
                dis += clamp(required / diff, 0.0, 1.0) * w;
                wsum += w;
            }
        }
    }
    return select(0.0, clamp(1.0 - dis / wsum, 0.0, 1.0), potential && wsum > 0.0);
}

// ComputeThinFeatureConfidence: a one-pixel ridge (lines, wires, text)
// the history should keep instead of the jittered frame's flicker.
fn thin_feature(px: vec2<i32>) -> f32 {
    let size = rs();
    var s: array<f32, 9>;
    var offs = array<vec2<i32>, 9>(
        vec2<i32>(0, 0), vec2<i32>(-1, -1), vec2<i32>(0, -1), vec2<i32>(1, -1),
        vec2<i32>(-1, 0), vec2<i32>(1, 0), vec2<i32>(-1, 1), vec2<i32>(0, 1), vec2<i32>(1, 1));
    var lo = 3.4e38;
    var hi = 1.175494351e-38;
    for (var i = 0; i < 9; i++) {
        s[i] = textureLoad(r_luma, clamp_px(px + offs[i], size), 0).r;
        lo = min(lo, s[i]);
        hi = max(hi, s[i]);
    }
    var dis_lo = 3.4e38;
    var dis_hi = 0.0;
    var pattern = 1u;
    for (var i = 1; i < 9; i++) {
        let d = abs(s[i] - s[0]) / max(hi - lo, 1e-8);
        if (d < 0.9) {
            pattern |= 1u << u32(i);
        } else {
            dis_lo = min(dis_lo, s[i]);
            dis_hi = max(dis_hi, s[i]);
        }
    }
    let ridge = s[0] > dis_hi || s[0] < dis_lo;
    if (!ridge) { return 0.0; }
    var masks = array<u32, 4>(
        (1u << 1u) | (1u << 2u) | (1u << 4u) | 1u,
        (1u << 2u) | (1u << 3u) | (1u << 5u) | 1u,
        (1u << 4u) | (1u << 6u) | (1u << 7u) | 1u,
        (1u << 5u) | (1u << 7u) | (1u << 8u) | 1u);
    for (var i = 0; i < 4; i++) {
        if ((pattern & masks[i]) == masks[i]) { return 0.0; }
    }
    return 1.0 - lo / max(hi, 1e-8);
}

@compute @workgroup_size(8, 8)
fn reactivity(@builtin(global_invocation_id) id: vec3<u32>) {
    let px = vec2<i32>(id.xy);
    let size = rs();
    if (!on_screen(px, size)) { return; }
    let uv = (vec2<f32>(px) + 0.5) / p.render_size;
    let mv = textureLoad(r_mv, px, 0).xy;
    let cur_z = textureLoad(r_depth, px, 0).r;
    let reset = p.reset > 0.5;
    let reproj = uv + mv;
    let inside = uv_inside(reproj) && !reset;
    let dis = select(0.0, disocclusion(uv, mv, cur_z), inside);
    // Shading change: this frame's luma (3x3, so the jitter's edge flicker
    // does not count) against the last frame's where this surface was.
    var shading = 0.0;
    if (inside) {
        var cur = 0.0;
        for (var y = -1; y <= 1; y++) {
            for (var x = -1; x <= 1; x++) {
                cur += textureLoad(r_luma, clamp_px(px + vec2<i32>(x, y), size), 0).r;
            }
        }
        cur /= 9.0;
        let prev = load_bilinear(r_prev_luma, reproj);
        let rel = abs(cur - prev) / max(max(cur, prev), 0.05);
        shading = clamp((rel - 0.12) * 2.5, 0.0, 1.0) * (1.0 - dis);
    }
    // UpdateAccumulation.
    var acc = 0.0;
    if (inside) {
        acc = clamp(load_bilinear(r_prev_acc, reproj), 0.0, 1.0);
    }
    acc = mix(acc, 0.0, shading);
    acc = mix(acc, min(-1.0 / 3.0, acc), dis);
    acc *= f32(round(acc * 100.0) > 1.0);
    textureStore(r_out_acc, px, vec4<f32>(clamp(acc + 1.0 / 3.0, 0.0, 1.0), 0.0, 0.0, 0.0));
    textureStore(r_out_masks, px, vec4<f32>(0.0, dis, shading, acc));
    // New locks, at the output pixel this sample lands on.
    let lock = thin_feature(px);
    if (lock > 0.01) {
        let hr = vec2<i32>(floor((vec2<f32>(px) + 0.5 - p.jitter) / p.render_size * p.out_size));
        if (on_screen(hr, os())) {
            textureStore(r_out_locks, hr, vec4<f32>(lock, 0.0, 0.0, 0.0));
        }
    }
}

// ------------------------------------------------------------ 3. accumulate

@group(1) @binding(0) var a_color: texture_2d<f32>;
@group(1) @binding(1) var a_mv: texture_2d<f32>;
@group(1) @binding(2) var a_masks: texture_2d<f32>;
@group(1) @binding(3) var a_far: texture_2d<f32>;
@group(1) @binding(4) var a_history: texture_2d<f32>;
@group(1) @binding(5) var a_locks: texture_2d<f32>;
@group(1) @binding(6) var a_out_history: texture_storage_2d<rgba16float, write>;
@group(1) @binding(7) var a_out: texture_storage_2d<rgba8unorm, write>;
@group(1) @binding(8) var a_sampler: sampler;

fn lanczos2(x0: f32) -> f32 {
    let x = min(abs(x0), 2.0);
    if (x < EPS) { return 1.0; }
    return (sin(PI * x) / (PI * x)) * (sin(0.5 * PI * x) / (0.5 * PI * x));
}

// The history at uv, Catmull-Rom bicubic over 4x4 texels.
fn history_sample(uv: vec2<f32>) -> vec4<f32> {
    let size = os();
    let pos = uv * p.out_size - 0.5;
    let base = floor(pos);
    let f = pos - base;
    let w0 = f * (-0.5 + f * (1.0 - 0.5 * f));
    let w1 = 1.0 + f * f * (-2.5 + 1.5 * f);
    let w2 = f * (0.5 + f * (2.0 - 1.5 * f));
    let w3 = f * f * (-0.5 + 0.5 * f);
    var wx = array<f32, 4>(w0.x, w1.x, w2.x, w3.x);
    var wy = array<f32, 4>(w0.y, w1.y, w2.y, w3.y);
    var s = vec4<f32>(0.0);
    for (var y = 0; y < 4; y++) {
        for (var x = 0; x < 4; x++) {
            let q = clamp_px(vec2<i32>(base) + vec2<i32>(x - 1, y - 1), size);
            s += textureLoad(a_history, q, 0) * (wx[x] * wy[y]);
        }
    }
    return max(s, vec4<f32>(0.0));
}

@compute @workgroup_size(8, 8)
fn accumulate(@builtin(global_invocation_id) id: vec3<u32>) {
    let hr = vec2<i32>(id.xy);
    let out_size = os();
    if (!on_screen(hr, out_size)) { return; }
    let size = rs();
    let downscale = p.render_size / p.out_size;
    // InitPassData.
    let hr_uv = (vec2<f32>(hr) + 0.5) / p.out_size;
    let lr_uv_j = hr_uv + p.jitter / p.render_size;
    let lr_px = clamp_px(vec2<i32>(hr_uv * p.render_size), size);
    let mv = textureLoad(a_mv, lr_px, 0).xy;
    let vel4k = velocity_4k(mv);
    let reproj = hr_uv + mv;
    let existing = uv_inside(reproj);
    let farthest_m = textureLoad(a_far, clamp_px(vec2<i32>(lr_uv_j * p.render_size), size), 0).r;
    let is_new = !existing || p.frame_index < 0.5 || p.reset > 0.5;
    let lr_uv_hw = clamp(lr_uv_j * p.render_size, vec2<f32>(0.5), p.render_size - 0.5) / p.render_size;
    let masks = textureSampleLevel(a_masks, a_sampler, lr_uv_hw, 0.0);
    let reactive = clamp(masks.x, 0.0, 1.0);
    let dis = clamp(masks.y, 0.0, 1.0);
    let shading = clamp(masks.z, 0.0, 1.0);
    var acc = clamp(masks.w, 0.0, 1.0);
    acc *= f32(round(acc * 100.0) > 1.0);

    // ReprojectHistoryColor.
    var hist = vec3<f32>(0.0);
    var lock = 0.0;
    if (existing && !is_new) {
        let h = history_sample(reproj);
        hist = rgb_to_ycocg(h.rgb);
        lock = h.w;
    }
    // UpdateLockStatus.
    lock *= f32(!is_new);
    let life_dec = max(shading, max(reactive, dis));
    lock = max(0.0, lock - life_dec * LOCK_MAX);
    let lock_contrib = clamp(clamp(lock - LOCK_THRESHOLD, 0.0, 1.0) * (LOCK_MAX - LOCK_THRESHOLD), 0.0, 1.0);
    let new_lock = textureLoad(a_locks, hr, 0).r * (1.0 - reactive);
    lock = max(0.0, min(lock + new_lock, LOCK_MAX));
    lock = max(0.0, lock - (0.1 / max(p.phases, 1.0)) * (1.0 - life_dec));
    lock *= f32(uv_inside(hr_uv - mv));
    // ComputeBaseAccumulationWeight.
    var hist_w = acc;
    hist_w = min(hist_w, mix(hist_w, 0.15, clamp(max(0.0, vel4k / 0.5), 0.0, 1.0)));

    // ComputeUpsampledColorAndWeight: Lanczos-2 over the 4x4 jittered samples
    // around this output pixel, and their rectification box.
    let src_out = (vec2<f32>(hr) + 0.5) * downscale;
    let src_in = vec2<i32>(floor(src_out));
    let src_unj = vec2<f32>(src_in) + 0.5 - p.jitter;
    let base_off = src_unj - src_out;
    let tl = vec2<i32>(select(-1, -2, src_unj.x > src_out.x), select(-1, -2, src_unj.y > src_out.y));
    let kernel_bias = min(1.99, 1.0 / downscale.x);
    var up = vec3<f32>(0.0);
    var up_w = 0.0;
    var box_min = vec3<f32>(0.0);
    var box_max = vec3<f32>(0.0);
    var box_center = vec3<f32>(0.0);
    var box_vec = vec3<f32>(0.0);
    var box_w = 0.0;
    for (var row = 0; row < 4; row++) {
        for (var col = 0; col < 4; col++) {
            let o = tl + vec2<i32>(col, row);
            let q = src_in + o;
            let onscreen = f32(on_screen(q, size));
            let s = rgb_to_ycocg(max(textureLoad(a_color, clamp_px(q, size), 0).rgb, vec3<f32>(0.0)));
            let off = base_off + vec2<f32>(o);
            let w = onscreen * lanczos2(length(off * kernel_bias));
            up += s * w;
            up_w += w;
            let bw = exp(-2.3 * dot(off, off)) * onscreen;
            if (row == 0 && col == 0) {
                box_min = s; box_max = s;
                box_center = s * bw; box_vec = s * s * bw; box_w = bw;
            } else {
                box_min = min(box_min, s); box_max = max(box_max, s);
                box_center += s * bw; box_vec += s * s * bw; box_w += bw;
            }
        }
    }
    box_w = select(1.0, box_w, abs(box_w) > 1.175494351e-38);
    box_center /= box_w;
    box_vec = sqrt(abs(box_vec / box_w - box_center * box_center));
    up_w *= f32(up_w > EPS);
    if (up_w > EPS) {
        up /= up_w;
        up_w *= 0.74 / 16.0;
        up = clamp(up, box_min, box_max);
    }
    if (acc == 0.0) {
        up = box_center;
        up_w = 1.0;
        hist_w = 0.0;
    }

    // RectifyHistory.
    let vel_f = clamp(vel4k / 20.0, 0.0, 1.0);
    let dist_f = clamp(0.75 - farthest_m / 20.0, 0.0, 1.0);
    let box_t = max(vel_f, max(dist_f, max(1.0 - acc, max(sqrt(reactive), shading))));
    let box_scale = mix(3.0, 1.0, box_t);
    let scaled = box_vec * vec3<f32>(1.7, 1.0, 1.0) * box_scale;
    let clamped_scaled = max(scaled, vec3<f32>(1.193e-7));
    let t = (hist - box_center) / clamped_scaled;
    if (length(t) > 1.0) {
        let clipped = normalize(t) * scaled + box_center;
        let contribution = lock_contrib * acc * (1.0 - dis);
        hist = mix(clipped, hist, clamp(contribution, 0.0, 1.0));
    }

    // Accumulate.
    hist_w *= f32(hist_w > FP16_MIN);
    hist_w = max(EPS, hist_w + up_w);
    let alpha = clamp(up_w / hist_w, 0.0, 1.0);
    let rgb = max(ycocg_to_rgb(mix(hist, up, alpha)), vec3<f32>(0.0));
    textureStore(a_out_history, hr, vec4<f32>(rgb, lock));
    textureStore(a_out, hr, vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0));
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
    mv: Tex,
    far: Tex,
    masks: Tex,
    /// Ping-ponged: this frame's and the last one's.
    depth: [Tex; 2],
    luma: [Tex; 2],
    acc: [Tex; 2],
    history: [Tex; 2],
    locks: Tex,
    out: Tex,
}

pub struct Fsr3 {
    globals_layout: wgpu::BindGroupLayout,
    prepare_layout: wgpu::BindGroupLayout,
    reactivity_layout: wgpu::BindGroupLayout,
    accumulate_layout: wgpu::BindGroupLayout,
    prepare: wgpu::ComputePipeline,
    reactivity: wgpu::ComputePipeline,
    accumulate: wgpu::ComputePipeline,
    params: wgpu::Buffer,
    sampler: wgpu::Sampler,
    targets: Option<Targets>,
    frame_index: u32,
    last: Option<Frame>,
}

impl Fsr3 {
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
        use wgpu::TextureFormat as F;
        let layout = |label, entries: &[E]| device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some(label), entries });
        let globals_layout = layout(
            "fsr3 params",
            &[E {
                binding: 0,
                visibility: S::COMPUTE,
                ty: T::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        );
        let prepare_layout = layout(
            "fsr3 prepare",
            &[
                tex(0, unfilterable),
                tex(1, wgpu::TextureSampleType::Depth),
                store(2, F::Rg32Float),
                store(3, F::R32Float),
                store(4, F::R32Float),
                store(5, F::R32Float),
            ],
        );
        let reactivity_layout = layout(
            "fsr3 reactivity",
            &[
                tex(0, unfilterable),
                tex(1, unfilterable),
                tex(2, unfilterable),
                tex(3, unfilterable),
                tex(4, unfilterable),
                tex(5, unfilterable),
                store(6, F::R32Float),
                store(7, F::Rgba16Float),
                store(8, F::R32Float),
            ],
        );
        let accumulate_layout = layout(
            "fsr3 accumulate",
            &[
                tex(0, unfilterable),
                tex(1, unfilterable),
                tex(2, filterable),
                tex(3, unfilterable),
                tex(4, unfilterable),
                tex(5, unfilterable),
                store(6, F::Rgba16Float),
                store(7, F::Rgba8Unorm),
                E { binding: 8, visibility: S::COMPUTE, ty: T::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        );
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fsr3"),
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
        let prepare = pipeline("prepare", &prepare_layout);
        let reactivity = pipeline("reactivity", &reactivity_layout);
        let accumulate = pipeline("accumulate", &accumulate_layout);
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fsr3 params"),
            size: 176,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("fsr3"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            globals_layout,
            prepare_layout,
            reactivity_layout,
            accumulate_layout,
            prepare,
            reactivity,
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
            self.targets = Some(Targets {
                render_size,
                out_size,
                mv: texture(device, "fsr3 motion", r, F::Rg32Float, false),
                far: texture(device, "fsr3 farthest", r, F::R32Float, false),
                masks: texture(device, "fsr3 masks", r, F::Rgba16Float, false),
                depth: [texture(device, "fsr3 depth", r, F::R32Float, false), texture(device, "fsr3 depth", r, F::R32Float, false)],
                luma: [texture(device, "fsr3 luma", r, F::R32Float, false), texture(device, "fsr3 luma", r, F::R32Float, false)],
                acc: [texture(device, "fsr3 accumulation", r, F::R32Float, false), texture(device, "fsr3 accumulation", r, F::R32Float, false)],
                history: [texture(device, "fsr3 history", o, F::Rgba16Float, false), texture(device, "fsr3 history", o, F::Rgba16Float, false)],
                locks: texture(device, "fsr3 new locks", o, F::R32Float, true),
                out: texture(device, "fsr3 output", o, F::Rgba8Unorm, false),
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
        let prev_vp = self.last.map_or(frame.view_proj, |l| l.view_proj);
        let mut params = [0f32; 44];
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
        params[42] = if reset { 1.0 } else { 0.0 };
        queue.write_buffer(&self.params, 0, bytemuck::cast_slice(&params));

        let t = self.targets.as_ref().expect("targets");
        let cur = (self.frame_index & 1) as usize;
        let prev = 1 - cur;
        let group = |layout: &wgpu::BindGroupLayout, views: &[&wgpu::TextureView], sampler: Option<&wgpu::Sampler>| {
            let mut entries: Vec<wgpu::BindGroupEntry> = views
                .iter()
                .enumerate()
                .map(|(i, v)| wgpu::BindGroupEntry { binding: i as u32, resource: wgpu::BindingResource::TextureView(v) })
                .collect();
            if let Some(s) = sampler {
                entries.push(wgpu::BindGroupEntry { binding: views.len() as u32, resource: wgpu::BindingResource::Sampler(s) });
            }
            device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("fsr3"), layout, entries: &entries })
        };
        let globals = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fsr3 params"),
            layout: &self.globals_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: self.params.as_entire_binding() }],
        });
        let prepare_bg = group(&self.prepare_layout, &[color, depth, &t.mv.view, &t.depth[cur].view, &t.luma[cur].view, &t.far.view], None);
        let reactivity_bg = group(
            &self.reactivity_layout,
            &[
                &t.mv.view,
                &t.depth[cur].view,
                &t.depth[prev].view,
                &t.luma[cur].view,
                &t.luma[prev].view,
                &t.acc[prev].view,
                &t.acc[cur].view,
                &t.masks.view,
                &t.locks.view,
            ],
            None,
        );
        let accumulate_bg = group(
            &self.accumulate_layout,
            &[color, &t.mv.view, &t.masks.view, &t.far.view, &t.history[prev].view, &t.locks.view, &t.history[cur].view, &t.out.view],
            Some(&self.sampler),
        );
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("fsr3") });
        // The new locks start at zero each frame.
        drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fsr3 clear locks"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &t.locks.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        }));
        let groups = |n: u32| n.div_ceil(8);
        for (pipeline, bg, size) in [
            (&self.prepare, &prepare_bg, render_size),
            (&self.reactivity, &reactivity_bg, render_size),
            (&self.accumulate, &accumulate_bg, out_size),
        ] {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("fsr3"), timestamp_writes: None });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &globals, &[]);
            pass.set_bind_group(1, bg, &[]);
            pass.dispatch_workgroups(groups(size[0]), groups(size[1]), 1);
        }
        queue.submit([encoder.finish()]);
        self.last = Some(frame);
        self.frame_index = self.frame_index.wrapping_add(1).max(1);
        &self.targets.as_ref().expect("targets").out.view
    }
}
