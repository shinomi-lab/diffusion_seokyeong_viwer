//! wgpu による描画: ノード（インスタンス描画。1 回の draw で全ノード）と辺（LineList）。
//! egui とは独立に作ってあり、オフスクリーンでピクセルを読んで位置を検証できる（`tests`）。

use std::borrow::Cow;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use eframe::wgpu;
use wgpu::util::DeviceExt;

use crate::camera::{Camera, NdcTransform};
use crate::tokens::Tokens;

const COMMON: &str = include_str!("common.wgsl");
const NODES: &str = include_str!("nodes.wgsl");
const EDGES: &str = include_str!("edges.wgsl");

/// ユニフォーム（WGSL の `struct U` と同じ配置）。
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Uniforms {
    scale: [f32; 2],
    offset: [f32; 2],
    view: [f32; 2],
    radius: f32,
    halo: f32,
    blue: [f32; 4],
    green: [f32; 4],
    bg: [f32; 4],
    pulse_color: [f32; 4],
    edge_color: [f32; 4],
    fill: [[f32; 4]; 6],
    stroke: [[f32; 4]; 6],
    kind: [[f32; 4]; 6],
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

impl Uniforms {
    /// 辺が多いほど薄くする（重なって黒くつぶれないように）。
    pub fn with_edge_count(mut self, n_edges: usize) -> Self {
        let a = (0.16 * (3000.0 / n_edges.max(1) as f32).sqrt()).clamp(0.03, 0.16);
        self.edge_color[3] = a;
        self
    }

    /// 描くノード数に応じた半径（px。0 以下なら変えない）。
    pub fn with_radius(mut self, r: f32) -> Self {
        if r > 0.0 {
            self.radius = r;
        }
        self
    }

    /// `srgb_target`: 描画先が sRGB フォーマットのとき、色を線形に直して渡す。
    pub fn new(t: &Tokens, cam: &Camera, view: [f32; 2], bg: [f32; 3], srgb_target: bool) -> Self {
        let xf: NdcTransform = cam.ndc_transform(view);
        let col = |c: [f32; 3], a: f32| -> [f32; 4] {
            if srgb_target {
                [
                    srgb_to_linear(c[0]),
                    srgb_to_linear(c[1]),
                    srgb_to_linear(c[2]),
                    a,
                ]
            } else {
                [c[0], c[1], c[2], a]
            }
        };
        let mut fill = [[0.0; 4]; 6];
        let mut stroke = [[0.0; 4]; 6];
        let mut kind = [[1.0, 0.0, 0.0, 0.0]; 6];
        for (i, k) in t.kinds.iter().enumerate().take(6) {
            fill[i] = col(k.fill.unwrap_or([0.0; 3]), 1.0);
            stroke[i] = col(k.stroke.unwrap_or([0.0; 3]), 1.0);
            kind[i] = [
                k.radius_scale,
                if k.stroke.is_some() && k.stroke_width > 0.0 {
                    k.stroke_width / (t.radius_px * k.radius_scale)
                } else {
                    0.0
                },
                0.0,
                0.0,
            ];
        }
        let edge = col(t.edge, 0.16);
        Self {
            scale: xf.scale,
            offset: xf.offset,
            view,
            radius: t.radius_px,
            halo: t.halo_scale,
            blue: col(t.split[0], 1.0),
            green: col(t.split[1], 1.0),
            bg: col(bg, 1.0),
            pulse_color: col(t.wave, 1.0),
            edge_color: edge,
            fill,
            stroke,
            kind,
        }
    }
}

/// 1 フレームぶんの描画データ。`*_ver` が変わったときだけ GPU のバッファを更新する。
#[derive(Clone)]
pub struct Frame {
    pub uniforms: Uniforms,
    pub pos: Arc<Vec<f32>>,
    pub pos_ver: u64,
    /// kind（下位 8 ビット）| pulse << 8
    pub attrs: Arc<Vec<u32>>,
    pub attr_ver: u64,
    pub edges: Arc<Vec<f32>>,
    pub edge_ver: u64,
}

/// 比較（F13）の A・B で別々のバッファを持つ描画器の組。`egui_wgpu` の `callback_resources` に入れる。
pub struct Renderers(pub Vec<Renderer>);

pub struct Renderer {
    node_pipe: wgpu::RenderPipeline,
    edge_pipe: wgpu::RenderPipeline,
    uniform_buf: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    pos_buf: wgpu::Buffer,
    attr_buf: wgpu::Buffer,
    edge_buf: wgpu::Buffer,
    cap_nodes: usize,
    cap_edge_floats: usize,
    n_nodes: u32,
    n_edge_verts: u32,
    pos_ver: u64,
    attr_ver: u64,
    edge_ver: u64,
}

fn buffer(
    device: &wgpu::Device,
    label: &str,
    bytes: u64,
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: bytes.max(16),
        usage,
        mapped_at_creation: false,
    })
}

impl Renderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("uniforms"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let node_mod = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("nodes"),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(format!("{COMMON}\n{NODES}"))),
        });
        let edge_mod = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("edges"),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(format!("{COMMON}\n{EDGES}"))),
        });
        let target = [Some(wgpu::ColorTargetState {
            format,
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        })];
        let pos_attr = [wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x2,
            offset: 0,
            shader_location: 0,
        }];
        let kind_attr = [wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Uint32,
            offset: 0,
            shader_location: 1,
        }];
        let node_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("nodes"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &node_mod,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[
                    wgpu::VertexBufferLayout {
                        array_stride: 8,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &pos_attr,
                    },
                    wgpu::VertexBufferLayout {
                        array_stride: 4,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &kind_attr,
                    },
                ],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &node_mod,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &target,
            }),
            multiview: None,
            cache: None,
        });
        let edge_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("edges"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &edge_mod,
                entry_point: Some("vs_edge"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 8,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &pos_attr,
                }],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &edge_mod,
                entry_point: Some("fs_edge"),
                compilation_options: Default::default(),
                targets: &target,
            }),
            multiview: None,
            cache: None,
        });
        let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("uniforms"),
            contents: bytemuck::bytes_of(&Uniforms::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("uniforms"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            }],
        });
        let vb = wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST;
        Self {
            node_pipe,
            edge_pipe,
            uniform_buf,
            bind_group,
            pos_buf: buffer(device, "pos", 16, vb),
            attr_buf: buffer(device, "attr", 16, vb),
            edge_buf: buffer(device, "edges", 16, vb),
            cap_nodes: 0,
            cap_edge_floats: 0,
            n_nodes: 0,
            n_edge_verts: 0,
            pos_ver: u64::MAX,
            attr_ver: u64::MAX,
            edge_ver: u64::MAX,
        }
    }

    /// バッファとユニフォームを更新する（描画の前に毎フレーム呼ぶ）。
    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, f: &Frame) {
        queue.write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(&f.uniforms));
        let n = f.pos.len() / 2;
        if f.pos_ver != self.pos_ver || n > self.cap_nodes {
            if n > self.cap_nodes {
                let vb = wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST;
                self.pos_buf = buffer(device, "pos", (n * 8) as u64, vb);
                self.attr_buf = buffer(device, "attr", (n * 4) as u64, vb);
                self.cap_nodes = n;
                self.attr_ver = u64::MAX;
            }
            queue.write_buffer(&self.pos_buf, 0, bytemuck::cast_slice(&f.pos));
            self.n_nodes = n as u32;
            self.pos_ver = f.pos_ver;
        }
        if f.attr_ver != self.attr_ver {
            queue.write_buffer(
                &self.attr_buf,
                0,
                bytemuck::cast_slice(&f.attrs[..n.min(f.attrs.len())]),
            );
            self.attr_ver = f.attr_ver;
        }
        if f.edge_ver != self.edge_ver || f.edges.len() > self.cap_edge_floats {
            if f.edges.len() > self.cap_edge_floats {
                self.edge_buf = buffer(
                    device,
                    "edges",
                    (f.edges.len() * 4) as u64,
                    wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                );
                self.cap_edge_floats = f.edges.len();
            }
            if !f.edges.is_empty() {
                queue.write_buffer(&self.edge_buf, 0, bytemuck::cast_slice(&f.edges));
            }
            self.n_edge_verts = (f.edges.len() / 2) as u32;
            self.edge_ver = f.edge_ver;
        }
    }

    /// 辺 → ノードの順に描く（ノードが辺の上に重なる）。
    pub fn paint(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_bind_group(0, &self.bind_group, &[]);
        if self.n_edge_verts > 0 {
            pass.set_pipeline(&self.edge_pipe);
            pass.set_vertex_buffer(0, self.edge_buf.slice(..));
            pass.draw(0..self.n_edge_verts, 0..1);
        }
        if self.n_nodes > 0 {
            pass.set_pipeline(&self.node_pipe);
            pass.set_vertex_buffer(0, self.pos_buf.slice(..));
            pass.set_vertex_buffer(1, self.attr_buf.slice(..));
            pass.draw(0..4, 0..self.n_nodes);
        }
    }
}

#[cfg(test)]
mod tests;
