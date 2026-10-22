import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import App from './App';
import './styles.css';
import { usePlayback } from './store/playback';
import { useCompare } from './store/actions';
import { useSession, useSessionB } from './store/session';
import { useView } from './store/view';
import { applyChromeVars } from './tokens';

applyChromeVars();

// 開発・検証用: ブラウザのコンソールや自動テストから状態を確認できるようにする（本番ビルドには出さない）
if (import.meta.env.DEV) (window as unknown as { __stores: unknown }).__stores = { useView, useSession, useSessionB, usePlayback, useCompare };

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
