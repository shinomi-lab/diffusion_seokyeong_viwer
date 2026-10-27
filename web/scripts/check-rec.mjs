// 推薦の動作確認（実ブラウザ、T-R10）: 推薦の設定欄、推薦で受信したノードの黄色の塗り、凡例、時系列の系列、ノード詳細の表。
//   実行: npm run e2e:rec   （Chrome が必要。既定は macOS の Google Chrome。CHROME_PATH で変更可）
import { spawn } from 'node:child_process';
import { readFileSync } from 'node:fs';
import puppeteer from 'puppeteer-core';

const CHROME = process.env.CHROME_PATH ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const PORT = 5199;
const TOK = JSON.parse(readFileSync(new URL('../../spec/ui-tokens.json', import.meta.url), 'utf8'));
const REC = parseInt(TOK.highlight.recommended.slice(1), 16);
const [RR, RG, RB] = [(REC >> 16) & 255, (REC >> 8) & 255, REC & 255];
const CHAIN = parseInt(TOK.highlight.recChain.slice(1), 16);
const [CR, CG, CB] = [(CHAIN >> 16) & 255, (CHAIN >> 8) & 255, CHAIN & 255];
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
  check((await ev(() => document.querySelector('[data-testid=rec-method]')?.value)) === 'hybrid', '方式の既定は CF＋グラフ');
  check(!(await ev(() => document.querySelector('.legend')?.textContent ?? '')).includes('推薦で受信'), '推薦 off では凡例に推薦の項目を出さない');

  // 推薦 on・θ を低く（画面のチェックで on にし、θ はストア経由で設定）して適用
  await page.click('[data-testid=rec-enabled]');
  await ev(() => { const s = window.__stores.useSession.getState(); s.setForm({ recommender: { ...s.form.recommender, threshold: 0.05 } }); });
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
  await sleep(400);
  const legend = await ev(() => document.querySelector('.legend')?.textContent ?? '');
  check(legend.includes('推薦で受信') && legend.includes('推薦由来'), '凡例に「推薦で受信」「推薦由来」が出る');
  const series = await ev(() => document.querySelector('.round-chart')?.textContent ?? '');
  check(series.includes('推薦経由の外部行動') && series.includes('推薦で受信してシェア'), '時系列に推薦の 2 系列が出る');
  const simCells = await ev(() => document.querySelectorAll('[data-testid=rec-sim] td').length);
  check(simCells === 36, `種類どうしの似かたの表（6×6）が出る（${simCells} セル）`);

  // 色づけ: 推薦で受信したノードは中が黄色、フォロー経由のノードには黄色の画素が無い
  const nodes = await ev(() => {
    const rs = window.__stores.useSession.getState().rounds;
    const e = rs[rs.length - 1].events;
    const rec = [], follow = [], chain = [];
    for (let i = 0; i < e.node.length; i++) (e.flags[i] & 8 ? rec : e.flags[i] & 16 ? chain : follow).push(e.node[i]);
    return { rec, follow, chain };
  });
  const colorAt = (n, c) => ev((n, c) => {
    const { useView, useSession } = window.__stores;
    const box = document.querySelector('[data-testid=network-canvas]').getBoundingClientRect();
    const cam = useView.getState().camera, xy = useSession.getState().positions.xy;
    const x = (xy[2 * n] - cam.cx) * cam.zoom + box.width / 2, y = -(xy[2 * n + 1] - cam.cy) * cam.zoom + box.height / 2;
    for (let d = 0; d <= 14; d += 0.5) {
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
  // 推薦由来の連鎖: 縁取りはなく、中が暗い黄色（明るい黄色とは別の色）で塗られる
  if (nodes.chain.length) {
    let darkOk = 0, brightWrong = 0;
    for (const n of nodes.chain.slice(0, 15)) { if (await colorAt(n, [CR, CG, CB])) darkOk++; if (await ringAt(n)) brightWrong++; }
    const m = Math.min(15, nodes.chain.length);
    check(darkOk >= m - 1, `推薦由来のノードが暗い黄色に塗られている（${darkOk}/${m}）`);
    check(brightWrong <= 1, `推薦由来のノードは明るい黄色にならない（${brightWrong}/${m}、重なりの誤差 1 件まで）`);
  } else {
    console.log('SKIP 推薦由来のノードがこのラウンドに無い');
  }
  await page.screenshot({ path: process.env.SHOT_DIR ? `${process.env.SHOT_DIR}/rec_network.png` : '/dev/null' });

  // ノード詳細: 受信元「推薦」と推薦のスコアの表
  if (nodes.rec.length) {
    await ev((n) => window.__stores.useView.getState().selectNode(n), nodes.rec[0]);
    await sleep(800);
    const detail = await ev(() => [...document.querySelectorAll('.detail-body')].map((d) => d.textContent).join(' '));
    check(detail.includes('推薦') && (await ev(() => !!document.querySelector('[data-testid=rec-table]'))), 'ノード詳細に受信元「推薦」と推薦のスコアの表が出る');
    check(detail.includes('f_CF') && detail.includes('f_G'), 'ノード詳細の表に f_CF・f_G の列がある');
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

  // Rモデル（v3、T-W10）: 方式の選択、方式ごとのスライダーの有効・無効、推薦の推移の表
  const sliderDisabled = (label) => ev((label) => {
    for (const l of document.querySelectorAll('[data-testid=rec-editor] label.slider')) {
      if (l.textContent.includes(label)) return l.querySelector('input').disabled;
    }
    return null;
  }, label);
  await page.select('[data-testid=rec-method]', 'wave');
  await sleep(200);
  check((await sliderDisabled('推薦の強さ ε')) === false && (await sliderDisabled('閾値 θ')) === true, 'Rモデルでは ε が有効、閾値 θ が無効');
  await page.select('[data-testid=rec-method]', 'hybrid');
  await sleep(200);
  check((await sliderDisabled('推薦の強さ ε')) === true && (await sliderDisabled('CF の比率 α')) === false, 'CF＋グラフでは ε が無効、α が有効');
  await page.select('[data-testid=rec-method]', 'wave');
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
  const waveRows = await ev(() => document.querySelectorAll('[data-testid=rec-wave] tbody tr').length);
  check(waveRows > 0, `推薦の推移の表が出る（${waveRows} 行）`);
  // 既定の母集団（100 人、較正 T 5）では ē0 = 0 で推薦しない。その旨を表示する
  check(await ev(() => !!document.querySelector('[data-testid=rec-wave-zero]')), '基準 ē0 = 0 のとき「推薦しません」の注意が出る');
  // 較正 T を 3 にすると初回にシェアする人がいて、ラウンド 1 から推薦が起きる
  await ev(() => window.__stores.useSession.getState().setForm({ calibration_rounds: 3 }));
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
  const waveRec = await ev(() => { const rs = window.__stores.useSession.getState().rounds; return rs[rs.length - 1].summary.rec_received; });
  check(waveRec > 0, `Rモデル（較正 T 3）でラウンド 1 から推薦で受信する（${waveRec} 人）`);
  check(!(await ev(() => !!document.querySelector('[data-testid=rec-wave-zero]'))), 'ē0 > 0 では注意を出さない');
  // 推薦由来の連鎖が起きるので、縁取りなし・中が暗い黄色で塗られていることを画素で確かめる
  const chainNodes = await ev(() => {
    const rs = window.__stores.useSession.getState().rounds;
    const e = rs[rs.length - 1].events, out = [];
    for (let i = 0; i < e.node.length; i++) if (!(e.flags[i] & 8) && e.flags[i] & 16) out.push(e.node[i]);
    return out;
  });
  if (chainNodes.length) {
    let darkOk = 0, brightWrong = 0;
    for (const n of chainNodes.slice(0, 15)) { if (await colorAt(n, [CR, CG, CB])) darkOk++; if (await ringAt(n)) brightWrong++; }
    const m = Math.min(15, chainNodes.length);
    check(darkOk >= m - 1, `Rモデルの推薦由来のノードが暗い黄色に塗られている（${darkOk}/${m}）`);
    check(brightWrong <= 1, `Rモデルの推薦由来のノードは明るい黄色にならない（${brightWrong}/${m}、重なりの誤差 1 件まで）`);
  } else {
    check(false, 'Rモデルのラウンドに推薦由来のノードが無い');
  }
  check((await ev(() => document.querySelectorAll('[data-testid=rec-sim] td').length)) === 0, 'Rモデルでは似かたの表を出さない');
  await page.screenshot({ path: process.env.SHOT_DIR ? `${process.env.SHOT_DIR}/rec_wave.png` : '/dev/null' });

  check(errs.length === 0, `コンソールのエラーなし${errs.length ? `: ${errs[0]}` : ''}`);
  await browser.close();
} finally {
  vite.kill();
}
console.log(failures === 0 ? '\nすべて通りました' : `\n${failures} 件の失敗があります`);
process.exit(failures === 0 ? 0 : 1);
