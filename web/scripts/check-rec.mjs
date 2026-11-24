// 推薦（Rモデル）の動作確認（実ブラウザ）: 推薦の設定欄、推薦で受信したノードの黄色の塗り、凡例、時系列の系列、推薦の推移の表。
//   実行: npm run e2e:rec   （Chrome が必要。既定は macOS の Google Chrome。CHROME_PATH で変更可）
import { spawn } from 'node:child_process';
import { readFileSync } from 'node:fs';
import puppeteer from 'puppeteer-core';

const CHROME = process.env.CHROME_PATH ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const PORT = 5199;
const TOK = JSON.parse(readFileSync(new URL('../../spec/ui-tokens.json', import.meta.url), 'utf8'));
const REC = parseInt(TOK.highlight.recommended.slice(1), 16);
const [RR, RG, RB] = [(REC >> 16) & 255, (REC >> 8) & 255, REC & 255];
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
  const waitIdle = async () => { for (let i = 0; i < 100; i++) { if (await ev(() => !window.__stores.usePlayback.getState().playing && !window.__stores.useSession.getState().busy)) return; await sleep(50); } };

  check(await ev(() => !!document.querySelector('[data-testid=rec-editor]')), '操作パネルに「推薦」の欄がある');
  const sliderDisabled = (label) => ev((label) => {
    for (const l of document.querySelectorAll('[data-testid=rec-editor] label.slider')) {
      if (l.textContent.includes(label)) return l.querySelector('input').disabled;
    }
    return null;
  }, label);
  check(!(await ev(() => !!document.querySelector('[data-testid=rec-method]'))), '方式の選択は無い（推薦は R モデルだけ）');
  check((await sliderDisabled('推薦の強さ ε')) === true, '推薦 off では ε のスライダーが無効');
  check(!(await ev(() => document.querySelector('.legend')?.textContent ?? '')).includes('推薦で受信'), '推薦 off では凡例に推薦の項目を出さない');

  // 推薦 on（画面のチェックで on にする）。既定の母集団（100 人、較正 T 5）では ē0 = 0 で推薦しない。その旨を表示する
  await page.click('[data-testid=rec-enabled]');
  check((await sliderDisabled('推薦の強さ ε')) === false, '推薦 on では ε のスライダーが有効');
  await ev(() => window.__stores.useSession.getState().applyForm());
  await waitIdle();
  await ev(() => window.__stores.useSession.getState().chooseIp(8));
  await sleep(600);
  await ev(() => document.activeElement?.blur());
  await page.keyboard.press('1');
  await page.keyboard.press('Enter');
  await sleep(250);
  await ev(() => window.__stores.usePlayback.getState().skipToEnd());
  await waitIdle();
  await sleep(400);
  check(await ev(() => !!document.querySelector('[data-testid=rec-wave-zero]')), '基準 ē0 = 0 のとき「推薦しません」の注意が出る');
  check((await ev(() => { const rs = window.__stores.useSession.getState().rounds; return rs[rs.length - 1].summary.rec_received; })) === 0, 'ē0 = 0 では推薦で受信する人がいない');
  // 較正 T を 3 にすると初回にシェアする人がいて、ラウンド 1 から推薦が起きる
  await ev(() => window.__stores.useSession.getState().setForm({ calibration_rounds: 3 }));
  await ev(() => window.__stores.useSession.getState().applyForm());
  await waitIdle();
  await ev(() => window.__stores.useSession.getState().chooseIp(8));
  await sleep(600);
  let recRound = 0;
  for (let r = 1; r <= 12 && !recRound; r++) {
    await ev(() => document.activeElement?.blur()); // 入力欄にフォーカスがあるとキー操作が効かない
    await page.keyboard.press('1'); // m0 を選んで発信（画面の操作と同じ経路）
    await page.keyboard.press('Enter');
    await sleep(250);
    await ev(() => window.__stores.usePlayback.getState().skipToEnd());
    await waitIdle();
    if (await ev(() => { const rs = window.__stores.useSession.getState().rounds; return rs[rs.length - 1].summary.rec_received > 0; })) recRound = r;
  }
  check(recRound > 0, `推薦で受信したラウンドがある（ラウンド ${recRound}）`);
  // 推薦は 2 ホップ先だけに届くので、推薦由来の連鎖（推薦で受信した人の下流）は、ラウンドを重ねてから起きる。起きるまで進める
  const hasChain = () => ev(() => { const rs = window.__stores.useSession.getState().rounds; const e = rs[rs.length - 1].events; return e.flags.some((f) => (f & 16) && !(f & 8)); });
  for (let r = 0; r < 12 && !(await hasChain()); r++) {
    await ev(() => document.activeElement?.blur());
    await page.keyboard.press('1');
    await page.keyboard.press('Enter');
    await sleep(250);
    await ev(() => window.__stores.usePlayback.getState().skipToEnd());
    await waitIdle();
  }
  await sleep(400);
  const legend = await ev(() => document.querySelector('.legend')?.textContent ?? '');
  check(legend.includes('推薦で受信'), '凡例に「推薦で受信」が出る');
  const series = await ev(() => document.querySelector('.round-chart')?.textContent ?? '');
  check(series.includes('推薦で受信') && series.includes('推薦で受信（累積人数）'), '時系列に「推薦で受信」の 2 系列が出る');
  check(!series.includes('推薦経由の外部行動') && !series.includes('推薦で受信してシェア'), '時系列に「推薦経由の外部行動」「推薦で受信してシェア」は出ない（系列を絞った）');
  check(['新規シェア', '新規外部行動', '累積外部行動'].every((s) => series.includes(s)), '時系列に新規シェア・新規外部行動・累積外部行動が出る');
  check(await ev(() => !!document.querySelector('[data-testid=rec-wave]')), '推薦の推移の表が出る');
  check(!(await ev(() => !!document.querySelector('[data-testid=rec-wave-zero]'))), 'ē0 > 0 では「推薦しません」の注意を出さない');

  // 色づけ: 推薦で受信したノードは中が黄色、フォロー経由のノードには黄色の画素が無い。
  // 前のラウンドに推薦で受け取った人は累積の表示で黄色のままなので、このラウンドだけの表示（「このラウンドのみ」）で確かめる
  await ev(() => window.__stores.useView.getState().setRecMode(1));
  await sleep(300);
  const nodes = await ev(() => {
    const rs = window.__stores.useSession.getState().rounds;
    const e = rs[rs.length - 1].events;
    const rec = [], follow = [];
    for (let i = 0; i < e.node.length; i++) (e.flags[i] & 8 ? rec : e.flags[i] & 24 ? [] : follow).push(e.node[i]);
    return { rec, follow };
  });
  const colorAt = (n, c) => ev((n, c) => {
    const { useView, useSession } = window.__stores;
    const box = document.querySelector('[data-testid=network-canvas]').getBoundingClientRect();
    const cam = useView.getState().camera, xy = useSession.getState().positions.xy;
    const x = (xy[2 * n] - cam.cx) * cam.zoom + box.width / 2, y = -(xy[2 * n + 1] - cam.cy) * cam.zoom + box.height / 2;
    for (let d = 0; d <= 3; d += 0.5) {   // 中の塗りだけを見る。広く探すと、近くの別のノード（2 ホップ先の推薦は近くに集まる）を拾う
      for (const [dx, dy] of [[d, 0], [-d, 0], [0, d], [0, -d]]) {
        const p = window.__glProbe(x + dx, y + dy);
        if (p && p.a > 200 && Math.abs(p.r - c[0]) < 24 && Math.abs(p.g - c[1]) < 24 && Math.abs(p.b - c[2]) < 24) return true;
      }
    }
    return false;
  }, n, c);
  const ringAt = (n) => colorAt(n, [RR, RG, RB]);
  let withRing = 0;
  for (const n of nodes.rec.slice(0, 15)) if (await ringAt(n)) withRing++;
  check(withRing >= Math.min(15, nodes.rec.length) - 1, `推薦で受信したノードが黄色に塗られている（${withRing}/${Math.min(15, nodes.rec.length)}）`);
  let followRing = 0;
  for (const n of nodes.follow.slice(0, 15)) if (await ringAt(n)) followRing++;
  check(followRing <= 1, `フォロー経由のノードは黄色にならない（${followRing}/${Math.min(15, nodes.follow.length)}、重なりの誤差 1 件まで）`);
  await ev(() => window.__stores.useView.getState().setRecMode(0));
  await sleep(300);
  await page.screenshot({ path: process.env.SHOT_DIR ? `${process.env.SHOT_DIR}/rec_network.png` : '/dev/null' });

  // ノード詳細: 受信元「推薦」と、推薦で受け取ったラウンド
  if (nodes.rec.length) {
    await ev((n) => window.__stores.useView.getState().selectNode(n), nodes.rec[0]);
    await sleep(800);
    const detail = await ev(() => [...document.querySelectorAll('.detail-body')].map((d) => d.textContent).join(' '));
    check(detail.includes('推薦') && (await ev(() => !!document.querySelector('[data-testid=rec-rounds]'))), 'ノード詳細に受信元「推薦」と「推薦で受け取ったラウンド」が出る');
    check(!(await ev(() => !!document.querySelector('[data-testid=rec-table]'))), 'ノード詳細に推薦のスコアの表は出ない（個人のスコアを持たない）');
  }
  // 累積の記録: 推薦で受け取った人は、次のラウンドも（累積なら）黄色のまま。「このラウンドのみ」なら消える
  const before = nodes.rec.slice();
  await ev(() => document.activeElement?.blur());
  await page.keyboard.press('2');
  await page.keyboard.press('Enter');
  await sleep(250);
  await ev(() => window.__stores.usePlayback.getState().skipToEnd());
  await waitIdle();
  await sleep(300);
  const nowRec = await ev(() => {
    const rs = window.__stores.useSession.getState().rounds;
    const e = rs[rs.length - 1].events;
    const out = [];
    for (let i = 0; i < e.node.length; i++) if (e.flags[i] & 24) out.push(e.node[i]);
    return out;
  });
  const old = before.filter((n) => !nowRec.includes(n)).slice(0, 10);
  if (old.length) {
    let keep = 0;
    for (const n of old) if (await ringAt(n)) keep++;
    check(keep >= old.length - 1, `累積: 前のラウンドに推薦で受け取った人が黄色のまま（${keep}/${old.length}）`);
    await ev(() => window.__stores.useView.getState().setRecMode(1));
    await sleep(300);
    let gone = 0;
    for (const n of old) if (!(await ringAt(n))) gone++;
    check(gone >= old.length - 1, `このラウンドのみ: 前のラウンドの推薦の色が消える（${gone}/${old.length}）`);
    await ev(() => window.__stores.useView.getState().setRecMode(0));
  }
  const summary = await ev(() => { const rs = window.__stores.useSession.getState().rounds; return rs[rs.length - 1].summary.cum_rec; });
  check(summary > 0, `ラウンド要約に推薦で受け取ったことのある人数（累積 ${summary} 人）`);
  const series2 = await ev(() => document.querySelector('.round-chart')?.textContent ?? '');
  check(series2.includes('推薦で受信（累積人数）'), '時系列に「推薦で受信」「推薦で受信（累積人数）」が出る');
  check(await ev(() => !!document.querySelector('[data-testid=rec-mode]')), '再生の操作に「推薦: 累積／このラウンドのみ」がある');

  await page.screenshot({ path: process.env.SHOT_DIR ? `${process.env.SHOT_DIR}/rec_wave.png` : '/dev/null' });

  check(errs.length === 0, `コンソールのエラーなし${errs.length ? `: ${errs[0]}` : ''}`);
  await browser.close();
} finally {
  vite.kill();
}
console.log(failures === 0 ? '\nすべて通りました' : `\n${failures} 件の失敗があります`);
process.exit(failures === 0 ? 0 : 1);
