//! オフスクリーンで描いてピクセルを読み、`camera.rs` の位置にノードがあること・上下ミラー位置に無いことを確かめる。
//! GPU アダプタが無い環境ではスキップする。

use super::*;

struct Offscreen {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

fn gpu() -> Option<Offscreen> {
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .ok()?;
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()?;
    Some(Offscreen { device, queue })
}

const W: u32 = 400;
const H: u32 = 300;

/// 描画して RGBA8 のピクセル列（左上原点）を返す。
fn render(g: &Offscreen, frame: &Frame) -> Vec<u8> {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut r = Renderer::new(&g.device, format);
    r.prepare(&g.device, &g.queue, frame);
    let tex = g.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("target"),
        size: wgpu::Extent3d {
            width: W,
            height: H,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = tex.create_view(&Default::default());
    let row = (W * 4).div_ceil(256) * 256;
    let out = g.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (row * H) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = g.device.create_command_encoder(&Default::default());
    {
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        r.paint(&mut pass);
    }
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &out,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(H),
            },
        },
        wgpu::Extent3d {
            width: W,
            height: H,
            depth_or_array_layers: 1,
        },
    );
    g.queue.submit([enc.finish()]);
    let slice = out.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    g.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let data = slice.get_mapped_range();
    let mut px = Vec::with_capacity((W * H * 4) as usize);
    for y in 0..H {
        px.extend_from_slice(&data[(y * row) as usize..(y * row + W * 4) as usize]);
    }
    px
}

fn alpha_at(px: &[u8], x: f32, y: f32) -> u8 {
    let (xi, yi) = (x.round() as i32, y.round() as i32);
    if xi < 0 || yi < 0 || xi >= W as i32 || yi >= H as i32 {
        return 0;
    }
    px[((yi as u32 * W + xi as u32) * 4 + 3) as usize]
}

/// 1px の線は丸めで隣のピクセルに乗ることがあるので、3×3 の範囲で最大のアルファを返す。
fn alpha_near(px: &[u8], x: f32, y: f32) -> u8 {
    let mut m = 0;
    for dy in -1..=1 {
        for dx in -1..=1 {
            m = m.max(alpha_at(px, x + dx as f32, y + dy as f32));
        }
    }
    m
}

fn frame_for(cam: &Camera, pos: Vec<f32>, kinds: Vec<u32>, edges: Vec<f32>) -> Frame {
    let t = Tokens::load();
    let uniforms = Uniforms::new(&t, cam, [W as f32, H as f32], t.bg, false);
    Frame {
        uniforms,
        pos: Arc::new(pos),
        pos_ver: 1,
        attrs: Arc::new(kinds),
        attr_ver: 1,
        edges: Arc::new(edges),
        edge_ver: 1,
    }
}

#[test]
fn nodes_are_drawn_where_the_2d_transform_says_not_mirrored() {
    let Some(g) = gpu() else {
        eprintln!("GPU アダプタが無いのでスキップ");
        return;
    };
    // 画面中心から縦にも横にもずらしたカメラ（反転していればミラー位置にずれる）
    for cam in [
        Camera {
            cx: 0.0,
            cy: 0.0,
            zoom: 40.0,
        },
        Camera {
            cx: 0.5,
            cy: 0.8,
            zoom: 35.0,
        },
        Camera {
            cx: -2.0,
            cy: -1.5,
            zoom: 20.0,
        },
    ] {
        let nodes: [[f32; 2]; 5] = [
            [0.0, 0.0],
            [1.5, 1.0],
            [-2.0, 2.2],
            [2.0, -1.8],
            [-1.0, -2.5],
        ];
        let pos: Vec<f32> = nodes.iter().flatten().copied().collect();
        let kinds = vec![5, 1, 2, 3, 4];
        let px = render(&g, &frame_for(&cam, pos, kinds, vec![]));
        for p in nodes {
            let s = cam.to_screen([W as f32, H as f32], p);
            assert!(
                alpha_at(&px, s[0], s[1]) > 0,
                "{cam:?} {p:?}: 位置 {s:?} にノードが無い"
            );
            // 上下ミラー位置（ビューの縦の中心に対して）に別のノードが無ければ、何も描かれていない
            let mirror = [s[0], H as f32 - s[1]];
            let near_other = nodes.iter().any(|q| {
                let t = cam.to_screen([W as f32, H as f32], *q);
                (t[0] - mirror[0]).abs() < 14.0 && (t[1] - mirror[1]).abs() < 14.0
            });
            if (s[1] - H as f32 / 2.0).abs() > 30.0 && !near_other {
                assert_eq!(
                    alpha_at(&px, mirror[0], mirror[1]),
                    0,
                    "{cam:?} {p:?}: ミラー位置 {mirror:?} に描かれている（縦反転）"
                );
            }
        }
    }
}

#[test]
fn edges_end_at_the_nodes() {
    let Some(g) = gpu() else {
        eprintln!("GPU アダプタが無いのでスキップ");
        return;
    };
    let cam = Camera {
        cx: 0.5,
        cy: 1.0,
        zoom: 50.0,
    };
    // 2 ノードを結ぶ水平な辺。端点（ノード中心）に辺の色が乗る
    let (a, b) = ([-2.0f32, 2.0], [2.0f32, 2.0]);
    let px = render(
        &g,
        &frame_for(
            &cam,
            vec![a[0], a[1], b[0], b[1]],
            vec![1, 1],
            vec![a[0], a[1], b[0], b[1]],
        ),
    );
    let size = [W as f32, H as f32];
    let mid = cam.to_screen(size, [0.0, 2.0]);
    assert!(
        alpha_near(&px, mid[0], mid[1]) > 0,
        "辺の中点にピクセルが無い"
    );
    let mirrored = [mid[0], H as f32 - mid[1]];
    assert_eq!(
        alpha_near(&px, mirrored[0], mirrored[1]),
        0,
        "辺がミラー位置にある"
    );
}

#[test]
fn many_nodes_draw_in_one_call() {
    let Some(g) = gpu() else {
        return;
    };
    let cam = Camera {
        cx: 0.0,
        cy: 0.0,
        zoom: 8.0,
    };
    let n = 20_000usize;
    let mut pos = Vec::with_capacity(n * 2);
    for i in 0..n {
        let a = i as f32 * 0.37;
        let r = (i as f32 / n as f32) * 20.0;
        pos.push(r * a.cos());
        pos.push(r * a.sin());
    }
    let px = render(&g, &frame_for(&cam, pos, vec![2; n], vec![]));
    assert!(px.chunks(4).any(|p| p[3] > 0));
}
