// 拡張機構（M1）の編集（プリセットと、paramSpecs から自動生成したスライダー）。A・B のどちらのストアにも使う（F13）。
import { useMemo } from 'react';
import { MECH_IDS, PRESET_LABELS, type MechId, type PresetName } from '../logic/config';
import type { createSessionStore } from '../store/session';
import type { ParamSpec } from '../worker/protocol';

export function MechanismEditor({ store }: { store: ReturnType<typeof createSessionStore> }) {
  const specs = store((s) => s.specs);
  const form = store((s) => s.form);
  const setForm = store((s) => s.setForm);
  const applyPreset = store((s) => s.applyPreset);

  const groups = useMemo(() => {
    const g = new Map<MechId, ParamSpec[]>();
    for (const p of specs) {
      const m = p.id.split('.')[0] as MechId;
      g.set(m, [...(g.get(m) ?? []), p]);
    }
    return g;
  }, [specs]);

  const setMech = (m: MechId, patch: Record<string, number | boolean>) =>
    setForm({ mechanisms: { ...form.mechanisms, [m]: { ...form.mechanisms[m], ...patch } } });

  return (
    <>
      <div className="presets">
        {(Object.keys(PRESET_LABELS) as PresetName[]).map((p) => <button key={p} onClick={() => applyPreset(p)}>{PRESET_LABELS[p]}</button>)}
      </div>
      {MECH_IDS.map((m) => {
        const ms = groups.get(m) ?? [];
        if (!ms.length) return null;
        const on = form.mechanisms[m].enabled;
        return (
          <fieldset key={m} className={on ? 'mech on' : 'mech'}>
            <legend><label className="check"><input type="checkbox" checked={on} onChange={(e) => setMech(m, { enabled: e.target.checked })} />{ms[0].mechanism}</label></legend>
            {ms.map((p) => {
              const key = p.id.split('.')[1];
              const v = form.mechanisms[m][key];
              if (p.kind === 'bool') {
                return <label key={p.id} className="check"><input type="checkbox" disabled={!on} checked={v === true} onChange={(e) => setMech(m, { [key]: e.target.checked })} />{p.label}</label>;
              }
              return (
                <label key={p.id} className="slider" title={`無効値: ${p.off}`}>
                  <span>{p.label}<b>{typeof v === 'number' ? v : ''}</b></span>
                  <input type="range" disabled={!on} min={p.min ?? 0} max={p.max ?? 1} step={p.step ?? 0.01} value={typeof v === 'number' ? v : p.default} onChange={(e) => setMech(m, { [key]: Number(e.target.value) })} />
                </label>
              );
            })}
          </fieldset>
        );
      })}
    </>
  );
}
