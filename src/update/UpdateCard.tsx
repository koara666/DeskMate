// 有新版本时的卡片（聊天气泡和设置「关于」页共用）：看看更新了什么 / 下载并安装 / 跳过这个版本。
// 只能安装后端上次从 GitHub 查到的那个版本；下载后后端校验 SHA-256，对不上就不装。
import { useEffect, useState } from 'preact/hooks';
import type { UpdateApi } from '../lib/ipc';
import type { UpdateProgress, UpdateView } from '../lib/types';
import '../styles/update.css';

const PHASE: Record<UpdateProgress['phase'], string> = {
  downloading: '正在下载',
  verifying: '正在校验',
  launching: '马上开始安装，DeskMate 会先关掉，装完自动打开',
  failed: '没装成',
};

export function UpdateCard({ api, view, onDone }: { api: UpdateApi; view: UpdateView; onDone?: () => void }) {
  const [showNotes, setShowNotes] = useState(false);
  const [progress, setProgress] = useState<UpdateProgress | null>(null);
  const [phase, setPhase] = useState<'ask' | 'skipped' | 'later'>('ask');
  useEffect(() => {
    const off = api.onProgress(setProgress);
    return () => {
      off.then((f) => f()).catch(() => {});
    };
  }, [api]);

  if (phase === 'skipped') return <div class="update-card done muted">（不再提示 {view.version}，想更新时到「设置 → 关于」里检查）</div>;
  if (phase === 'later') return <div class="update-card done muted">（好，下次再说）</div>;
  const busy = progress !== null && progress.phase !== 'failed';
  const install = () => {
    setProgress({ phase: 'downloading', percent: 0 });
    api.install().catch((e: unknown) => setProgress({ phase: 'failed', percent: 0, message: String(e) }));
  };
  return (
    <div class="update-card">
      <div class="update-title">
        ⬆ 新版本 <strong>{view.version}</strong>
        <span class="muted small">（现在是 {view.current}）</span>
      </div>
      {showNotes && <div class="update-notes">{view.notes || '这个版本没有写更新说明。'}</div>}
      {progress && (
        <div class={`update-progress ${progress.phase === 'failed' ? 'action-err' : ''}`} role="status">
          {PHASE[progress.phase]}
          {progress.phase === 'downloading' && ` ${progress.percent}%`}
          {progress.message && `：${progress.message}`}
          {progress.phase === 'downloading' && (
            <div class="update-bar">
              <span style={{ width: `${progress.percent}%` }} />
            </div>
          )}
        </div>
      )}
      {!busy && (
        <div class="update-buttons">
          <button type="button" class="btn btn-small btn-plain" onClick={() => setShowNotes((v) => !v)}>
            {showNotes ? '收起说明' : '看看更新了什么'}
          </button>
          {view.can_install && (
            <button type="button" class="btn btn-small" onClick={install}>
              下载并安装
            </button>
          )}
          <button
            type="button"
            class="btn btn-small btn-plain"
            onClick={() => {
              api.skip(view.version).catch(() => {});
              setPhase('skipped');
              onDone?.();
            }}
          >
            跳过这个版本
          </button>
          {onDone && (
            <button
              type="button"
              class="btn btn-small btn-plain"
              onClick={() => {
                setPhase('later');
                onDone();
              }}
            >
              以后再说
            </button>
          )}
        </div>
      )}
      {!view.can_install && <div class="muted small">这个版本需要到 GitHub 页面手动下载：{view.page}</div>}
    </div>
  );
}
