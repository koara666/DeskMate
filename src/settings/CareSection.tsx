// 「陪伴」页里的日常关怀：久坐、喝水、深夜、吃饭。玩游戏、看视频、开会、全屏时会自动推迟。
import { useState } from 'preact/hooks';
import type { SettingsApi } from '../lib/ipc';
import type { CareSettings } from '../lib/types';
import { StatusLine, useStatus } from './SettingsApp';

export function CareSection({ api, initial }: { api: Pick<SettingsApi, 'saveCare'>; initial: CareSettings }) {
  const [c, setC] = useState<CareSettings>(initial);
  const { status, run } = useStatus();
  const save = async (next: CareSettings) => {
    setC(next);
    let saved: CareSettings | null = null;
    const ok = await run(async () => {
      saved = await api.saveCare(next);
    });
    if (ok && saved) setC(saved);
  };
  const check = (k: keyof CareSettings) => (e: Event) => save({ ...c, [k]: (e.currentTarget as HTMLInputElement).checked });
  const num = (e: Event) => Number((e.currentTarget as HTMLInputElement).value) || 0;

  return (
    <div class="card">
      <h2>日常关怀</h2>
      <p class="muted small">温和的小提醒，用预先写好的台词，不花钱。玩游戏、看视频、开会、全屏时会等一会儿再说。</p>
      <div class="row">
        <label>
          <input type="checkbox" checked={c.sit} onChange={check('sit')} /> 坐太久了提醒站起来走走、看看远处，每
        </label>
        <input type="number" min={30} max={240} value={c.sit_minutes} aria-label="久坐提醒间隔" onChange={(e) => save({ ...c, sit_minutes: num(e) })} />
        <span>分钟</span>
      </div>
      <div class="row">
        <label>
          <input type="checkbox" checked={c.water} onChange={check('water')} /> 喝水，每
        </label>
        <input type="number" min={60} max={360} value={c.water_minutes} aria-label="喝水提醒间隔" onChange={(e) => save({ ...c, water_minutes: num(e) })} />
        <span>分钟</span>
      </div>
      <div class="row">
        <label>
          <input type="checkbox" checked={c.late} onChange={check('late')} /> 过了
        </label>
        <input type="time" value={c.late_time} aria-label="深夜从几点算" onChange={(e) => save({ ...c, late_time: (e.currentTarget as HTMLInputElement).value })} />
        <span>还在电脑前，提醒早点睡（一晚一次）</span>
      </div>
      <label class="row">
        <input type="checkbox" checked={c.meal} onChange={check('meal')} />
        <span>到饭点提醒吃饭（午饭、晚饭各一次）</span>
      </label>
      <StatusLine status={status} />
    </div>
  );
}
