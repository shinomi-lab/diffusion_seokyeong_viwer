import { useEffect } from 'react';
import { ControlPanel, loadSample, sendSelected, usePaletteChoice } from './panels/ControlPanel';
import { NodeDetail } from './panels/NodeDetail';
import { RoundChart } from './panels/RoundChart';
import { Toolbar } from './panels/Toolbar';
import { NetworkView, PlayControls } from './render/NetworkView';
import { SeriesPanel } from './panels/SeriesPanel';
import { useCompare } from './store/actions';
import { usePlayback } from './store/playback';
import { useSession } from './store/session';

/** キーボード: 1〜6 でメッセージ選択、Enter で発信、Space で再生／一時停止、←→ で 1 ウェーブ戻る／進む（11.3）。 */
function useKeyboard() {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement;
      if (['INPUT', 'SELECT', 'TEXTAREA'].includes(t.tagName) || e.ctrlKey || e.metaKey || e.altKey) return;
      const pb = usePlayback.getState();
      const sc = useSession.getState().scenario;
      if (/^[1-9]$/.test(e.key)) {
        const n = (sc?.n_messages ?? 6);
        if (Number(e.key) <= n) usePaletteChoice.getState().set(Number(e.key) - 1);
      } else if (e.key === 'Enter') {
        // 「発信」ボタン自身は標準の動作に任せ、ほかのボタンにフォーカスがあっても発信する（11.3）
        if (t.dataset.testid === 'send') return;
        e.preventDefault();
        void sendSelected();
      } else if (e.key === ' ') {
        e.preventDefault();
        pb.toggle();
      } else if (e.key === 'ArrowLeft') {
        pb.stepWave(-1);
      } else if (e.key === 'ArrowRight') {
        pb.stepWave(1);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);
}

export default function App() {
  const init = useSession((s) => s.init);
  const ready = useSession((s) => s.ready);
  const compare = useCompare((c) => c.enabled);
  useKeyboard();
  useEffect(() => {
    void init();
    // 開発・検証用: ?sample=1 でサンプルを自動で読み込む
    if (new URLSearchParams(location.search).has('sample')) {
      const t = setInterval(() => {
        if (useSession.getState().ready) {
          clearInterval(t);
          void loadSample();
        }
      }, 100);
    }
  }, [init]);
  return (
    <div className="app">
      <Toolbar />
      {ready ? <ControlPanel /> : <aside className="control"><p className="hint">WASM を読み込み中…</p></aside>}
      <section className="network" aria-label="ネットワーク表示">
        <div className="views">
          {compare ? (
            <>
              <NetworkView slot={0} label="A" />
              <NetworkView slot={1} label="B" />
            </>
          ) : (
            <NetworkView slot={0} />
          )}
        </div>
        <PlayControls />
      </section>
      <NodeDetail />
      <div className="bottom">
        <RoundChart />
        <SeriesPanel />
      </div>
    </div>
  );
}
