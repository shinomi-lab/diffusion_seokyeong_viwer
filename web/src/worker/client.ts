// メインスレッド側の窓口。要求に id を付け、応答を Promise で返す。
import type { OpName, Ops, Req, Res } from './protocol';

export class SimClient {
  private worker = new Worker(new URL('./sim.worker.ts', import.meta.url), { type: 'module' });
  private nextId = 1;
  private pending = new Map<number, { resolve: (v: unknown) => void; reject: (e: Error) => void }>();

  constructor() {
    this.worker.onmessage = (ev: MessageEvent<Res>) => {
      const r = ev.data;
      const p = this.pending.get(r.id);
      if (!p) return;
      this.pending.delete(r.id);
      if (r.ok) p.resolve(r.res);
      else p.reject(new Error(r.error));
    };
    this.worker.onerror = (ev) => {
      for (const p of this.pending.values()) p.reject(new Error(ev.message || 'Worker でエラーが発生しました'));
      this.pending.clear();
    };
  }

  call<K extends OpName>(op: K, args: Ops[K]['args'], transfer: Transferable[] = [], slot = 0): Promise<Ops[K]['res']> {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve: resolve as (v: unknown) => void, reject });
      this.worker.postMessage({ id, slot, op, args } as Req, transfer);
    });
  }
}

export const sim = new SimClient();
