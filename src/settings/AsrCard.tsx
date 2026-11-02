// 设置「声音」页里的「语音输入」卡片：对着麦克风说话，本机识别（中文模型），变成一条聊天消息。
// 默认关闭；第一次打开开关时自动下载识别模型（约 50 MB，之后离线可用），录音不上传、不保存。
import { useEffect, useState } from 'preact/hooks';
import type { UnlistenFn } from '@tauri-apps/api/event';
import type { AsrDownload, AsrSettings, AsrStatus } from '../lib/types';
import { StatusLine, useStatus } from './SettingsApp';

export interface AsrApi {
  asrSave(asr: AsrSettings): Promise<AsrSettings>;
  asrStatus(): Promise<AsrStatus>;
  asrDownload(): Promise<unknown>;
  onAsrDownload?(cb: (d: AsrDownload) => void): Promise<UnlistenFn>;
}

const mb = (n: number) => `${Math.round(n / 1048576)} MB`;

export function AsrCard({ api, initial }: { api: AsrApi; initial: AsrSettings }) {
  const [a, setA] = useState<AsrSettings>(initial);
  const [st, setSt] = useState<AsrStatus | null>(null);
  const { status, run } = useStatus();

  useEffect(() => {
    let alive = true;
    let off: UnlistenFn | null = null;
    api.asrStatus().then((s) => alive && setSt(s)).catch(() => {});
    api
      .onAsrDownload?.((d) => {
        if (!alive) return;
        setSt((s) => (s ? { ...s, download: d, model_ready: s.model_ready || (!d.running && !d.error && d.done > 0) } : s));
        // 下好了：再问一次，确认文件真的齐了
        if (!d.running && !d.error) api.asrStatus().then((s) => alive && setSt(s)).catch(() => {});
      })
      .then((u) => {
        if (alive) off = u;
        else u();
      })
      .catch(() => {});
    return () => {
      alive = false;
      off?.();
    };
  }, [api]);

  const save = async (next: AsrSettings) => {
    setA(next);
    let saved: AsrSettings | null = null;
    const ok = await run(async () => {
      saved = await api.asrSave(next);
    }, '已保存');
    if (ok && saved) setA(saved);
    // 刚打开开关会开始下载：刷新状态
    api.asrStatus().then(setSt).catch(() => {});
    if (!ok) setA((cur) => ({ ...cur, ...initial, ...a, hotkey_enabled: false }));
  };

  const dl = st?.download;
  return (
    <div class="card" aria-label="语音输入">
      <h2>语音输入</h2>
      <p class="muted small">
        对着麦克风直接跟她说话：在聊天框点麦克风按钮（或用快捷键）开始，说完停一下就会自动识别并发给她。识别在你自己的电脑上完成，
        录音不会上传，也不会保存。
      </p>
      <label class="row">
        <input
          type="checkbox"
          aria-label="打开语音输入"
          checked={a.enabled}
          onChange={(e) => save({ ...a, enabled: (e.currentTarget as HTMLInputElement).checked })}
        />
        <span>
          打开语音输入
          <small>第一次打开会自动下载识别模型（约 50 MB），之后离线可用</small>
        </span>
      </label>
      {a.enabled && (
        <>
          {st && !st.model_ready && (
            <div class="alert alert-warn" role="status">
              {dl?.running ? (
                <>
                  正在下载识别模型：{mb(dl.done)} / {mb(dl.total)}（只需要下载这一次）
                  <progress value={dl.done} max={Math.max(dl.total, 1)} aria-label="下载进度" />
                </>
              ) : dl?.error ? (
                <>
                  {dl.error}{' '}
                  <button type="button" class="btn btn-small" onClick={() => api.asrDownload().then(() => api.asrStatus().then(setSt))}>
                    重试
                  </button>
                </>
              ) : (
                '正在准备识别模型…'
              )}
            </div>
          )}
          {st?.model_ready && <p class="small">识别模型已就绪（中文，本机运行）。目前只认中文。</p>}
          <label class="row">
            <input
              type="checkbox"
              aria-label="识别完自动发送"
              checked={a.auto_send}
              onChange={(e) => save({ ...a, auto_send: (e.currentTarget as HTMLInputElement).checked })}
            />
            <span>
              识别完自动发送
              <small>关掉就先放进输入框，改完再发</small>
            </span>
          </label>
          <label class="row">
            <input
              type="checkbox"
              aria-label="连续对话"
              checked={a.continuous}
              onChange={(e) => save({ ...a, continuous: (e.currentTarget as HTMLInputElement).checked })}
            />
            <span>
              连续对话
              <small>她说完话自己接着听，不用再点；她说话时不收音，免得听到她自己</small>
            </span>
          </label>
          <div class="row">
            <span class="label">麦克风</span>
            <select aria-label="麦克风" value={a.input_device} onChange={(e) => save({ ...a, input_device: (e.currentTarget as HTMLSelectElement).value })}>
              <option value="">系统默认</option>
              {(st?.devices ?? []).map((d) => (
                <option key={d.id} value={d.id}>
                  {d.name}
                </option>
              ))}
            </select>
          </div>
          <label class="row">
            <input
              type="checkbox"
              aria-label="用快捷键开始说话"
              checked={a.hotkey_enabled}
              onChange={(e) => save({ ...a, hotkey_enabled: (e.currentTarget as HTMLInputElement).checked })}
            />
            <span>
              用快捷键开始 / 结束说话
              <small>不用先点开聊天框，按一下就开始听，再按一下结束</small>
            </span>
          </label>
          {a.hotkey_enabled && (
            <div class="row">
              <span class="label">快捷键</span>
              <input
                class="mono"
                aria-label="语音输入快捷键"
                value={a.hotkey}
                onChange={(e) => setA({ ...a, hotkey: (e.currentTarget as HTMLInputElement).value })}
                onBlur={() => save(a)}
              />
            </div>
          )}
          <p class="small">第一次用时，Windows 可能会问是否允许使用麦克风；在「系统设置 → 隐私 → 麦克风」里要允许桌面应用使用。</p>
        </>
      )}
      <StatusLine status={status} />
    </div>
  );
}
