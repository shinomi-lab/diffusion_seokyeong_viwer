// ノード（WebGL）とエッジ・リング（Canvas2D）の位置が合っているかを、実ブラウザで確かめる。
// 初期表示・パン・ズームの後に、WebGL のピクセルが Canvas2D の計算位置に描かれていて、上下ミラーした位置には無いことを検証する。
// DPR 1 と 2（Retina）の両方で行う。Chrome が必要（既定は macOS の Google Chrome。CHROME_PATH で変更可）。
//   実行: npm run e2e:align
import { spawn } from 'node:child_process';
import puppeteer from 'puppeteer-core';

const CHROME = process.env.CHROME_PATH ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const PORT = 5199;
const URL = `http://localhost:${PORT}/?sample=1`;
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
  for (const dpr of [1, 2]) {
    const browser = await puppeteer.launch({
      executablePath: CHROME, headless: 'new',
      args: ['--enable-unsafe-swiftshader', '--use-angle=swiftshader', '--ignore-gpu-blocklist'],
      defaultViewport: { width: 1440, height: 900, deviceScaleFactor: dpr },
    });
    const page = await browser.newPage();
    await page.goto(URL, { waitUntil: 'networkidle0' });
    await sleep(1200);
    await page.click('[data-testid=ranking] button');
    await sleep(800);

    // Canvas2D と同じ式で、ノードのキャンバス内座標（CSS px、左上原点）を求める
    const nodeLocal = (node) => page.evaluate((n) => {
      const { useView, useSession } = window.__stores;
      const c = document.querySelector('[data-testid=network-canvas]').getBoundingClientRect();
      const cam = useView.getState().camera, xy = useSession.getState().positions.xy;
      return { x: (xy[2 * n] - cam.cx) * cam.zoom + c.width / 2, y: -(xy[2 * n + 1] - cam.cy) * cam.zoom + c.height / 2, w: c.width, h: c.height };
    }, node);
    const probe = (x, y) => page.evaluate((a, b) => window.__glProbe(a, b), x, y);
    const ip = await page.evaluate(() => window.__stores.useSession.getState().ip);
    const nNodes = await page.evaluate(() => window.__stores.useSession.getState().positions.xy.length / 2);

    const verify = async (label) => {
      // IP: 描かれているはず。上下ミラーした位置（キャンバス中心に対して）には何も無いはず
      const p = await nodeLocal(ip);
      const here = await probe(p.x, p.y);
      check(here && here.a > 0, `DPR${dpr} ${label}: IP の位置 (${p.x.toFixed(1)}, ${p.y.toFixed(1)}) にノードが描かれている`);
      if (Math.abs(p.y - p.h / 2) > 25) {
        const mirror = await probe(p.x, p.h - p.y);
        check(!mirror || mirror.a === 0, `DPR${dpr} ${label}: 上下ミラーした位置 (y=${(p.h - p.y).toFixed(1)}) には描かれていない`);
      }
      // 画面内の全ノードのうち、描かれていない（ずれている）ものの数
      let inView = 0, bad = 0;
      for (let n = 0; n < nNodes; n++) {
        // 到達不能のノードは通常は描かない（展開したときだけ描く）
        if (await page.evaluate((i) => window.__stores.useSession.getState().dist[i] === 0xffffffff, n)) continue;
        const q = await nodeLocal(n);
        if (q.x < 6 || q.y < 6 || q.x > q.w - 6 || q.y > q.h - 6) continue;
        inView++;
        const px = await probe(q.x, q.y);
        if (!px || px.a === 0) bad++;
      }
      check(inView > 0 && bad === 0, `DPR${dpr} ${label}: 画面内 ${inView} ノードのうち位置のずれ ${bad} 件`);
    };

    await verify('初期表示');
    const box = await (await page.$('[data-testid=network-canvas]')).boundingBox();
    const cx = box.x + box.width / 2, cy = box.y + box.height / 2;
    for (const [label, dx, dy] of [['縦にパン（下へ）', 0, 140], ['横・縦にパン（左上へ）', -110, -120]]) {
      await page.mouse.move(cx, cy);
      await page.mouse.down();
      await page.mouse.move(cx + dx / 2, cy + dy / 2, { steps: 5 });
      await page.mouse.move(cx + dx, cy + dy, { steps: 5 });
      await page.mouse.up();
      await sleep(250);
      await verify(label);
    }
    await page.mouse.move(cx + 60, cy - 40);
    await page.mouse.wheel({ deltaY: -400 });
    await sleep(300);
    await verify('ホイールでズームイン');
    await page.mouse.wheel({ deltaY: 700 });
    await sleep(300);
    await verify('ホイールでズームアウト');
    await browser.close();
  }
} finally {
  vite.kill();
}
console.log(failures === 0 ? '\nすべて一致しました' : `\n${failures} 件の不一致があります`);
process.exit(failures === 0 ? 0 : 1);
