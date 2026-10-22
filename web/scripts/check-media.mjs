// 画像・動画の書き出し（F15）の確認: ネットワーク図の PNG、再生の WebM 録画、時系列の PNG / SVG。
//   実行: npm run e2e:media   （Chrome が必要。動画の長さの確認に ffprobe があれば使う）
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
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
const waitFile = async (path, ms = 40000) => {
  for (let i = 0; i < ms / 200; i++) {
    if (existsSync(path) && !readdirSync(join(path, '..')).some((f) => f.endsWith('.crdownload'))) return true;
    await sleep(200);
  }
  return false;
};

try {
  for (let i = 0; i < 100; i++) {
    try { if ((await fetch(`http://localhost:${PORT}/`)).ok) break; } catch { /* 起動待ち */ }
    await sleep(200);
  }
  const dir = mkdtempSync(join(tmpdir(), 'viewer-media-'));
  const browser = await puppeteer.launch({
    executablePath: CHROME, headless: 'new', args: ['--enable-unsafe-swiftshader', '--use-angle=swiftshader'],
    defaultViewport: { width: 1600, height: 950 },
  });
  const page = await browser.newPage();
  const cdp = await page.createCDPSession();
  await cdp.send('Browser.setDownloadBehavior', { behavior: 'allow', downloadPath: dir });
  const errs = [];
  page.on('pageerror', (e) => errs.push(e.message));
  await page.goto(`http://localhost:${PORT}/?sample=1`, { waitUntil: 'networkidle0' });
  await sleep(1200);
  const ev = (f, ...a) => page.evaluate(f, ...a);
  await ev(() => window.__stores.useSession.getState().chooseIp(8));
  await sleep(600);
  for (const k of ['1', '4', '6']) {
    await page.keyboard.press(k);
    await page.keyboard.press('Enter');
    await sleep(250);
    await ev(() => window.__stores.usePlayback.getState().skipToEnd());
    await sleep(150);
  }

  // ネットワーク図 PNG
  await page.click('[data-testid=network-png]');
  check(await waitFile(join(dir, 'network.png')), 'ネットワーク図の PNG が保存される');
  const png = readFileSync(join(dir, 'network.png'));
  check(png.subarray(0, 8).toString('hex') === '89504e470d0a1a0a' && png.length > 5000, `PNG として正しい（${png.length} バイト）`);

  // 時系列 PNG・SVG
  await page.click('[data-testid=round-counts-svg]');
  check(await waitFile(join(dir, 'round-counts.svg')), '時系列の SVG が保存される');
  const svg = readFileSync(join(dir, 'round-counts.svg'), 'utf8');
  check(svg.startsWith('<svg') && svg.includes('新規シェア') && svg.includes('累積外部行動') && !svg.includes('NaN'), 'SVG に系列名とデータが入っている');
  await page.click('[data-testid=round-utility-png]');
  check(await waitFile(join(dir, 'round-utility.png')), '効用グラフの PNG が保存される');

  // WebM 録画（再生を頭から流す）
  const t0 = Date.now();
  await page.click('[data-testid=record-webm]');
  check(await waitFile(join(dir, 'diffusion.webm')), `再生を WebM に録画して保存できる（${((Date.now() - t0) / 1000).toFixed(1)} 秒）`);
  const webm = join(dir, 'diffusion.webm');
  if (existsSync(webm)) {
    const head = readFileSync(webm).subarray(0, 4).toString('hex');
    check(head === '1a45dfa3' && statSync(webm).size > 3000, `WebM（EBML ヘッダー）として正しい（${statSync(webm).size} バイト）`);
    const pr = spawnSync('ffprobe', ['-v', 'error', '-show_entries', 'stream=codec_name,width,height,nb_read_frames:format=duration', '-count_frames', '-of', 'default=nw=1', webm], { encoding: 'utf8' });
    if (pr.status === 0) {
      console.log(pr.stdout.trim().split('\n').join(' / '));
      // MediaRecorder の WebM は長さ（duration）のヘッダーを持たない（再生は可能）。フレーム数で確かめる
      const frames = Number(/nb_read_frames=(\d+)/.exec(pr.stdout)?.[1] ?? '0');
      check(frames >= 10, `ffprobe で動画として読め、フレームがある（${frames} フレーム）`);
    } else console.log('（ffprobe が無いので長さの確認は省略）');
  }
  check(errs.length === 0, `エラーなし ${errs.join(' | ')}`);
  console.log(`書き出し先: ${dir}`);
  await browser.close();
} finally {
  vite.kill();
}
console.log(failures === 0 ? '\nすべて通りました' : `\n${failures} 件失敗しました`);
process.exit(failures === 0 ? 0 : 1);
