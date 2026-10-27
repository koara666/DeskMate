// 设置窗口：左侧导航 + 右侧页面。所有修改立即保存到 Rust 端。
import { useCallback, useEffect, useState } from 'preact/hooks';
import type { SettingsApi } from '../lib/ipc';
import type { SettingsView } from '../lib/types';
import { GeneralPage } from './GeneralPage';
import { ModelsPage } from './ModelsPage';
import { UsagePage } from './UsagePage';
import { AboutPage } from './AboutPage';

type Tab = 'general' | 'models' | 'usage' | 'about';

const TABS: { id: Tab; label: string }[] = [
  { id: 'general', label: '常规' },
  { id: 'models', label: '模型' },
  { id: 'usage', label: '用量与花费' },
  { id: 'about', label: '关于与隐私' },
];

export function SettingsApp({ api }: { api: SettingsApi }) {
  const [tab, setTab] = useState<Tab>('models');
  const [view, setView] = useState<SettingsView | null>(null);
  const [error, setError] = useState('');

  const reload = useCallback(async () => {
    try {
      setView(await api.get());
      setError('');
    } catch (e) {
      setError(String(e));
    }
  }, [api]);

  useEffect(() => {
    reload();
  }, [reload]);

  return (
    <div class="settings">
      <nav class="side-nav" aria-label="设置分类">
        <div class="brand">DeskMate</div>
        {TABS.map((t) => (
          <button
            type="button"
            key={t.id}
            class={`nav-item ${tab === t.id ? 'active' : ''}`}
            aria-current={tab === t.id ? 'page' : undefined}
            onClick={() => setTab(t.id)}
          >
            {t.label}
          </button>
        ))}
      </nav>
      <main class="page">
        {error && <div class="alert alert-error">{error}</div>}
        {!view ? (
          <div class="muted">加载中…</div>
        ) : tab === 'general' ? (
          <GeneralPage api={api} view={view} onChanged={reload} />
        ) : tab === 'models' ? (
          <ModelsPage api={api} view={view} onChanged={reload} />
        ) : tab === 'usage' ? (
          <UsagePage api={api} />
        ) : (
          <AboutPage api={api} view={view} />
        )}
      </main>
    </div>
  );
}

/** 把调用结果显示成一行提示 */
export function useStatus() {
  const [status, setStatus] = useState<{ ok: boolean; text: string } | null>(null);
  const run = async (fn: () => Promise<unknown>, okText = '已保存') => {
    try {
      await fn();
      setStatus({ ok: true, text: okText });
      return true;
    } catch (e) {
      setStatus({ ok: false, text: String(e) });
      return false;
    }
  };
  return { status, setStatus, run };
}

export function StatusLine({ status }: { status: { ok: boolean; text: string } | null }) {
  if (!status) return null;
  return (
    <span class={`status ${status.ok ? 'status-ok' : 'status-err'}`} role="status">
      {status.text}
    </span>
  );
}
