// 9.4 のフラグメントの規則どおり。kind は NodeKind と同じ番号（0 未受信 … 5 IP）。
struct VsOut {
  @builtin(position) pos: vec4<f32>,
  @location(0) p: vec2<f32>,
  @location(1) @interpolate(flat) kind: u32,
  @location(2) @interpolate(flat) pulse: u32,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, @location(0) pos: vec2<f32>, @location(1) attr: u32) -> VsOut {
  var corners = array<vec2<f32>, 4>(vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(-1.0, 1.0), vec2(1.0, 1.0));
  let corner = corners[vi];
  let kind = attr & 255u;
  let pulse = (attr >> 8u) & 255u;
  let s = u.radius * u.kind[kind].x * u.halo;                 // 四角形の半分の大きさ（ポイント）
  let center = pos * u.scale + u.offset;                       // NDC
  let ndc = center + corner * s * vec2(2.0, 2.0) / u.view;     // ポイント → NDC（幅 = 2 / view）
  var o: VsOut;
  o.pos = vec4(ndc, 0.0, 1.0);
  o.p = corner * u.halo;
  o.kind = kind;
  o.pulse = pulse;
  return o;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
  let k = in.kind;
  let square = (k == 3u || k == 4u);
  let d = select(length(in.p), max(abs(in.p.x), abs(in.p.y)), square);
  let aa = max(fwidth(d), 0.0001);
  if (d <= 1.0 + aa) {
    var col = u.fill[k].rgb;
    if (k == 4u) { col = select(u.green.rgb, u.blue.rgb, in.p.x < 0.0); }   // 両方活性: 左右半分塗り
    let sf = u.kind[k].y;
    if (sf > 0.0 && d > 1.0 - sf) { col = u.stroke[k].rgb; }
    else if (k == 0u) { col = u.bg.rgb; }                                   // 未受信: 白抜き（内側は背景色）
    let a = 1.0 - smoothstep(1.0 - aa, 1.0 + aa, d);
    return vec4(col, a);
  }
  if (in.pulse == 1u && d <= u.halo) {                                      // 現在のウェーブの輪
    return vec4(u.pulse_color.rgb, smoothstep(1.0, 1.0 + aa, d));
  }
  discard;
  return vec4(0.0);
}
