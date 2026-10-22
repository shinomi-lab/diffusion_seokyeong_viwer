// ネットワーク表示（F06・F07・F11）。WebGL2 でノード、Canvas2D でリング・辺・扇形・強調を重ねる。
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { EXPAND_CAP, EDGE_CAP, N_SECTORS, effectiveRadius } from '../logic/units';
import { aggregateSectors, ringCount, ringTotals } from '../logic/sectors';
import { boundsOf } from '../logic/layout';
import { useDisplay } from '../store/display';
import { SPEEDS, usePlayback } from '../store/playback';
import { useSession, useSessionB } from '../store/session';
import { useView } from '../store/view';
import { hexToRgb, KIND_LABELS, T } from '../tokens';
import { sim } from '../worker/client';
import { fitCamera, toScreen, toWorld, zoomAt } from './camera';
import { registerLayers } from './capture';
import { NodeRenderer } from './gl';
import { hitTest } from './hit';
import * as ov from './overlay';

const MINI = 150;


/** `slot`: 比較（F13）の A = 0 / B = 1。カメラ・選択・再生位置は共有し、状態（kinds）だけがスロットごとに違う。 */
export function NetworkView({ slot = 0, label }: { slot?: 0 | 1; label?: string }) {
  const useS = slot === 1 ? useSessionB : useSession;
  const positions = useS((s) => s.positions);
  const dist = useS((s) => s.dist);
  const angle = useS((s) => s.angle);
  const ip = useS((s) => s.ip);
  const ipInfo = useS((s) => s.ipInfo);
  const nodeCount = positions ? positions.xy.length / 2 : 0;

  const mode = useView((s) => s.mode);
  const expanded = useView((s) => s.expanded);
  const expandedLabel = useView((s) => s.expandedLabel);
  const fromOverview = useView((s) => s.fromOverview);
  const camera = useView((s) => s.camera);
  const selectedNode = useView((s) => s.selectedNode);
  const notice = useView((s) => s.notice);
  const edgesTruncated = useView((s) => s.edgesTruncated);
  const view = useView.getState;

  const display = useDisplay(slot);
  const playing = usePlayback((s) => s.playing);

  const wrapRef = useRef<HTMLDivElement>(null);
  const baseRef = useRef<HTMLCanvasElement>(null);
  const glRef = useRef<HTMLCanvasElement>(null);
  const overRef = useRef<HTMLCanvasElement>(null);
  const miniRef = useRef<HTMLCanvasElement>(null);
  const rendererRef = useRef<NodeRenderer | null>(null);
  const [size, setSize] = useState({ w: 800, h: 520 });
  const [glError, setGlError] = useState<string | null>(null);
  const [edges, setEdges] = useState<Uint32Array>(new Uint32Array(0));
  const [rubber, setRubber] = useState<{ x0: number; y0: number; x1: number; y1: number } | null>(null);
  const [hover, setHover] = useState<{ ring: number; sector: number; x: number; y: number } | null>(null);
  const frameMs = useRef(0);

  // ---- サイズ ----
  useEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setSize({ w: Math.max(200, el.clientWidth), h: Math.max(200, el.clientHeight) }));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // ---- IP が選ばれたら初期モードを決める（1 万ノード以下は個別表示、超えたら全体図） ----
  useEffect(() => {
    if (!positions) return;
    useView.getState().resetView(positions.xy.length / 2 > EXPAND_CAP ? 'overview' : 'expanded');
  }, [positions]);

  // 展開中でないときは、IP から到達できるノードだけを描く。到達不能のノードは格子状に並べると
  // 大きな塊になって画面を占めるので、通常は出さない（件数を示し、クリックで表示する）。
  const reachableNodes = useMemo(() => {
    const out: number[] = [];
    for (let i = 0; i < nodeCount; i++) if (!dist || dist[i] !== 0xffffffff) out.push(i);
    return Uint32Array.from(out);
  }, [nodeCount, dist]);
  const drawNodes = useMemo(() => (mode === 'expanded' ? (expanded ?? reachableNodes) : new Uint32Array(0)), [mode, expanded, reachableNodes]);
  const subXY = useMemo(() => {
    if (!positions) return new Float32Array(0);
    const a = new Float32Array(drawNodes.length * 2);
    drawNodes.forEach((i, j) => {
      a[2 * j] = positions.xy[2 * i];
      a[2 * j + 1] = positions.xy[2 * i + 1];
    });
    return a;
  }, [positions, drawNodes]);

  const visible = useMemo(() => {
    if (mode !== 'expanded' || expanded === null) return null;
    const v = new Uint8Array(nodeCount);
    for (const i of expanded) v[i] = 1;
    return v;
  }, [mode, expanded, nodeCount]);
  const nRings = dist ? ringCount(dist) : 0;
  const maxDist = Math.max(0, nRings - 1);

  // ---- カメラをノード集合・モードに合わせる ----
  useEffect(() => {
    if (!positions) return;
    if (mode === 'overview') {
      const r = maxDist + 0.8;
      const b = { minX: -r, minY: -r, maxX: r, maxY: r };
      if ((ipInfo?.unreachable ?? 0) > 0) b.maxX = ov.unreachableRect(maxDist).x + 3.2;
      useView.getState().setCamera(fitCamera(b, size.w, size.h));
    } else {
      useView.getState().setCamera(fitCamera(boundsOf(positions.xy, expanded ?? reachableNodes), size.w, size.h));
    }
    // 意図: ノード集合かモードが変わったときだけ合わせる（リサイズでは動かさない）
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [positions, mode, expanded, reachableNodes]);

  // ---- 辺（展開中のノード集合の内側だけ。上限 EDGE_CAP） ----
  useEffect(() => {
    if (mode !== 'expanded' || drawNodes.length === 0) {
      setEdges(new Uint32Array(0));
      return;
    }
    let cancel = false;
    sim.call('edgesAmong', { nodes: drawNodes.slice(), cap: EDGE_CAP }, [], slot).then((e) => {
      if (cancel) return;
      setEdges(e);
      useView.getState().setEdgesTruncated(e.length / 2 >= EDGE_CAP);
    }).catch(() => undefined);
    return () => {
      cancel = true;
    };
  }, [mode, drawNodes]);

  // ---- 画像・録画のために、3 層のキャンバスを登録する（F15） ----
  useEffect(() => registerLayers(slot, { base: baseRef.current, gl: glRef.current, over: overRef.current }), [slot]);

  // ---- WebGL ----
  useEffect(() => {
    if (!glRef.current) return;
    try {
      rendererRef.current = new NodeRenderer(glRef.current);
    } catch (e) {
      setGlError(e instanceof Error ? e.message : String(e));
    }
  }, []);
  useEffect(() => {
    rendererRef.current?.setPositions(subXY);
  }, [subXY]);

  const sectors = useMemo(
    () => (mode === 'overview' && display && dist && angle ? aggregateSectors(display.kinds, dist, angle) : null),
    [mode, display, dist, angle],
  );
  const totals = useMemo(() => (sectors ? ringTotals(sectors, nRings) : []), [sectors, nRings]);

  // ---- 描画（レイヤーごとに更新条件を分ける。再生中は GL とオーバーレイだけ描き直す） ----
  const dpr = window.devicePixelRatio || 1;
  const surf = (c: HTMLCanvasElement | null): ov.Surface | null => {
    const ctx = c?.getContext('2d');
    if (!c || !ctx) return null;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    return { ctx, w: size.w, h: size.h, cam: camera };
  };

  // キャンバスの大きさ（他の描画より先に実行される）
  useEffect(() => {
    for (const c of [baseRef.current, glRef.current, overRef.current]) {
      if (!c) continue;
      if (c.width !== Math.round(size.w * dpr) || c.height !== Math.round(size.h * dpr)) {
        c.width = Math.round(size.w * dpr);
        c.height = Math.round(size.h * dpr);
      }
    }
  }, [size, dpr]);

  // 背景: リング・辺・扇形。再生中の wave の進行では変わらない（全体図の扇形だけは状態に依存する）
  useEffect(() => {
    const base = surf(baseRef.current);
    if (!base || !positions) return;
    ov.clear(base);
    if (mode === 'overview') {
      if (sectors) ov.drawSectors(base, sectors, nRings);
      ov.drawRings(base, maxDist);
      if ((ipInfo?.unreachable ?? 0) > 0) ov.drawUnreachable(base, maxDist, ipInfo!.unreachable);
    } else {
      ov.drawRings(base, maxDist);
      ov.drawEdges(base, positions.xy, edges);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [size, camera, edges, mode, sectors, positions, maxDist, nRings, ipInfo]);

  // ノード（WebGL）
  useEffect(() => {
    const t0 = performance.now();
    const r = rendererRef.current;
    if (!r || !display) return;
    const kinds = new Uint8Array(drawNodes.length);
    const pulse = new Uint8Array(drawNodes.length);
    for (let j = 0; j < drawNodes.length; j++) {
      kinds[j] = display.kinds[drawNodes[j]];
      pulse[j] = display.pulse[drawNodes[j]];
    }
    r.setStates(kinds, pulse);
    r.draw({ center: [camera.cx, camera.cy], zoom: camera.zoom, width: size.w, height: size.h, bg: hexToRgb(T.background) });
    frameMs.current = performance.now() - t0;
  }, [size, camera, display, drawNodes, subXY]);

  // 強調・選択・範囲選択
  useEffect(() => {
    const over = surf(overRef.current);
    if (!over || !positions) return;
    ov.clear(over);
    if (mode === 'expanded' && display) {
      ov.drawWaveEdges(over, positions.xy, display.events.from, display.events.node, display.events.wave, display.k, visible);
      if (selectedNode !== null) ov.drawSelected(over, positions.xy, selectedNode, selectedNode === ip ? 2.2 : 1);
    }
    if (rubber) {
      over.ctx.save();
      over.ctx.strokeStyle = T.highlight.selected;
      over.ctx.fillStyle = 'rgba(213,94,0,0.10)';
      const x = Math.min(rubber.x0, rubber.x1), y = Math.min(rubber.y0, rubber.y1);
      over.ctx.fillRect(x, y, Math.abs(rubber.x1 - rubber.x0), Math.abs(rubber.y1 - rubber.y0));
      over.ctx.strokeRect(x, y, Math.abs(rubber.x1 - rubber.x0), Math.abs(rubber.y1 - rubber.y0));
      over.ctx.restore();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [size, camera, display, mode, selectedNode, rubber, positions, ip, visible]);

  // 縮小地図（状態が変わったときだけ）
  useEffect(() => {
    const mc = miniRef.current?.getContext('2d');
    if (!mc || !positions || !miniRef.current) return;
    miniRef.current.width = MINI * dpr;
    miniRef.current.height = MINI * dpr;
    mc.setTransform(dpr, 0, 0, dpr, 0, 0);
    mc.clearRect(0, 0, MINI, MINI);
    mc.fillStyle = T.chrome.card;
    mc.fillRect(0, 0, MINI, MINI);
    const ext = maxDist + 1.2;
    const sc = (MINI / 2 - 6) / ext;
    mc.strokeStyle = T.rings.stroke;
    mc.beginPath();
    mc.arc(MINI / 2, MINI / 2, maxDist * sc, 0, Math.PI * 2);
    mc.stroke();
    if (display) {
      const step = Math.max(1, Math.floor(nodeCount / 3000));
      for (let i = 0; i < nodeCount; i += step) {
        const kd = display.kinds[i];
        if (kd === 0 || !isFinite(positions.xy[2 * i])) continue;
        const x = positions.xy[2 * i], y = positions.xy[2 * i + 1];
        if (Math.hypot(x, y) > ext) continue;
        mc.fillStyle = (T.kinds[String(kd) as '0'] as unknown as { fill?: string; fillSplit?: string[] }).fill ?? T.kinds['4'].fillSplit[1];
        mc.fillRect(MINI / 2 + x * sc - 0.75, MINI / 2 - y * sc - 0.75, 1.5, 1.5);
      }
    }
    const [wx0, wy0] = toWorld(camera, size.w, size.h, 0, 0);
    const [wx1, wy1] = toWorld(camera, size.w, size.h, size.w, size.h);
    mc.strokeStyle = T.highlight.selected;
    mc.lineWidth = 1.5;
    mc.strokeRect(MINI / 2 + wx0 * sc, MINI / 2 - wy0 * sc, (wx1 - wx0) * sc, (wy0 - wy1) * sc);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [size, camera, display, positions, maxDist, nodeCount]);

  // ---- 再生ループ ----
  useEffect(() => {
    if (!playing) return;
    let raf = 0;
    let prev = performance.now();
    const loop = (t: number) => {
      const dt = (t - prev) / 1000;
      prev = t;
      usePlayback.getState().advance(dt * usePlayback.getState().speed);
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  }, [playing]);

  // ---- 操作 ----
  const drag = useRef<{ x: number; y: number; cam: typeof camera; shift: boolean; moved: boolean } | null>(null);
  const localPos = (e: { clientX: number; clientY: number }) => {
    const r = overRef.current!.getBoundingClientRect();
    return [e.clientX - r.left, e.clientY - r.top] as const;
  };

  const expandRange = useCallback(async (ring: number, sector: number | null) => {
    try {
      const step = (Math.PI * 2) / N_SECTORS;
      const nodes = await sim.call('selectNodes', {
        ringLo: ring, ringHi: ring, angLo: sector === null ? 0 : sector * step, angHi: sector === null ? 7 : (sector + 1) * step,
      }, [], slot);
      useView.getState().expand(nodes, sector === null ? `${ring}ホップ` : `${ring}ホップ・扇形 ${sector + 1}`);
      useView.getState().setNotice(null);
    } catch (e) {
      useView.getState().setNotice(e instanceof Error ? e.message : String(e));
    }
  }, []);

  const expandUnreachable = useCallback(async () => {
    try {
      const nodes = await sim.call('selectUnreachable', {}, [], slot);
      useView.getState().expand(nodes, '到達不能');
      useView.getState().setNotice(null);
    } catch (e) {
      useView.getState().setNotice(e instanceof Error ? e.message : String(e));
    }
  }, []);

  const onPointerDown = (e: React.PointerEvent) => {
    const [x, y] = localPos(e);
    drag.current = { x, y, cam: camera, shift: e.shiftKey, moved: false };
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
  };
  const onPointerMove = (e: React.PointerEvent) => {
    const [x, y] = localPos(e);
    const d = drag.current;
    if (d) {
      if (Math.hypot(x - d.x, y - d.y) > 4) d.moved = true;
      if (d.shift && mode === 'expanded') {
        setRubber({ x0: d.x, y0: d.y, x1: x, y1: y });
      } else if (d.moved) {
        view().setCamera({ ...d.cam, cx: d.cam.cx - (x - d.x) / d.cam.zoom, cy: d.cam.cy + (y - d.y) / d.cam.zoom });
      }
    } else if (mode === 'overview') {
      const c = ov.cellAt(camera, size.w, size.h, x, y, nRings);
      setHover(c ? { ...c, x, y } : null);
    }
  };
  const onPointerUp = (e: React.PointerEvent) => {
    const [x, y] = localPos(e);
    const d = drag.current;
    drag.current = null;
    if (!d || !positions) return;
    if (d.shift && mode === 'expanded' && d.moved) {
      // 範囲選択 → 任意の群
      const x0 = Math.min(d.x, x), x1 = Math.max(d.x, x), y0 = Math.min(d.y, y), y1 = Math.max(d.y, y);
      const picked: number[] = [];
      for (const i of drawNodes) {
        const [sx, sy] = toScreen(camera, size.w, size.h, positions.xy[2 * i], positions.xy[2 * i + 1]);
        if (sx >= x0 && sx <= x1 && sy >= y0 && sy <= y1) picked.push(i);
      }
      setRubber(null);
      view().setGroup(Uint32Array.from(picked));
      return;
    }
    setRubber(null);
    if (d.moved) return;
    if (mode === 'expanded') {
      const hit = hitTest(positions.xy, drawNodes, camera, size.w, size.h, x, y, effectiveRadius(T.node.radiusPx, drawNodes.length));
      view().selectNode(hit >= 0 ? hit : null);
    } else {
      const [wx, wy] = toWorld(camera, size.w, size.h, x, y);
      const rect = ov.unreachableRect(maxDist);
      if ((ipInfo?.unreachable ?? 0) > 0 && wx >= rect.x && wx <= rect.x + rect.w && wy >= rect.y && wy <= rect.y + rect.h) {
        void expandUnreachable();
        return;
      }
      const c = ov.cellAt(camera, size.w, size.h, x, y, nRings);
      if (c) void expandRange(c.ring, c.sector);
    }
  };
  useEffect(() => {
    const el = overRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const r = el.getBoundingClientRect();
      view().setCamera(zoomAt(useView.getState().camera, size.w, size.h, e.clientX - r.left, e.clientY - r.top, Math.exp(-e.deltaY * 0.0015)));
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  }, [size, view]);

  const onMiniClick = (e: React.MouseEvent) => {
    const r = miniRef.current!.getBoundingClientRect();
    const sc = (MINI / 2 - 6) / (maxDist + 1.2);
    view().setCamera({ ...camera, cx: (e.clientX - r.left - MINI / 2) / sc, cy: -(e.clientY - r.top - MINI / 2) / sc });
  };

  const hoverCell = hover && sectors ? sectors.subarray((hover.ring * N_SECTORS + hover.sector) * 5, (hover.ring * N_SECTORS + hover.sector) * 5 + 5) : null;

  return (
      <div className="canvas-wrap" ref={wrapRef} data-slot={slot}>
        {label && <div className="slot-label">{label}</div>}
        <canvas ref={baseRef} className="layer" />
        <canvas ref={glRef} className="layer" />
        <canvas
          ref={overRef}
          className="layer interactive"
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerLeave={() => setHover(null)}
          data-testid="network-canvas"
        />
        {!positions && <div className="empty">グラフを読み込み、IP を選ぶと、ここに拡散が表示されます。</div>}
        {glError && <div className="empty error">{glError}</div>}
        {mode === 'expanded' && fromOverview && (
          <div className="expand-bar">
            <button onClick={() => useView.getState().backToOverview()}>← 全体図に戻る</button>
            <span>{expandedLabel}（{expanded?.length ?? 0} ノード）</span>
          </div>
        )}
        {mode === 'expanded' && !fromOverview && expanded === null && (ipInfo?.unreachable ?? 0) > 0 && (
          <div className="expand-bar">
            <span>IP から到達できないノード {ipInfo!.unreachable} 個は表示していません</span>
            <button onClick={() => void expandUnreachable()}>表示する</button>
          </div>
        )}
        {notice && <div className="notice">{notice}</div>}
        {edgesTruncated && mode === 'expanded' && <div className="notice small">辺は {EDGE_CAP.toLocaleString()} 本まで表示しています</div>}
        {mode === 'overview' && totals.length > 0 && (
          <div className="ring-table" aria-label="リングごとの数">
            <table>
              <thead><tr><th>リング</th><th>数</th><th>受信</th><th>内部</th><th>外部</th><th>両方</th><th /></tr></thead>
              <tbody>
                {totals.map((t, r) => r === 0 ? null : (
                  <tr key={r}>
                    <td>{r}ホップ</td><td>{t.n}</td><td>{t.recv}</td><td>{t.int}</td><td>{t.ext}</td><td>{t.both}</td>
                    <td><button onClick={() => void expandRange(r, null)}>展開</button></td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        {hover && hoverCell && hoverCell[0] > 0 && (
          <div className="tooltip" style={{ left: hover.x + 12, top: hover.y + 12 }}>
            {hover.ring}ホップ・扇形 {hover.sector + 1}: {hoverCell[0]} ノード / 受信 {hoverCell[1]} / 内部 {hoverCell[2]} / 外部 {hoverCell[3]} / 両方 {hoverCell[4]}
          </div>
        )}
        {positions && <canvas ref={miniRef} className="minimap" style={{ width: MINI, height: MINI }} onClick={onMiniClick} title="縮小地図（クリックで移動）" />}
        <div className="hud" data-testid="frame-ms">{frameMs.current.toFixed(1)} ms</div>
      </div>
  );
}

/** 凡例と再生の操作（A・B で共有）。 */
export function PlayControls() {
  const intMode = useView((st) => st.intMode);
  const playing = usePlayback((st) => st.playing);
  const k = usePlayback((st) => st.k);
  const last = usePlayback((st) => st.last);
  const speed = usePlayback((st) => st.speed);
  const playback = usePlayback.getState();
  return (
      <div className="controls">
        <div className="legend" aria-label="凡例">
          {KIND_LABELS.map((label, i) => <LegendItem key={label} kind={i} label={label} />)}
        </div>
        <div className="playbar">
          <button title="コマ戻し (←)" disabled={k <= -1} onClick={() => playback.stepWave(-1)}>◀|</button>
          <button className="primary" title="再生／一時停止 (Space)" disabled={last < 0} onClick={() => playback.toggle()}>{playing ? '⏸' : '▶'}</button>
          <button title="コマ送り (→)" disabled={k >= last} onClick={() => playback.stepWave(1)}>|▶</button>
          <button title="最後までスキップ" disabled={k >= last} onClick={() => playback.skipToEnd()}>⏭</button>
          <label>速度 <select value={speed} onChange={(e) => playback.setSpeed(Number(e.target.value))}>{SPEEDS.map((v) => <option key={v} value={v}>{v} ウェーブ/秒</option>)}</select></label>
          <span className="wave-label" data-testid="wave-label">{last < 0 ? '—' : `ウェーブ ${k + 1} / ${last + 1}`}</span>
          <label className="int-mode" title="内部活性の表示（F07）">
            内部活性:
            <select value={intMode} onChange={(e) => useView.getState().setIntMode(Number(e.target.value) as 0 | 1)}>
              <option value={0}>累積</option>
              <option value={1}>このラウンドのみ</option>
            </select>
          </label>
        </div>
      </div>
  );
}

function LegendItem({ kind, label }: { kind: number; label: string }) {
  const d = T.kinds[String(kind) as '0'] as { shape: string; fill?: string | null; stroke?: string | null; fillSplit?: string[] };
  const style: React.CSSProperties = {
    background: d.fillSplit ? `linear-gradient(90deg, ${d.fillSplit[0]} 50%, ${d.fillSplit[1]} 50%)` : (d.fill ?? 'transparent'),
    border: `2px solid ${d.stroke ?? (d.fill ?? 'transparent')}`,
    borderRadius: d.shape === 'circle' ? '50%' : '2px',
  };
  return <span className="legend-item"><i style={style} />{label}</span>;
}
