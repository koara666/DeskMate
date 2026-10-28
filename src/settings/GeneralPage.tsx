import { useState } from 'preact/hooks';
import type { SettingsApi } from '../lib/ipc';
import type { DailySettings, General, SettingsView } from '../lib/types';
import { StatusLine, useStatus } from './SettingsApp';

interface Props {
  api: SettingsApi;
  view: SettingsView;
  onChanged: () => void;
}

export function GeneralPage({ api, view, onChanged }: Props) {
  const [g, setG] = useState<General>(view.general);
  const { status, run } = useStatus();

  const save = async (next: General) => {
    setG(next);
    if (await run(() => api.saveGeneral(next))) onChanged();
  };

  return (
    <section>
      <h1>常规</h1>
      <div class="card">
        <label class="row">
          <input
            type="checkbox"
            checked={g.always_on_top}
            onChange={(e) => save({ ...g, always_on_top: (e.currentTarget as HTMLInputElement).checked })}
          />
          <span>
            总在最前
            <small>桌宠和聊天窗口显示在其他窗口上面</small>
          </span>
        </label>
        <label class="row">
          <input
            type="checkbox"
            checked={g.idle_animation}
            onChange={(e) => save({ ...g, idle_animation: (e.currentTarget as HTMLInputElement).checked })}
          />
          <span>
            待机小动画
            <small>偶尔呼吸、眨眼。关掉更省电</small>
          </span>
        </label>
        <label class="row">
          <input
            type="checkbox"
            checked={g.dnd}
            onChange={(e) => save({ ...g, dnd: (e.currentTarget as HTMLInputElement).checked })}
          />
          <span>
            勿扰模式
            <small>开启后角色不会主动说话（你设的提醒除外）。更细的设置在「陪伴」页</small>
          </span>
        </label>
        <div class="row">
          <span class="label">桌宠大小</span>
          <input
            type="range"
            min={0.6}
            max={2}
            step={0.1}
            value={g.pet_scale}
            aria-label="桌宠大小"
            onChange={(e) => save({ ...g, pet_scale: Number((e.currentTarget as HTMLInputElement).value) })}
          />
          <span class="mono">{Math.round(g.pet_scale * 100)}%</span>
        </div>
      </div>
      <StatusLine status={status} />
      <DailySection api={api} initial={view.daily} />
    </section>
  );
}

function DailySection({ api, initial }: { api: SettingsApi; initial: DailySettings }) {
  const [d, setD] = useState<DailySettings>(initial);
  const { status, run } = useStatus();
  const save = (next: DailySettings) => {
    setD(next);
    run(() => api.saveDaily(next));
  };
  const check = (k: keyof DailySettings) => (e: Event) => save({ ...d, [k]: (e.currentTarget as HTMLInputElement).checked });
  return (
    <>
      <div class="card">
        <h2>启动与快捷键</h2>
        <label class="row">
          <input type="checkbox" checked={d.autostart} onChange={check('autostart')} />
          <span>
            开机自动启动
            <small>默认关闭</small>
          </span>
        </label>
        <label class="row">
          <input type="checkbox" checked={d.hotkey_enabled} onChange={check('hotkey_enabled')} />
          <span>
            全局快捷键：随时唤出 / 收起聊天框
            <small>格式比如 Alt+Shift+D、Ctrl+Alt+K；被其他程序占用时会提示</small>
          </span>
        </label>
        <div class="row">
          <span class="label">快捷键</span>
          <input value={d.hotkey} aria-label="快捷键" onInput={(e) => setD({ ...d, hotkey: (e.currentTarget as HTMLInputElement).value })} />
          <button type="button" class="btn btn-small" onClick={() => save(d)}>
            应用
          </button>
        </div>
      </div>
      <div class="card">
        <h2>不打扰</h2>
        <label class="row">
          <input type="checkbox" checked={d.hide_in_fullscreen} onChange={check('hide_in_fullscreen')} />
          <span>
            玩游戏、看全屏视频、演示时自动隐藏
            <small>退出全屏后自动回来</small>
          </span>
        </label>
        <div class="row">
          <span class="label">多久没操作就睡觉</span>
          <select
            aria-label="睡觉时间"
            value={d.sleep_minutes}
            onChange={(e) => save({ ...d, sleep_minutes: Number((e.currentTarget as HTMLSelectElement).value) })}
          >
            {[0, 5, 10, 20, 30, 60].map((m) => (
              <option key={m} value={m}>
                {m === 0 ? '不睡觉' : `${m} 分钟`}
              </option>
            ))}
          </select>
        </div>
        <StatusLine status={status} />
      </div>
    </>
  );
}
