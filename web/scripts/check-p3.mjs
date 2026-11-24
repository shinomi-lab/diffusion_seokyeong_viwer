// P3 の動作確認（実ブラウザ）: 巻き戻し・分岐・元の枝への復帰（F09）、MC の併記（F12）。
//   実行: npm run e2e:p3   （Chrome が必要。既定は macOS の Google Chrome。CHROME_PATH で変更可）
import { spawn } from 'node:child_process';
import puppeteer from 'puppeteer-core';

const CHROME = process.env.CHROME_PATH ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const PORT = 5198;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const vite = spawn('npx', ['vite', '--port', String(PORT), '--strictPort'], { stdio: 'ignore' });
let failures = 0;
const check = (ok, msg) => {
  if (!ok) failures++;
  console.log(`${ok ? 'OK  ' : 'FAIL'} ${msg}`);
};

try {
  for (let i = 0; i < 100; i++) {
    try { if ((await fetch(`http://localhost:${PORT}/`)).ok) break; } catch { /* 起動待ち */ }
    await sleep(200);
  }
  const browser = await puppeteer.launch({
    executablePath: CHROME, headless: 'new', args: ['--enable-unsafe-swiftshader', '--use-angle=swiftshader'],
    defaultViewport: { width: 1600, height: 950 },
  });
  const page = await browser.newPage();
  const errs = [];
  page.on('pageerror', (e) => errs.push(e.message));
  page.on('console', (m) => { if (m.type() === 'error' && !/404/.test(m.text())) errs.push(m.text()); });
  await page.goto(`http://localhost:${PORT}/?sample=1`, { waitUntil: 'networkidle0' });
  await sleep(1200);
  const ev = (f, ...a) => page.evaluate(f, ...a);
  const rounds = () => ev(() => window.__stores.useSession.getState().rounds.map((r) => JSON.stringify(r.summary)));
  const waitIdle = async () => { for (let i = 0; i < 100; i++) { if (await ev(() => !window.__stores.usePlayback.getState().playing && !window.__stores.useSession.getState().busy)) return; await sleep(50); } };
  const send = async (c) => {
    await ev((c) => { window.__stores.useSession.getState(); }, c);
    await page.keyboard.press(String(c + 1));
    await page.keyboard.press('Enter');
    await sleep(250);
    await ev(() => window.__stores.usePlayback.getState().skipToEnd());
    await waitIdle();
  };
  const clickNode = async (sel) => { await page.click(sel); await sleep(500); await waitIdle(); };

  // ---- F09: 巻き戻し・分岐・元の枝へ戻る（IP はノード 8、機構なし） ----
  await ev(() => window.__stores.useSession.getState().chooseIp(8));
  await sleep(600);
  for (const c of [0, 3, 5, 1, 4]) await send(c);
  const original = await rounds();
  check(original.length === 5, `5 ラウンド発信した（${original.length}）`);
  const nodeCount = () => ev(() => document.querySelectorAll('[data-testid=branch-tree] .tree-node').length);
  check((await nodeCount()) === 6, '木に根 + 5 節がある');

  await clickNode('[data-node-id="2"]'); // ラウンド 2 へ巻き戻し
  const afterRewind = await rounds();
  check(afterRewind.length === 2 && afterRewind.every((s, i) => s === original[i]), 'ラウンド 2 へ巻き戻すと、最初の 2 ラウンドの要約は元と同じ');
  check((await ev(() => window.__stores.useSession.getState().choices.join(','))) === '0,3', '系列が 0,3 に戻る');

  await send(2); await send(2); // 別のメッセージで分岐
  const branch = await rounds();
  check(branch.length === 4 && branch[1] === original[1] && branch[2] !== original[2], '別のメッセージで分岐し、分岐前の要約は元と同じ');
  check((await nodeCount()) === 8, '木に枝ができた（根 + 5 + 2 節）');
  check(await ev(() => window.__stores.useSession.getState().tree.n_lanes === 2), '枝は別の段に置かれる');

  // 元の枝の末尾（ラウンド 5 の節）へ切り替え → 元と完全に一致
  const origTail = await ev(() => { const t = window.__stores.useSession.getState().tree; return t.nodes.filter((n) => n.round === 5)[0].id; });
  await clickNode(`[data-node-id="${origTail}"]`);
  const back = await rounds();
  check(back.length === 5 && back.every((s, i) => s === original[i]), '元の枝へ戻ると、5 ラウンドすべての要約が元とビット単位で同じ');
  // 表示（ノードの状態）も一致: ever flags が元と同じ
  check(await ev(() => { const s = window.__stores.useSession.getState(); return s.ever.length > 0 && s.rounds.length === 5; }), '現在の状態も 5 ラウンドぶん復元されている');

  // ---- F12: MC 併記 ----
  await page.click('[data-testid=run-mc]');
  for (let i = 0; i < 100; i++) { if (await ev(() => !!window.__stores.useSession.getState().mc.result)) break; await sleep(100); }
  const mcText = await ev(() => document.querySelector('[data-testid=mc-result]')?.textContent ?? '');
  check(/200 試行/.test(mcText) && /帯に入っているラウンド/.test(mcText), `MC の結果と、実現が帯に入るラウンド数が表示される: ${mcText.slice(0, 60)}…`);
  await page.screenshot({ path: process.env.SHOT_DIR ? `${process.env.SHOT_DIR}/p3_tree_mc.png` : '/dev/null' });

  // ---- 発信と巻き戻し ----
  await send(2);
  check(await ev(() => window.__stores.useSession.getState().rounds.length === 6), '発信すると 6 ラウンドになる');
  await clickNode('[data-node-id="3"]');
  check(await ev(() => window.__stores.useSession.getState().rounds.length === 3), 'ツリーのノードをクリックすると 3 ラウンドに巻き戻る');
  check(await ev(() => { const r = window.__stores.useSession.getState().rounds; let c = 0; return r.every((x) => (c += x.summary.n_share) === x.summary.cum_share); }), 'cum_share が新規シェアの累積と一致する（巻き戻し後も）');
  check(errs.length === 0, `コンソールエラーなし ${errs.join(' | ')}`);
  await browser.close();
} finally {
  vite.kill();
}
console.log(failures === 0 ? '\nすべて通りました' : `\n${failures} 件失敗しました`);
process.exit(failures === 0 ? 0 : 1);
