// 2 画面比較（F13）: 設定だけを変えた 2 つのシナリオ（A・B）を、同じグラフ・同じ IP・同じシード・同じ選択で左右に並べる。
import { applyB, setCompare, useCompare } from '../store/actions';
import { isDirty, useSession, useSessionB } from '../store/session';
import { MechanismEditor } from './MechanismEditor';

export function ComparePanel() {
  const enabled = useCompare((c) => c.enabled);
  const hasGraph = useSession((s) => s.graph !== null);
  const b = useSessionB();
  const dirtyB = isDirty(b);
  return (
    <details open>
      <summary>2 画面比較（A と B）</summary>
      <label className="check" title="同じグラフ・IP・シード・選択で、機構の設定だけを変えた B を左右に並べる">
        <input type="checkbox" disabled={!hasGraph} checked={enabled} onChange={(e) => void setCompare(e.target.checked)} />比較モード
      </label>
      {enabled && (
        <div data-testid="compare-b">
          <p className="note">B の設定（機構）。A と同じ IP・同じ選択が B にも適用されます。</p>
          <MechanismEditor store={useSessionB} />
          <button className={dirtyB ? 'primary' : ''} disabled={!b.scenario} onClick={() => void applyB()} data-testid="apply-b">
            B に設定を適用{dirtyB && ' ●未適用'}
          </button>
          {b.error && <p className="error">{b.error}</p>}
        </div>
      )}
    </details>
  );
}
