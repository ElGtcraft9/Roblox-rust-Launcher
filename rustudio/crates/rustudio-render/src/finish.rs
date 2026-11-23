//! (RustLauncher) The view's last passes, after everything else has drawn
//! into the display image (post, world GUIs, effects, highlights):
//!
//! - **FXAA** in place of multisampling on a weak GPU. 4× MSAA multiplied
//!   the raster, the see-through encode and decode (both per sample) and the
//!   resolve: on a Vega 8 it cost a quarter of the frame at quality level 18.
//! - **FSR 1 upscaling** (EASU, then RCAS sharpening) when the 3D view is
//!   drawn below its on-screen size (Performance > 3D resolution, or
//!   Automatic on a weak GPU), instead of egui's bilinear stretch.
//!
//! EASU and RCAS follow AMD FidelityFX Super Resolution 1.0 (MIT licence):
//! the edge-adaptive Lanczos-2-like upscale with its direction and length
//! analysis over four bilinear quadrants and its deringing clamp, and the
//! robust contrast-adaptive sharpen. FXAA follows the published FXAA 3.11
//! quality algorithm (edge walk along the luma edge, sub-pixel blend).
//! Both are written here from those algorithms for wgpu.
//!
//! The display image is 8-bit and already in the display's gamma, the space
//! all three expect.

use egui_wgpu::wgpu;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// RCAS sharpness in stops (0 = sharpest; FSR suggests 0.2): a little
/// softer, so stone and concrete textures drawn at 68 % do not crawl.
const RCAS_STOPS: f32 = 0.35;

const SHADER: &str = r#"
struct Params {
    in_size: vec2<f32>,
    out_size: vec2<f32>,
    sharp: vec4<f32>,
};
@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var smp: sampler;
@group(0) @binding(2) var<uniform> P: Params;

@vertex
fn vs_full(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}

// ---------------------------------------------------------------- FXAA

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.299, 0.587, 0.114));
}
fn sample_luma(uv: vec2<f32>) -> f32 {
    return luma(textureSampleLevel(src, smp, uv, 0.0).rgb);
}

const FXAA_MIN: f32 = 0.0312;
const FXAA_MAX: f32 = 0.125;
const FXAA_SUBPIX: f32 = 0.75;

@fragment
fn fs_fxaa(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let inv = 1.0 / P.in_size;
    let uv = p.xy * inv;
    let centre = textureSampleLevel(src, smp, uv, 0.0);
    let lm = luma(centre.rgb);
    let ln = sample_luma(uv + vec2<f32>(0.0, -inv.y));
    let ls = sample_luma(uv + vec2<f32>(0.0, inv.y));
    let lw = sample_luma(uv + vec2<f32>(-inv.x, 0.0));
    let le = sample_luma(uv + vec2<f32>(inv.x, 0.0));
    let hi = max(lm, max(max(ln, ls), max(lw, le)));
    let lo = min(lm, min(min(ln, ls), min(lw, le)));
    let range = hi - lo;
    if (range < max(FXAA_MIN, hi * FXAA_MAX)) {
        return centre;
    }
    let lnw = sample_luma(uv + vec2<f32>(-inv.x, -inv.y));
    let lne = sample_luma(uv + vec2<f32>(inv.x, -inv.y));
    let lsw = sample_luma(uv + vec2<f32>(-inv.x, inv.y));
    let lse = sample_luma(uv + vec2<f32>(inv.x, inv.y));
    let edge_h = abs(-2.0 * lw + lnw + lsw) + 2.0 * abs(-2.0 * lm + ln + ls) + abs(-2.0 * le + lne + lse);
    let edge_v = abs(-2.0 * ln + lnw + lne) + 2.0 * abs(-2.0 * lm + lw + le) + abs(-2.0 * ls + lsw + lse);
    // A horizontal edge: blend across Y, walk along X.
    let horizontal = edge_h >= edge_v;
    let l1 = select(lw, ln, horizontal); // the negative side
    let l2 = select(le, ls, horizontal); // the positive side
    let g1 = abs(l1 - lm);
    let g2 = abs(l2 - lm);
    let toward1 = g1 >= g2;
    let grad = 0.25 * max(g1, g2);
    var step_len = select(inv.x, inv.y, horizontal);
    var local_avg = 0.5 * (l2 + lm);
    if (toward1) {
        step_len = -step_len;
        local_avg = 0.5 * (l1 + lm);
    }
    var on_edge = uv;
    if (horizontal) { on_edge.y += 0.5 * step_len; } else { on_edge.x += 0.5 * step_len; }
    let along = select(vec2<f32>(0.0, inv.y), vec2<f32>(inv.x, 0.0), horizontal);
    var uv1 = on_edge - along;
    var uv2 = on_edge + along;
    var end1 = sample_luma(uv1) - local_avg;
    var end2 = sample_luma(uv2) - local_avg;
    var done1 = abs(end1) >= grad;
    var done2 = abs(end2) >= grad;
    if (!done1) { uv1 -= along; }
    if (!done2) { uv2 += along; }
    var steps = array<f32, 10>(1.0, 1.0, 1.0, 1.0, 1.5, 2.0, 2.0, 2.0, 4.0, 8.0);
    for (var i = 0; i < 10; i++) {
        if (done1 && done2) { break; }
        if (!done1) { end1 = sample_luma(uv1) - local_avg; }
        if (!done2) { end2 = sample_luma(uv2) - local_avg; }
        done1 = abs(end1) >= grad;
        done2 = abs(end2) >= grad;
        if (!done1) { uv1 -= along * steps[i]; }
        if (!done2) { uv2 += along * steps[i]; }
    }
    let d1 = select(uv.y - uv1.y, uv.x - uv1.x, horizontal);
    let d2 = select(uv2.y - uv.y, uv2.x - uv.x, horizontal);
    let nearer1 = d1 < d2;
    let thickness = d1 + d2;
    let pixel_offset = 0.5 - min(d1, d2) / max(thickness, 1e-6);
    let centre_smaller = lm < local_avg;
    let good = (select(end2, end1, nearer1) < 0.0) != centre_smaller;
    var offset = select(0.0, pixel_offset, good);
    // Sub-pixel aliasing: thin features the edge walk misses.
    let avg = (2.0 * (ln + ls + lw + le) + lnw + lne + lsw + lse) / 12.0;
    let s1 = clamp(abs(avg - lm) / range, 0.0, 1.0);
    let s2 = (-2.0 * s1 + 3.0) * s1 * s1;
    offset = max(offset, s2 * s2 * FXAA_SUBPIX);
    var out_uv = uv;
    if (horizontal) { out_uv.y += offset * step_len; } else { out_uv.x += offset * step_len; }
    return vec4<f32>(textureSampleLevel(src, smp, out_uv, 0.0).rgb, 1.0);
}

// ---------------------------------------------------------------- FSR 1 EASU

fn tap(fp: vec2<i32>, off: vec2<i32>) -> vec3<f32> {
    let hi = vec2<i32>(P.in_size) - vec2<i32>(1);
    return textureLoad(src, clamp(fp + off, vec2<i32>(0), hi), 0).rgb;
}
// Luma ×2 for the analysis (FSR: B/2 + R/2 + G).
fn l2(c: vec3<f32>) -> f32 {
    return c.b * 0.5 + (c.r * 0.5 + c.g);
}

struct DirLen { dir: vec2<f32>, len: f32 };

// One bilinear quadrant's direction and edge length, weighted by w; the
// centre lc with its up la, left lb, right ld, down le.
fn easu_set(acc: DirLen, w: f32, la: f32, lb: f32, lc: f32, ld: f32, le: f32) -> DirLen {
    var r = acc;
    let dir_x = ld - lb;
    let len_x = clamp(abs(dir_x) / max(max(abs(ld - lc), abs(lc - lb)), 1e-8), 0.0, 1.0);
    r.len += len_x * len_x * w;
    r.dir.x += dir_x * w;
    let dir_y = le - la;
    let len_y = clamp(abs(dir_y) / max(max(abs(le - lc), abs(lc - la)), 1e-8), 0.0, 1.0);
    r.len += len_y * len_y * w;
    r.dir.y += dir_y * w;
    return r;
}

struct Acc { c: vec3<f32>, w: f32 };

fn easu_tap(acc: Acc, off: vec2<f32>, dir: vec2<f32>, len: vec2<f32>, lob: f32, clp: f32, c: vec3<f32>) -> Acc {
    // Rotate into the edge's frame, then stretch along / across it.
    var v = vec2<f32>(off.x * dir.x + off.y * dir.y, off.x * (-dir.y) + off.y * dir.x);
    v *= len;
    let d2 = min(dot(v, v), clp);
    // Lanczos-2 approximated: (25/16 (2/5 x² - 1)² - (25/16 - 1)) × (lob x² - 1)².
    var wb = 0.4 * d2 - 1.0;
    var wa = lob * d2 - 1.0;
    wb *= wb;
    wa *= wa;
    wb = 1.5625 * wb - 0.5625;
    let w = wb * wa;
    var r = acc;
    r.c += c * w;
    r.w += w;
    return r;
}

@fragment
fn fs_easu(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let pp0 = p.xy * (P.in_size / P.out_size) - vec2<f32>(0.5);
    let fpf = floor(pp0);
    let pp = pp0 - fpf;
    let fp = vec2<i32>(fpf);
    //    b c
    //  e f g h
    //  i j k l
    //    n o
    let b = tap(fp, vec2<i32>(0, -1));
    let c = tap(fp, vec2<i32>(1, -1));
    let e = tap(fp, vec2<i32>(-1, 0));
    let f = tap(fp, vec2<i32>(0, 0));
    let g = tap(fp, vec2<i32>(1, 0));
    let h = tap(fp, vec2<i32>(2, 0));
    let i = tap(fp, vec2<i32>(-1, 1));
    let j = tap(fp, vec2<i32>(0, 1));
    let k = tap(fp, vec2<i32>(1, 1));
    let l = tap(fp, vec2<i32>(2, 1));
    let n = tap(fp, vec2<i32>(0, 2));
    let o = tap(fp, vec2<i32>(1, 2));
    let lb = l2(b); let lc = l2(c); let le = l2(e); let lf = l2(f); let lg = l2(g); let lh = l2(h);
    let li = l2(i); let lj = l2(j); let lk = l2(k); let ll = l2(l); let ln = l2(n); let lo = l2(o);
    var dl = DirLen(vec2<f32>(0.0), 0.0);
    dl = easu_set(dl, (1.0 - pp.x) * (1.0 - pp.y), lb, le, lf, lg, lj);
    dl = easu_set(dl, pp.x * (1.0 - pp.y), lc, lf, lg, lh, lk);
    dl = easu_set(dl, (1.0 - pp.x) * pp.y, lf, li, lj, lk, ln);
    dl = easu_set(dl, pp.x * pp.y, lg, lj, lk, ll, lo);
    var dir = dl.dir;
    let dir2 = dot(dir, dir);
    if (dir2 < 1.0 / 32768.0) {
        dir = vec2<f32>(1.0, 0.0);
    } else {
        dir *= inverseSqrt(dir2);
    }
    var len = dl.len * 0.5;
    len *= len;
    // Stretch the kernel along the edge, squeeze it across.
    let stretch = 1.0 / max(abs(dir.x), abs(dir.y));
    let len2 = vec2<f32>(1.0 + (stretch - 1.0) * len, 1.0 - 0.5 * len);
    let lob = 0.5 + (0.25 - 0.04 - 0.5) * len;
    let clp = 1.0 / lob;
    var acc = Acc(vec3<f32>(0.0), 0.0);
    acc = easu_tap(acc, vec2<f32>(0.0, -1.0) - pp, dir, len2, lob, clp, b);
    acc = easu_tap(acc, vec2<f32>(1.0, -1.0) - pp, dir, len2, lob, clp, c);
    acc = easu_tap(acc, vec2<f32>(-1.0, 1.0) - pp, dir, len2, lob, clp, i);
    acc = easu_tap(acc, vec2<f32>(0.0, 1.0) - pp, dir, len2, lob, clp, j);
    acc = easu_tap(acc, vec2<f32>(0.0, 0.0) - pp, dir, len2, lob, clp, f);
    acc = easu_tap(acc, vec2<f32>(-1.0, 0.0) - pp, dir, len2, lob, clp, e);
    acc = easu_tap(acc, vec2<f32>(1.0, 1.0) - pp, dir, len2, lob, clp, k);
    acc = easu_tap(acc, vec2<f32>(2.0, 1.0) - pp, dir, len2, lob, clp, l);
    acc = easu_tap(acc, vec2<f32>(2.0, 0.0) - pp, dir, len2, lob, clp, h);
    acc = easu_tap(acc, vec2<f32>(1.0, 0.0) - pp, dir, len2, lob, clp, g);
    acc = easu_tap(acc, vec2<f32>(1.0, 2.0) - pp, dir, len2, lob, clp, o);
    acc = easu_tap(acc, vec2<f32>(0.0, 2.0) - pp, dir, len2, lob, clp, n);
    // Deringing: within the four nearest texels.
    let mn = min(min(f, g), min(j, k));
    let mx = max(max(f, g), max(j, k));
    let col = clamp(acc.c / max(acc.w, 1e-6), mn, mx);
    return vec4<f32>(col, 1.0);
}

// ---------------------------------------------------------------- FSR 1 RCAS

const RCAS_LIMIT: f32 = 0.25 - 1.0 / 16.0;

@fragment
fn fs_rcas(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let ip = vec2<i32>(p.xy);
    let hi = vec2<i32>(P.out_size) - vec2<i32>(1);
    let b = textureLoad(src, clamp(ip + vec2<i32>(0, -1), vec2<i32>(0), hi), 0).rgb;
    let d = textureLoad(src, clamp(ip + vec2<i32>(-1, 0), vec2<i32>(0), hi), 0).rgb;
    let e = textureLoad(src, ip, 0).rgb;
    let f = textureLoad(src, clamp(ip + vec2<i32>(1, 0), vec2<i32>(0), hi), 0).rgb;
    let h = textureLoad(src, clamp(ip + vec2<i32>(0, 1), vec2<i32>(0), hi), 0).rgb;
    let mn4 = min(min(b, d), min(f, h));
    let mx4 = max(max(b, d), max(f, h));
    // How far the ring can push without clipping, per channel.
    let hit_min = min(mn4, e) / max(4.0 * mx4, vec3<f32>(1e-6));
    let hit_max = (vec3<f32>(1.0) - max(mx4, e)) / min(4.0 * mn4 - vec3<f32>(4.0), vec3<f32>(-1e-6));
    let lobe_c = max(-hit_min, hit_max);
    let lobe = max(-RCAS_LIMIT, min(max(lobe_c.r, max(lobe_c.g, lobe_c.b)), 0.0)) * P.sharp.x;
    let col = (lobe * (b + d + f + h) + e) / (4.0 * lobe + 1.0);
    return vec4<f32>(clamp(col, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
"#;

struct Tex {
    _tex: wgpu::Texture,
    view: wgpu::TextureView,
}

fn texture(device: &wgpu::Device, label: &str, size: [u32; 2]) -> Tex {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: size[0].max(1), height: size[1].max(1), depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = tex.create_view(&Default::default());
    Tex { _tex: tex, view }
}

/// What the targets were made for: the source image (its generation), its
/// size, the size on screen, and whether FXAA runs.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Key {
    source: u64,
    in_size: [u32; 2],
    out_size: [u32; 2],
    fxaa: bool,
}

struct Targets {
    key: Key,
    /// FXAA's output (at the render size), when it runs.
    fxaa: Option<(Tex, wgpu::BindGroup)>,
    /// EASU's output and RCAS's (at the on-screen size), when upscaling.
    upscale: Option<(Tex, wgpu::BindGroup, Tex, wgpu::BindGroup)>,
    /// The bind group reading the display image (FXAA's or EASU's input).
    from_display: wgpu::BindGroup,
}

pub struct Finish {
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    params: wgpu::Buffer,
    fxaa: wgpu::RenderPipeline,
    easu: wgpu::RenderPipeline,
    rcas: wgpu::RenderPipeline,
    targets: Option<Targets>,
    /// RCAS alone over an image already upscaled (FSR 2's): its output.
    sharpened: Option<([u32; 2], Tex)>,
    /// Bumped each time the image shown changes texture.
    generation: u64,
}

impl Finish {
    pub fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("finish"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("finish"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("finish params"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("finish"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("finish"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_full"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(entry),
                    targets: &[Some(wgpu::ColorTargetState { format: FORMAT, blend: None, write_mask: wgpu::ColorWrites::ALL })],
                    compilation_options: Default::default(),
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        Self {
            fxaa: pipeline("fs_fxaa"),
            easu: pipeline("fs_easu"),
            rcas: pipeline("fs_rcas"),
            layout,
            sampler,
            params,
            targets: None,
            sharpened: None,
            generation: 0,
        }
    }

    fn group(&self, device: &wgpu::Device, view: &wgpu::TextureView) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("finish"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: self.params.as_entire_binding() },
            ],
        })
    }

    /// (RustLauncher) RCAS over `input` (`size`, already at the size on
    /// screen: FSR 2's output). Returns the view to show and its generation.
    pub fn sharpen(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, input: &wgpu::TextureView, size: [u32; 2]) -> (&wgpu::TextureView, u64) {
        self.targets = None;
        if self.sharpened.as_ref().is_none_or(|(s, _)| *s != size) {
            self.sharpened = Some((size, texture(device, "fsr2 rcas", size)));
            self.generation += 1;
        }
        let sharp = 2f32.powf(-RCAS_STOPS);
        let params: [f32; 8] = [size[0] as f32, size[1] as f32, size[0] as f32, size[1] as f32, sharp, 0.0, 0.0, 0.0];
        queue.write_buffer(&self.params, 0, bytemuck::cast_slice(&params));
        let input_group = self.group(device, input);
        let (_, out) = self.sharpened.as_ref().expect("sharpened");
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("fsr2 rcas") });
        {
            let mut p = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("fsr2 rcas"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &out.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            p.set_pipeline(&self.rcas);
            p.set_bind_group(0, &input_group, &[]);
            p.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
        (&out.view, self.generation)
    }

    /// The last image `sharpen` produced (what the view showed).
    pub fn last_sharpened(&self) -> Option<&wgpu::TextureView> {
        self.sharpened.as_ref().map(|(_, t)| &t.view)
    }

    /// Runs FXAA and/or the upscale over `display` (`source` changes when
    /// that texture is recreated). Returns the view to show and its
    /// generation, or `None` when the display image is shown as it is.
    pub fn apply(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        display: &wgpu::TextureView,
        source: u64,
        in_size: [u32; 2],
        out_size: [u32; 2],
        fxaa: bool,
    ) -> Option<(&wgpu::TextureView, u64)> {
        self.sharpened = None;
        let upscale = out_size[0] > in_size[0] || out_size[1] > in_size[1];
        if !fxaa && !upscale {
            self.targets = None;
            return None;
        }
        let key = Key { source, in_size, out_size, fxaa };
        if self.targets.as_ref().is_none_or(|t| t.key != key) {
            let fxaa_t = fxaa.then(|| {
                let t = texture(device, "fxaa", in_size);
                let g = self.group(device, &t.view);
                (t, g)
            });
            let up = upscale.then(|| {
                let easu = texture(device, "fsr easu", out_size);
                let easu_g = self.group(device, &easu.view);
                let rcas = texture(device, "fsr rcas", out_size);
                let rcas_g = self.group(device, &rcas.view);
                (easu, easu_g, rcas, rcas_g)
            });
            let from_display = self.group(device, display);
            self.targets = Some(Targets { key, fxaa: fxaa_t, upscale: up, from_display });
            self.generation += 1;
        }
        let t = self.targets.as_ref()?;
        let sharp = 2f32.powf(-RCAS_STOPS);
        let params: [f32; 8] = [
            in_size[0] as f32,
            in_size[1] as f32,
            out_size[0] as f32,
            out_size[1] as f32,
            sharp,
            0.0,
            0.0,
            0.0,
        ];
        queue.write_buffer(&self.params, 0, bytemuck::cast_slice(&params));
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("finish") });
        let mut pass = |pipeline: &wgpu::RenderPipeline, input: &wgpu::BindGroup, output: &wgpu::TextureView| {
            let mut p = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("finish"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            p.set_pipeline(pipeline);
            p.set_bind_group(0, input, &[]);
            p.draw(0..3, 0..1);
        };
        let mut input = &t.from_display;
        if let Some((out, group)) = &t.fxaa {
            pass(&self.fxaa, input, &out.view);
            input = group;
        }
        if let Some((easu, easu_g, rcas, _)) = &t.upscale {
            pass(&self.easu, input, &easu.view);
            pass(&self.rcas, easu_g, &rcas.view);
        }
        drop(pass);
        queue.submit([encoder.finish()]);
        let shown = match (&t.upscale, &t.fxaa) {
            (Some((_, _, rcas, _)), _) => &rcas.view,
            (None, Some((out, _))) => &out.view,
            (None, None) => return None,
        };
        Some((shown, self.generation))
    }
}
