// ノード・辺で共通のユニフォーム。ワールド → NDC は CPU 側（camera.rs の ndc_transform）で作る。
// シェーダでは y を反転しない（NDC もワールドも y が上向き。反転が要るのは y が下向きの egui の 2D だけ）。
struct U {
  scale: vec2<f32>,
  offset: vec2<f32>,
  view: vec2<f32>,        // ビューの大きさ（ポイント）
  radius: f32,            // ノード半径（ポイント）
  halo: f32,              // 現在のウェーブの輪の倍率
  blue: vec4<f32>,
  green: vec4<f32>,
  bg: vec4<f32>,
  pulse_color: vec4<f32>,
  edge_color: vec4<f32>,
  fill: array<vec4<f32>, 6>,
  stroke: array<vec4<f32>, 6>,
  kind: array<vec4<f32>, 6>,  // x = 半径の倍率, y = 縁の太さ（半径に対する割合。0 なら縁なし）
  rec_color: vec4<f32>,
  chain_color: vec4<f32>,
  rec_ring: vec4<f32>,        // x = 推薦で受信した内部活性の人の青い縁取りの太さ（ポイント）
};
@group(0) @binding(0) var<uniform> u: U;
