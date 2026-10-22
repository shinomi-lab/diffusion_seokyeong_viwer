import react from '@vitejs/plugin-react';
import { defineConfig } from 'vitest/config';

// 静的ファイル一式として配布する（サーバ不要・オフライン可。N05・N06）。
export default defineConfig({
  base: './',
  plugins: [react()],
  worker: { format: 'es' },
  server: { fs: { allow: ['..'] } },
  test: { environment: 'node', include: ['src/**/*.test.ts'] },
});
