// 推薦（Rモデル）の編集（on/off と、recParamSpecs から自動生成したスライダー）。
import type { RecForm } from '../logic/config';
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
        <label className="check" title="各ウェーブのあと、反応率に応じた数だけフォロー外へ届ける（Rモデル）">
          <input type="checkbox" checked={on} onChange={(e) => set({ enabled: e.target.checked })} data-testid="rec-enabled" />推薦
        </label>
      </legend>
      {specs.map((p) => {
        const key = p.id.split('.')[1] as keyof RecForm;
        const v = rec[key] as number;
        return (
          <label key={p.id} className="slider">
            <span>{p.label}<b>{v}</b></span>
            <input type="range" disabled={!on} min={p.min ?? 0} max={p.max ?? 1} step={p.step ?? 0.01} value={v} onChange={(e) => set({ [key]: Number(e.target.value) })} />
          </label>
        );
      })}
    </fieldset>
  );
}
