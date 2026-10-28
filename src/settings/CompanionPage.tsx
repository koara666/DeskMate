// 「陪伴」：所有主动行为的总控（DECISIONS #35）。具体功能（播报、关怀、活动评论……）的开关随里程碑加进来。
import { useState } from 'preact/hooks';
import type { SettingsApi } from '../lib/ipc';
import type { CompanionSettings, DailySettings, Frequency, SettingsView } from '../lib/types';
import { StatusLine, useStatus } from './SettingsApp';
import { BriefingSection } from './BriefingSection';
import { CareSection } from './CareSection';

interface Props {
  api: SettingsApi;
  view: SettingsView;
}

const FREQ: { id: Frequency; label: string }[] = [
  { id: 'rare', label: '很少' },
  { id: 'sometimes', label: '偶尔' },
  { id: 'often', label: '经常' },
];

export function CompanionPage({ api, view }: Props) {
  const [c, setC] = useState<CompanionSettings>(view.companion);
  const [d, setD] = useState<DailySettings>(view.daily);
  const { status, run } = useStatus();

  const save = (next: CompanionSettings) => {
    setC(next);
    run(() => api.saveCompanion(next));
  };
  const saveDaily = (next: DailySettings) => {
    setD(next);
    run(() => api.saveDaily(next));
  };
  const checked = (e: Event) => (e.currentTarget as HTMLInputElement).checked;
  const num = (e: Event, fallback: number) => {
    const v = Number((e.currentTarget as HTMLInputElement).value);
    return Number.isFinite(v) ? v : fallback;
  };

  return (
    <section>
      <h1>陪伴</h1>
      <p class="muted">
        角色会在合适的时候主动说话：提醒、关心你、每日播报、对你在做的事说上一句。这里统一控制「多久说一次、什么时候别说话」。
        勿扰模式、全屏（玩游戏、看视频）时她不会主动说话，你设的提醒除外。
      </p>

      <div class="card">
        <label class="row">
          <input type="checkbox" checked={c.enabled} onChange={(e) => save({ ...c, enabled: checked(e) })} />
          <span>
            允许她主动说话
            <small>关掉后只剩你自己设的提醒会出声</small>
          </span>
        </label>
        <div class="row">
          <span class="label">闲聊、评论的频率</span>
          <div class="seg" role="radiogroup" aria-label="主动说话频率">
            {FREQ.map((f) => (
              <button
                type="button"
                key={f.id}
                role="radio"
                aria-checked={c.frequency === f.id}
                class={c.frequency === f.id ? 'on' : ''}
                onClick={() => save({ ...c, frequency: f.id })}
              >
                {f.label}
              </button>
            ))}
          </div>
        </div>
        <div class="row">
          <label>
            两次主动说话至少间隔（分钟）
            <input
              type="number"
              min={5}
              max={240}
              value={c.cooldown_minutes}
              aria-label="主动说话间隔"
              onChange={(e) => save({ ...c, cooldown_minutes: num(e, 20) })}
            />
          </label>
        </div>
        <label class="row">
          <input type="checkbox" checked={c.quiet_enabled} onChange={(e) => save({ ...c, quiet_enabled: checked(e) })} />
          <span>
            安静时段
            <small>这段时间里除了你设的提醒，她不会主动说话</small>
          </span>
        </label>
        {c.quiet_enabled && (
          <div class="row">
            <label>
              从
              <input
                type="time"
                value={c.quiet_start}
                aria-label="安静时段开始"
                onChange={(e) => save({ ...c, quiet_start: (e.currentTarget as HTMLInputElement).value })}
              />
            </label>
            <label>
              到
              <input
                type="time"
                value={c.quiet_end}
                aria-label="安静时段结束"
                onChange={(e) => save({ ...c, quiet_end: (e.currentTarget as HTMLInputElement).value })}
              />
            </label>
          </div>
        )}
        <label class="row">
          <input type="checkbox" checked={c.system_notify} onChange={(e) => save({ ...c, system_notify: checked(e) })} />
          <span>
            全屏时到了的提醒改发系统通知
            <small>关掉的话会等你退出全屏再说</small>
          </span>
        </label>
      </div>

      <BriefingSection api={api} initial={view.briefing} />

      <CareSection api={api} initial={view.care} />

      <div class="card">
        <h2>很久没聊天时打个招呼</h2>
        <label class="row">
          <input type="checkbox" checked={d.proactive} onChange={(e) => saveDaily({ ...d, proactive: checked(e) })} />
          <span>
            打招呼
            <small>你在电脑前、但很久没和她说话时，她会打个招呼（默认关闭）</small>
          </span>
        </label>
        {d.proactive && (
          <div class="row">
            <label>
              多久没聊天才打招呼（分钟）
              <input
                type="number"
                min={30}
                value={d.proactive_minutes}
                onChange={(e) => saveDaily({ ...d, proactive_minutes: num(e, 90) || 90 })}
              />
            </label>
            <label>
              每天最多几次
              <input
                type="number"
                min={1}
                max={10}
                value={d.proactive_daily_max}
                onChange={(e) => saveDaily({ ...d, proactive_daily_max: num(e, 3) || 3 })}
              />
            </label>
          </div>
        )}
      </div>

      <div class="card">
        <h2>主动说话的花费</h2>
        <p class="muted small">
          大部分主动说的话是预先写好的台词，不花钱。少数需要结合具体情况的（比如每日播报）才调用模型，它们单独计数，用完当天就只用预写台词。
          聊天把每日 / 每月花费上限用到八成时，主动说话也会先停止调用模型，把额度留给聊天。
        </p>
        <div class="row">
          <label>
            每天最多调用几次模型
            <input
              type="number"
              min={0}
              max={100}
              value={c.daily_calls}
              aria-label="主动说话每日调用次数"
              onChange={(e) => save({ ...c, daily_calls: num(e, 10) })}
            />
          </label>
          <label>
            每天最多花（美元）
            <input
              type="number"
              min={0}
              step={0.01}
              value={c.daily_usd}
              aria-label="主动说话每日花费"
              onChange={(e) => save({ ...c, daily_usd: num(e, 0.05) })}
            />
          </label>
        </div>
      </div>
      <StatusLine status={status} />
    </section>
  );
}
