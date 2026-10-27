import { useState } from 'preact/hooks';
import type { SettingsApi } from '../lib/ipc';
import type { General, SettingsView } from '../lib/types';
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
            <small>开启后角色不会主动打扰你</small>
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
    </section>
  );
}
