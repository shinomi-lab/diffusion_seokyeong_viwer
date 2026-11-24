// パネルの表示・非表示（左・右・下）で、ネットワーク図が隠した側へ広がることの確認（実ブラウザ）。
//   実行: npm run e2e:panels   （Chrome が必要。既定は macOS の Google Chrome。CHROME_PATH で変更可）
import { spawn } from 'node:child_process';
import puppeteer from 'puppeteer-core';

const CHROME = process.env.CHROME_PATH ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const PORT = 5197;
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
  await ev(() => window.__stores.useSession.getState().chooseIp(8));
  await sleep(600);
  // 凡例で隠す系列を 1 つ作っておく（下グラフを隠して戻しても残るか）
  const legendOff = () => ev(() => document.querySelectorAll('.u-legend .u-series.u-off').length);
  await page.click('.u-legend .u-series:nth-child(2) th');
  await sleep(200);
  const off0 = await legendOff();

  const size = () => ev(() => { const r = document.querySelector('.canvas-wrap').getBoundingClientRect(); return { x: r.x, y: r.y, w: r.width, h: r.height, b: r.bottom, r: r.right }; });
  const toggle = async (n) => { await page.click(`[data-testid=toggle-${n}]`); await sleep(500); };
  const base = await size();
  const near = (a, b) => Math.abs(a - b) <= 3;

  await toggle('left');
  let s = await size();
  check(near(s.w - base.w, 280) && near(s.x, 0) && near(s.r, base.r) && near(s.h, base.h), `左を隠すと図が左へ 280px 広がる（幅 +${Math.round(s.w - base.w)}）`);
  await toggle('left');
  await toggle('right');
  s = await size();
  check(near(s.w - base.w, 340) && near(s.x, base.x) && near(s.h, base.h), `右を隠すと図が右へ 340px 広がる（幅 +${Math.round(s.w - base.w)}）`);
  await toggle('right');
  await toggle('bottom');
  s = await size();
  check(s.h - base.h > 200 && near(s.w, base.w) && near(s.y, base.y), `下を隠すと図が下へ広がる（高さ +${Math.round(s.h - base.h)}）`);
  check(await ev(() => getComputedStyle(document.querySelector('.bottom')).display === 'none'), '下を隠すと、下グラフと選んだ系列（.bottom）が消える');
  await toggle('left');
  await toggle('right');
  s = await size();
  check(s.w - base.w > 600 && s.h - base.h > 200 && near(s.x, 0), `3 つとも隠すと図が最大になる（${Math.round(s.w)}×${Math.round(s.h)}）`);
  // キーボード: [ ] \ で全部戻す
  for (const k of ['[', ']', '\\']) await page.keyboard.press(k);
  await sleep(500);
  s = await size();
  check(near(s.w, base.w) && near(s.h, base.h) && near(s.x, base.x), 'キー [ ] \\ で元の配置に戻る');
  check((await legendOff()) === off0 && off0 > 0, `凡例で隠した系列は、パネルを隠して戻しても残る（${off0} 件）`);
  check(errs.length === 0, `コンソールエラーなし ${errs.join(' | ')}`);
  await browser.close();
} finally {
  vite.kill();
}
console.log(failures ? `\n${failures} 件失敗` : '\nすべて通りました');
process.exit(failures ? 1 : 0);
