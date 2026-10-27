// 推薦の編集（on/off・方式と、recParamSpecs から自動生成したスライダー）。A・B のどちらのストアにも使う（F13）。
import { recParamUsed, type RecForm, type RecMethod } from '../logic/config';
import type { createSessionStore } from '../store/session';

export function RecommenderEditor({ store }: { store: ReturnType<typeof createSessionStore> }) {
  const specs = store((s) => s.recSpecs);
  const rec = store((s) => s.form.recommender);
  const setForm = store((s) => s.setForm);
  const set = (patch: Partial<RecForm>) => setForm({ recommender: { ...rec, ...patch } });
  const on = rec.enabled;
  return (
    <fieldset className={on ? 'mech on' : 'mech'} data-testid="rec-editor">
      <legend>
        <label className="check" title="フォロー経由の連鎖のあと、スコアが閾値以上の未受信者にも届ける">
          <input type="checkbox" checked={on} onChange={(e) => set({ enabled: e.target.checked })} data-testid="rec-enabled" />推薦
        </label>
      </legend>
      <label className="row" title="基準: 本人の同じ種類の反応率。CF＋グラフ: 種類どうしの似かた（協調フィルタリング）とフォロー先のシェア（社会的証明）。Rモデル: 各ウェーブのあと、反応率に応じた数だけフォロー外へ届ける">
        方式
        <select disabled={!on} value={rec.method} onChange={(e) => set({ method: e.target.value as RecMethod })} data-testid="rec-method">
          <option value="hybrid">CF＋グラフ（v2）</option>
          <option value="baseline">基準（v1: 個人の反応率）</option>
          <option value="wave">Rモデル（v3: 波ごとの押し出し）</option>
        </select>
      </label>
      {specs.map((p) => {
        const key = p.id.split('.')[1] as keyof RecForm;
        const v = rec[key] as number;
        const usable = on && recParamUsed(rec.method, p.id);
        return (
          <label key={p.id} className="slider">
            <span>{p.label}<b>{v}</b></span>
            <input type="range" disabled={!usable} min={p.min ?? 0} max={p.max ?? 1} step={p.step ?? 0.01} value={v} onChange={(e) => set({ [key]: Number(e.target.value) })} />
          </label>
        );
      })}
    </fieldset>
  );
}
