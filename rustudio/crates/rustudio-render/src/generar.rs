//! (RustLauncher) Frame generation for the editor view (experimental, off
//! by default): a frame between two drawn ones, made by reprojecting the
//! last finished image to the camera's pose now, with the last frame's depth.
//! The scene is not drawn that turn, so the GPU does about half the work.
//!
//! 1. **splat** (render size): the last depth's texels are moved into the new
//!    view (reverse-Z, atomic max keeps the nearest at each pixel);
//! 2. **warp** (output size): each pixel takes that depth (holes: the
//!    farthest neighbour in 5×5, what a disocclusion reveals), goes back to
//!    the last view and reads the last image there.
//!
//! Only the camera's motion is known (rustudio draws no per-object
//! velocity): what moves on its own stays where it was for that frame.

use egui_wgpu::wgpu;
use glam::Mat4;

const SHADER: &str = r#"
struct G {
    vp_now: mat4x4<f32>,
    inv_vp_now: mat4x4<f32>,
    vp_last: mat4x4<f32>,
    inv_vp_last_jit: mat4x4<f32>,
    render_size: vec2<f32>,
    out_size: vec2<f32>,
};
@group(0) @binding(0) var<uniform> g: G;
@group(0) @binding(1) var last_depth: texture_depth_2d;
@group(0) @binding(2) var last_color: texture_2d<f32>;
@group(0) @binding(3) var<storage, read_write> splat: array<atomic<u32>>;
@group(0) @binding(4) var out_img: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(5) var samp: sampler;

fn rs() -> vec2<i32> { return vec2<i32>(g.render_size); }

@compute @workgroup_size(8, 8)
fn splat_pass(@builtin(global_invocation_id) id: vec3<u32>) {
    let px = vec2<i32>(id.xy);
    let size = rs();
    if (any(px >= size)) { return; }
    let d = textureLoad(last_depth, px, 0);
    if (d <= 1e-7) { return; }
    let uv = (vec2<f32>(px) + 0.5) / g.render_size;
    let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let h = g.inv_vp_last_jit * vec4<f32>(ndc, d, 1.0);
    let world = h.xyz / h.w;
    let c = g.vp_now * vec4<f32>(world, 1.0);
    if (c.w <= 1e-6) { return; }
    let n = c.xy / c.w;
    let q = vec2<i32>(floor(vec2<f32>(n.x * 0.5 + 0.5, 0.5 - n.y * 0.5) * g.render_size));
    if (any(q < vec2<i32>(0)) || any(q >= size)) { return; }
    atomicMax(&splat[q.y * size.x + q.x], bitcast<u32>(c.z / c.w));
}

fn splat_at(q: vec2<i32>) -> f32 {
    let size = rs();
    let c = clamp(q, vec2<i32>(0), size - vec2<i32>(1));
    return bitcast<f32>(atomicLoad(&splat[c.y * size.x + c.x]));
}

@compute @workgroup_size(8, 8)
fn warp_pass(@builtin(global_invocation_id) id: vec3<u32>) {
    let o = vec2<i32>(id.xy);
    if (any(o >= vec2<i32>(g.out_size))) { return; }
    let uv = (vec2<f32>(o) + 0.5) / g.out_size;
    let rpx = vec2<i32>(uv * g.render_size);
    var d = splat_at(rpx);
    if (d <= 1e-7) {
        // A hole: the farthest (smallest) non-zero depth around.
        var far = 1e30;
        for (var y = -2; y <= 2; y++) {
            for (var x = -2; x <= 2; x++) {
                let v = splat_at(rpx + vec2<i32>(x, y));
                if (v > 1e-7) { far = min(far, v); }
            }
        }
        d = select(1e-7, far, far < 1e29);
    }
    let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let h = g.inv_vp_now * vec4<f32>(ndc, d, 1.0);
    let world = h.xyz / h.w;
    let p = g.vp_last * vec4<f32>(world, 1.0);
    var src = uv;
    if (p.w > 1e-6) {
        let n = p.xy / p.w;
        src = vec2<f32>(n.x * 0.5 + 0.5, 0.5 - n.y * 0.5);
    }
    let col = textureSampleLevel(last_color, samp, clamp(src, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
    textureStore(out_img, o, vec4<f32>(col.rgb, 1.0));
}
"#;

struct Targets {
    render_size: [u32; 2],
    out_size: [u32; 2],
    splat: wgpu::Buffer,
    out_tex: wgpu::Texture,
    pub out: wgpu::TextureView,
}

pub struct Generar {
    layout: wgpu::BindGroupLayout,
    splat: wgpu::ComputePipeline,
    warp: wgpu::ComputePipeline,
    params: wgpu::Buffer,
    sampler: wgpu::Sampler,
    targets: Option<Targets>,
    /// Bumped when `out` is recreated, so the caller can re-register it.
    pub generation: u64,
}

impl Generar {
    pub fn new(device: &wgpu::Device) -> Self {
        use wgpu::{BindGroupLayoutEntry as E, BindingType as T, ShaderStages as S};
        let entry = |binding, ty| E { binding, visibility: S::COMPUTE, ty, count: None };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("generar"),
            entries: &[
                entry(0, T::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }),
                entry(1, T::Texture { sample_type: wgpu::TextureSampleType::Depth, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }),
                entry(2, T::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }),
                entry(3, T::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: false }, has_dynamic_offset: false, min_binding_size: None }),
                entry(4, T::StorageTexture { access: wgpu::StorageTextureAccess::WriteOnly, format: wgpu::TextureFormat::Rgba8Unorm, view_dimension: wgpu::TextureViewDimension::D2 }),
                entry(5, T::Sampler(wgpu::SamplerBindingType::Filtering)),
            ],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("generar"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("generar"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let pipeline = |entry: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&pl),
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        Self {
            splat: pipeline("splat_pass"),
            warp: pipeline("warp_pass"),
            params: device.create_buffer(&wgpu::BufferDescriptor { label: Some("generar"), size: 288, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false }),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor { label: Some("generar"), mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default() }),
            layout,
            targets: None,
            generation: 0,
        }
    }

    /// The generated frame: the last finished image `last_color` and its
    /// depth `last_depth` (both from the last drawn frame, whose matrices
    /// are `vp_last` (no jitter) and `inv_vp_last_jit`) warped to the view
    /// `vp_now` / `inv_vp_now`. Returns the result's view, and whether it is
    /// a new texture (to register again).
    #[allow(clippy::too_many_arguments)]
    pub fn run(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        last_color: &wgpu::TextureView,
        last_depth: &wgpu::TextureView,
        render_size: [u32; 2],
        out_size: [u32; 2],
        vp_now: Mat4,
        inv_vp_now: Mat4,
        vp_last: Mat4,
        inv_vp_last_jit: Mat4,
    ) -> (&wgpu::TextureView, bool) {
        let recreated = self.targets.as_ref().is_none_or(|t| t.render_size != render_size || t.out_size != out_size);
        if self.targets.as_ref().is_none_or(|t| t.render_size != render_size || t.out_size != out_size) {
            let out_tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("generar out"),
                size: wgpu::Extent3d { width: out_size[0].max(1), height: out_size[1].max(1), depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let out = out_tex.create_view(&Default::default());
            self.targets = Some(Targets {
                render_size,
                out_size,
                splat: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("generar splat"),
                    size: render_size[0].max(1) as u64 * render_size[1].max(1) as u64 * 4,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                out_tex,
                out,
            });
            self.generation += 1;
        }
        let t = self.targets.as_ref().expect("targets");
        let mut p = [0f32; 72];
        p[0..16].copy_from_slice(&vp_now.to_cols_array());
        p[16..32].copy_from_slice(&inv_vp_now.to_cols_array());
        p[32..48].copy_from_slice(&vp_last.to_cols_array());
        p[48..64].copy_from_slice(&inv_vp_last_jit.to_cols_array());
        p[64] = render_size[0] as f32;
        p[65] = render_size[1] as f32;
        p[66] = out_size[0] as f32;
        p[67] = out_size[1] as f32;
        queue.write_buffer(&self.params, 0, bytemuck::cast_slice(&p));
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("generar"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.params.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(last_depth) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(last_color) },
                wgpu::BindGroupEntry { binding: 3, resource: t.splat.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&t.out) },
                wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("generar") });
        encoder.clear_buffer(&t.splat, 0, None);
        let groups = |n: u32| n.div_ceil(8);
        for (pipeline, size) in [(&self.splat, render_size), (&self.warp, out_size)] {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("generar"), timestamp_writes: None });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups(groups(size[0]), groups(size[1]), 1);
        }
        queue.submit([encoder.finish()]);
        (&self.targets.as_ref().expect("targets").out, recreated)
    }
}
