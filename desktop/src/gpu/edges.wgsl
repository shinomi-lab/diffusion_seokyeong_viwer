@vertex
fn vs_edge(@location(0) pos: vec2<f32>) -> @builtin(position) vec4<f32> {
  return vec4(pos * u.scale + u.offset, 0.0, 1.0);
}

@fragment
fn fs_edge() -> @location(0) vec4<f32> {
  return u.edge_color;
}
