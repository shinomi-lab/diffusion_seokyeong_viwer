// 自前の WebGL2 インスタンス描画。1 回の drawArraysInstanced で全ノードを描く（9.4）。
import { hexToRgb, T } from '../tokens';
import { FRAG, VERT } from './shaders';
import { effectiveRadius } from '../logic/units';

function compile(gl: WebGL2RenderingContext, type: number, src: string): WebGLShader {
  const sh = gl.createShader(type)!;
  gl.shaderSource(sh, src);
  gl.compileShader(sh);
  if (!gl.getShaderParameter(sh, gl.COMPILE_STATUS)) throw new Error(`シェーダのコンパイルに失敗: ${gl.getShaderInfoLog(sh)}`);
  return sh;
}

export interface DrawParams {
  center: [number, number];
  zoom: number;
  width: number;
  height: number;
  /** 背景色（未受信ノードの内側の色にも使う）。 */
  bg: [number, number, number];
}

export class NodeRenderer {
  private gl: WebGL2RenderingContext;
  private prog: WebGLProgram;
  private vao: WebGLVertexArrayObject;
  private posBuf: WebGLBuffer;
  private attrBuf: WebGLBuffer;
  private u: Record<string, WebGLUniformLocation | null> = {};
  private count = 0;
  private attrBytes = new Uint8Array(0);
  private lastParams: DrawParams | null = null;

  constructor(private canvas: HTMLCanvasElement) {
    // preserveDrawingBuffer: 画像・録画で、描画済みの内容を別のキャンバスに合成するために必要
    const gl = canvas.getContext('webgl2', { antialias: false, alpha: true, premultipliedAlpha: false, preserveDrawingBuffer: true });
    if (!gl) throw new Error('このブラウザは WebGL2 に対応していません');
    this.gl = gl;
    const prog = gl.createProgram()!;
    gl.attachShader(prog, compile(gl, gl.VERTEX_SHADER, VERT));
    gl.attachShader(prog, compile(gl, gl.FRAGMENT_SHADER, FRAG));
    gl.linkProgram(prog);
    if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) throw new Error(`リンクに失敗: ${gl.getProgramInfoLog(prog)}`);
    this.prog = prog;
    for (const n of ['u_center', 'u_zoom', 'u_res', 'u_radius', 'u_halo', 'u_kindScale', 'u_fill', 'u_stroke', 'u_strokeFrac', 'u_blue', 'u_green', 'u_bg', 'u_pulseColor', 'u_recColor', 'u_chainColor', 'u_recRingPx']) {
      this.u[n] = gl.getUniformLocation(prog, n);
    }
    // 開発時のみ: 描画結果のピクセルを読む（Canvas2D の位置と合っているかを自動テストで確かめる用）
    if (import.meta.env.DEV) (window as unknown as { __glProbe: unknown }).__glProbe = (x: number, y: number) => this.probe(x, y);
    this.vao = gl.createVertexArray()!;
    gl.bindVertexArray(this.vao);
    const corner = gl.createBuffer()!;
    gl.bindBuffer(gl.ARRAY_BUFFER, corner);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]), gl.STATIC_DRAW);
    const aCorner = gl.getAttribLocation(prog, 'a_corner');
    gl.enableVertexAttribArray(aCorner);
    gl.vertexAttribPointer(aCorner, 2, gl.FLOAT, false, 0, 0);
    this.posBuf = gl.createBuffer()!;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.posBuf);
    const aPos = gl.getAttribLocation(prog, 'a_pos');
    gl.enableVertexAttribArray(aPos);
    gl.vertexAttribPointer(aPos, 2, gl.FLOAT, false, 0, 0);
    gl.vertexAttribDivisor(aPos, 1);
    this.attrBuf = gl.createBuffer()!;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.attrBuf);
    const aAttr = gl.getAttribLocation(prog, 'a_attr');
    gl.enableVertexAttribArray(aAttr);
    gl.vertexAttribIPointer(aAttr, 2, gl.UNSIGNED_BYTE, 2, 0);
    gl.vertexAttribDivisor(aAttr, 1);
    gl.bindVertexArray(null);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
  }

  /** 描画するノードの位置を設定する（ノード集合が変わったときだけ）。 */
  setPositions(xy: Float32Array): void {
    const gl = this.gl;
    this.count = xy.length / 2;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.posBuf);
    gl.bufferData(gl.ARRAY_BUFFER, xy, gl.DYNAMIC_DRAW);
    this.attrBytes = new Uint8Array(this.count * 2);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.attrBuf);
    gl.bufferData(gl.ARRAY_BUFFER, this.attrBytes, gl.DYNAMIC_DRAW);
  }

  /** 状態（kind と pulse）を更新する。 */
  setStates(kinds: Uint8Array, pulse: Uint8Array): void {
    const a = this.attrBytes;
    for (let i = 0; i < this.count; i++) {
      a[2 * i] = kinds[i];
      a[2 * i + 1] = pulse[i];
    }
    this.gl.bindBuffer(this.gl.ARRAY_BUFFER, this.attrBuf);
    this.gl.bufferSubData(this.gl.ARRAY_BUFFER, 0, a);
  }

  draw(p: DrawParams): void {
    this.lastParams = p;
    const gl = this.gl;
    gl.viewport(0, 0, this.canvas.width, this.canvas.height);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);
    if (this.count === 0) return;
    gl.useProgram(this.prog);
    const u = this.u;
    const radius = effectiveRadius(T.node.radiusPx, this.count);
    gl.uniform2f(u.u_center, p.center[0], p.center[1]);
    gl.uniform1f(u.u_zoom, p.zoom);
    gl.uniform2f(u.u_res, p.width, p.height);
    gl.uniform1f(u.u_radius, radius);
    gl.uniform1f(u.u_halo, T.highlight.haloScale);
    const fill: number[] = [], stroke: number[] = [], sfrac: number[] = [], scale: number[] = [];
    for (const k of ['0', '1', '2', '3', '4', '5'] as const) {
      const d = T.kinds[k] as { fill?: string | null; stroke?: string | null; strokeWidth?: number; radiusScale?: number };
      fill.push(...hexToRgb(d.fill ?? '#000000'));
      stroke.push(...hexToRgb(d.stroke ?? '#000000'));
      const rs = d.radiusScale ?? 1;
      sfrac.push(d.stroke && d.strokeWidth ? d.strokeWidth / (radius * rs) : 0);
      scale.push(rs);
    }
    gl.uniform3fv(u.u_fill, fill);
    gl.uniform3fv(u.u_stroke, stroke);
    gl.uniform1fv(u.u_strokeFrac, sfrac);
    gl.uniform1fv(u.u_kindScale, scale);
    const split = (T.kinds['4'] as { fillSplit: string[] }).fillSplit;
    gl.uniform3fv(u.u_blue, hexToRgb(split[0]));
    gl.uniform3fv(u.u_green, hexToRgb(split[1]));
    gl.uniform3fv(u.u_bg, p.bg);
    gl.uniform3fv(u.u_pulseColor, hexToRgb(T.highlight.currentWave));
    gl.uniform3fv(u.u_recColor, hexToRgb(T.highlight.recommended));
    gl.uniform3fv(u.u_chainColor, hexToRgb(T.highlight.recChain));
    gl.uniform1f(u.u_recRingPx, T.highlight.recRingPx);
    gl.bindVertexArray(this.vao);
    gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, this.count);
    gl.bindVertexArray(null);
  }

  /** 開発・テスト用: 最後の描画を再実行し、CSS 座標（左上原点）のピクセルを読む。 */
  probe(cssX: number, cssY: number): { r: number; g: number; b: number; a: number } | null {
    const p = this.lastParams;
    if (!p) return null;
    this.draw(p); // 読む直前に同じ内容で描き直す（描画バッファはコンポジット後に空になるため）
    const dpr = this.canvas.width / p.width;
    const x = Math.round(cssX * dpr);
    const y = this.canvas.height - 1 - Math.round(cssY * dpr); // GL の原点は左下
    if (x < 0 || y < 0 || x >= this.canvas.width || y >= this.canvas.height) return null;
    const px = new Uint8Array(4);
    this.gl.readPixels(x, y, 1, 1, this.gl.RGBA, this.gl.UNSIGNED_BYTE, px);
    return { r: px[0], g: px[1], b: px[2], a: px[3] };
  }
}
