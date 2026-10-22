// ツールバー: 設定の保存・読み込み、書き出し、現在ラウンド／T の表示（F01・F14）。
import { T as Tok } from '../tokens';
import { toConfigJson } from '../logic/config';
import { useState } from 'react';
import { downloadBlob, downloadText } from '../logic/units';
import { networkPng, recordPlayback, recordingSupported } from '../render/capture';
import { usePlayback } from '../store/playback';
import { useSession } from '../store/session';
import { sim } from '../worker/client';

async function exportWith(name: string, op: 'exportScenarioJson' | 'exportRoundsCsv' | 'exportEventsCsv', mime: string) {
  try {
    const text = await sim.call(op, {} as never);
    downloadText(name, text, mime);
  } catch (e) {
    useSession.setState({ error: e instanceof Error ? e.message : String(e) });
  }
}

export function Toolbar() {
  const [recording, setRecording] = useState(false);
  const bg = () => Tok.background;
  const rounds = useSession((s) => s.rounds);
  const form = useSession((s) => s.form);
  const ip = useSession((s) => s.ip);
  const scenario = useSession((s) => s.scenario);
  const T = (scenario?.config.max_rounds as number | undefined) ?? form.max_rounds;
  const has = ip !== null && rounds.length > 0;
  return (
    <header className="toolbar">
      <h1>情報拡散ライブビューア</h1>
      <div className="tools">
        <button onClick={() => downloadText('scenario-config.json', JSON.stringify(JSON.parse(toConfigJson(form)), null, 2), 'application/json')}>設定を保存</button>
        <label className="file-btn">設定を読み込む
          <input type="file" accept=".json" hidden onChange={async (e) => {
            const f = e.target.files?.[0];
            if (!f) return;
            try {
              const j = JSON.parse(await f.text());
              useSession.getState().loadConfig(j.config ?? j);
            } catch (err) {
              useSession.setState({ error: `設定を読み込めません: ${err instanceof Error ? err.message : err}` });
            }
            e.target.value = '';
          }} />
        </label>
        <label className="file-btn" title="書き出したシナリオ記録（JSON）を読み込み、設定・IP・分岐ツリー全体を復元する。同じグラフを先に読み込んでおく">記録を読み込む
          <input type="file" accept=".json" hidden data-testid="import-record" onChange={async (e) => {
            const f = e.target.files?.[0];
            if (!f) return;
            await useSession.getState().importRecord(await f.text());
            usePlayback.getState().setLast((() => { const r = useSession.getState().rounds; return r.length ? r[r.length - 1].summary.n_waves - 1 : -1; })());
            e.target.value = '';
          }} />
        </label>
        <span className="sep" />
        <button disabled={ip === null} title="ネットワーク図を PNG で保存（比較中は A と B を左右に並べる）" data-testid="network-png" onClick={() => void networkPng(bg()).then((b) => downloadBlob('network.png', b)).catch((e: Error) => useSession.setState({ error: e.message }))}>画像 (PNG)</button>
        <button disabled={!has || recording || !recordingSupported()} title="いまのラウンドの再生を WebM 動画に録画して保存" data-testid="record-webm" onClick={() => {
          setRecording(true);
          void recordPlayback(bg()).then((b) => downloadBlob('diffusion.webm', b)).catch((e: Error) => useSession.setState({ error: e.message })).finally(() => setRecording(false));
        }}>{recording ? '録画中…' : '録画 (WebM)'}</button>
        <span className="sep" />
        <button disabled={!has} onClick={() => void exportWith('scenario.json', 'exportScenarioJson', 'application/json')}>シナリオ記録 (JSON)</button>
        <button disabled={!has} onClick={() => void exportWith('rounds.csv', 'exportRoundsCsv', 'text/csv')}>ラウンド別 (CSV)</button>
        <button disabled={!has} onClick={() => void exportWith('events.csv', 'exportEventsCsv', 'text/csv')}>ノード別イベント (CSV)</button>
      </div>
      <div className="round-indicator" data-testid="round-indicator">ラウンド {rounds.length} / {T}</div>
    </header>
  );
}
