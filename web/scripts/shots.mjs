// 見た目の確認用スクリーンショット（通常/比較）と、再生中のフレーム時間。配色はダーク専用なので、OS がライトの状態をまねて撮り、ダークで出ることも確かめる。
//   実行: node scripts/shots.mjs OUT_DIR
import { spawn } from 'node:child_process';
import { mkdirSync } from 'node:fs';
import puppeteer from 'puppeteer-core';

const OUT = process.argv[2] ?? 'shots';
mkdirSync(OUT, { recursive: true });
const CHROME = process.env.CHROME_PATH ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const PORT = 5197;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const vite = spawn('npx', ['vite', '--port', String(PORT), '--strictPort'], { stdio: 'ignore' });
try {
  for (let i = 0; i < 100; i++) {
    try { if ((await fetch(`http://localhost:${PORT}/`)).ok) break; } catch { /* 起動待ち */ }
    await sleep(200);
  }
  const browser = await puppeteer.launch({
    executablePath: CHROME, headless: 'new', args: ['--enable-unsafe-swiftshader', '--use-angle=swiftshader'],
    defaultViewport: { width: 1600, height: 950 },
  });
  for (const scheme of ['light']) {
    const page = await browser.newPage();
    await page.emulateMediaFeatures([{ name: 'prefers-color-scheme', value: scheme }]);
    await page.goto(`http://localhost:${PORT}/?sample=1`, { waitUntil: 'networkidle0' });
    await sleep(1200);
    const ev = (f, ...a) => page.evaluate(f, ...a);
    const waitIdle = async () => { for (let i = 0; i < 100; i++) { if (await ev(() => !window.__stores.usePlayback.getState().playing && !window.__stores.useSession.getState().busy)) return; await sleep(50); } };
    await page.waitForSelector('.ranking button', { timeout: 20000 });
    await page.click('.ranking button');
    await sleep(600);
    for (const c of [0, 3, 5, 1, 2, 4, 0, 5, 3, 1]) {
      await page.keyboard.press(String(c + 1));
      await page.keyboard.press('Enter');
      await sleep(250);
      await ev(() => window.__stores.usePlayback.getState().skipToEnd());
      await waitIdle();
    }
    // 再生中のフレーム時間（rAF の間隔。swiftshader なので絶対値でなく変更前後の比較に使う）
    await page.keyboard.press('3');
    await page.keyboard.press('Enter');
    const ft = await ev(() => new Promise((res) => {
      const t = []; let last = performance.now();
      const f = (now) => { t.push(now - last); last = now; if (t.length < 120) requestAnimationFrame(f); else res(t); };
      requestAnimationFrame(f);
    }));
    ft.sort((a, b) => a - b);
    console.log(`${scheme} frame p50=${ft[60].toFixed(1)}ms p95=${ft[114].toFixed(1)}ms`);
    await ev(() => window.__stores.usePlayback.getState().skipToEnd());
    await waitIdle();
    await page.screenshot({ path: `${OUT}/web_${scheme}.png` });
    const box = await page.evaluateHandle(() => [...document.querySelectorAll('label.check')].find((l) => l.textContent.includes('比較モード')));
    await box.asElement().click();
    await sleep(1500);
    await waitIdle();
    await page.screenshot({ path: `${OUT}/web_${scheme}_compare.png` });
    await page.close();
  }
  await browser.close();
} finally {
  vite.kill();
}
