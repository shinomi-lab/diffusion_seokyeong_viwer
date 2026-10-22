// 9.4 のフラグメントの規則どおり。kind は NodeKind と同じ番号。色と寸法はユニフォームで渡す（tokens から）。
export const VERT = `#version 300 es
precision highp float;
in vec2 a_corner;           // 四角形の角 (-1..1)
in vec2 a_pos;              // ワールド座標（インスタンスごと）
in uvec2 a_attr;            // x = kind, y = pulse（インスタンスごと）
uniform vec2 u_center;      // カメラ中心（ワールド）
uniform float u_zoom;       // ワールド 1 単位あたりの CSS px
uniform vec2 u_res;         // キャンバスの CSS px
uniform float u_radius;     // ノード半径 px
uniform float u_halo;       // 現在のウェーブの輪の倍率
uniform float u_kindScale[6];
out vec2 v_p;
flat out uint v_kind;
flat out uint v_pulse;
void main() {
  float s = u_radius * u_kindScale[a_attr.x] * u_halo;
  // NDC は y が上向きで、ワールドも y が上向き。ここでは反転しない（反転が要るのは y が下向きの Canvas2D だけ。render/camera.ts の toScreen）。
  vec2 screen = (a_pos - u_center) * u_zoom + a_corner * s;
  gl_Position = vec4(screen / (u_res * 0.5), 0.0, 1.0);
  v_p = a_corner * u_halo;
  v_kind = a_attr.x;
  v_pulse = a_attr.y;
}`;

export const FRAG = `#version 300 es
precision highp float;
in vec2 v_p;
flat in uint v_kind;
flat in uint v_pulse;
uniform vec3 u_fill[6];
uniform vec3 u_stroke[6];
uniform float u_strokeFrac[6];   // 縁の太さ（半径に対する割合）。0 なら縁なし
uniform vec3 u_blue;
uniform vec3 u_green;
uniform vec3 u_bg;
uniform vec3 u_pulseColor;
uniform float u_halo;
out vec4 o;
void main() {
  int k = int(v_kind);
  bool square = (k == 3 || k == 4);
  float d = square ? max(abs(v_p.x), abs(v_p.y)) : length(v_p);
  float aa = max(fwidth(d), 1e-4);
  if (d <= 1.0 + aa) {
    vec3 col = u_fill[k];
    if (k == 4) col = (v_p.x < 0.0) ? u_blue : u_green;          // 両方活性: 左右半分塗り
    float sf = u_strokeFrac[k];
    if (sf > 0.0 && d > 1.0 - sf) col = u_stroke[k];
    else if (k == 0) col = u_bg;                                   // 未受信: 白抜き（内側は背景色）
    float a = 1.0 - smoothstep(1.0 - aa, 1.0 + aa, d);
    o = vec4(col, a);
    return;
  }
  if (v_pulse == 1u && d <= u_halo) {                              // 現在のウェーブの輪
    float inner = smoothstep(1.0, 1.0 + aa, d);
    o = vec4(u_pulseColor, inner);
    return;
  }
  discard;
}`;
