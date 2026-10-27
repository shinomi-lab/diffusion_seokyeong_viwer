// 9.4 のフラグメントの規則どおり。kind は NodeKind と同じ番号（0 未受信 … 5 IP）。
struct VsOut {
  @builtin(position) pos: vec4<f32>,
  @location(0) p: vec2<f32>,
  @location(1) @interpolate(flat) kind: u32,
  @location(2) @interpolate(flat) pulse: u32,
  @location(3) ring_w: f32,   // 縁取りの太さ（半径に対する割合）。0 なら縁取りなし
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, @location(0) pos: vec2<f32>, @location(1) attr: u32) -> VsOut {
  var corners = array<vec2<f32>, 4>(vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(-1.0, 1.0), vec2(1.0, 1.0));
  let corner = corners[vi];
  let kind = attr & 255u;
  let pulse = (attr >> 8u) & 255u;
  let r = u.radius * u.kind[kind].x;
  let via = (pulse >> 1u) & 3u;
  var ring_px = 0.0;
  // 推薦で受信: 中を黄色で塗り、内部活性（内部・両方）なら青の縁取り。推薦由来: 縁取りなしで、中を暗い黄色で塗る
  if (via == 1u && (kind == 2u || kind == 4u)) { ring_px = u.rec_ring.x; }
  let ring_w = ring_px / r;
  // 四角形は現在のウェーブの輪と縁取り（＋アンチエイリアスの 1.5 ポイント）が収まる大きさにする
  let ext = max(u.halo, select(0.0, 1.0 + ring_w + 1.5 / r, via != 0u));
  let s = r * ext;                                             // 四角形の半分の大きさ（ポイント）
  let center = pos * u.scale + u.offset;                       // NDC
  let ndc = center + corner * s * vec2(2.0, 2.0) / u.view;     // ポイント → NDC（幅 = 2 / view）
  var o: VsOut;
  o.pos = vec4(ndc, 0.0, 1.0);
  o.p = corner * ext;
  o.ring_w = ring_w;
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
  let via = (in.pulse >> 1u) & 3u;
  let ring_col = u.blue.rgb;                                              // 推薦で受信の縁取り = 内部活性の青
  if (d <= 1.0 + aa) {
    var col = u.fill[k].rgb;
    if (k == 4u) { col = select(u.green.rgb, u.blue.rgb, in.p.x < 0.0); }   // 両方活性: 左右半分塗り
    let sf = u.kind[k].y;
    if (sf > 0.0 && d > 1.0 - sf) { col = u.stroke[k].rgb; }
    else if (k == 0u) { col = u.bg.rgb; }                                   // 未受信: 白抜き（内側は背景色）
    if (via == 1u) { col = u.rec_color.rgb; }                               // 推薦で受信: 中を黄色で塗る（形は残す）
    if (via == 2u) { col = u.chain_color.rgb; }                             // 推薦由来: 中を暗い黄色で塗る（形は残す）
    let a = 1.0 - smoothstep(1.0 - aa, 1.0 + aa, d);
    // 縁取りがあるときは、ノードの縁を縁取りの上に重ねる（縁の外側に隙間を作らない）
    if (in.ring_w > 0.0) { return vec4(mix(ring_col, col, a), 1.0); }
    return vec4(col, a);
  }
  if (in.ring_w > 0.0 && d <= 1.0 + in.ring_w + aa) {                     // 縁取り（ノードの外側）
    return vec4(ring_col, 1.0 - smoothstep(1.0 + in.ring_w - aa, 1.0 + in.ring_w + aa, d));
  }
  if ((in.pulse & 1u) == 1u && d <= u.halo) {                                      // 現在のウェーブの輪
    return vec4(u.pulse_color.rgb, smoothstep(1.0, 1.0 + aa, d));
  }
  discard;
  return vec4(0.0);
}
