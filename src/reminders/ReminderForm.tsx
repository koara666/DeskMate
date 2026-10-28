// 手动设置 / 修改提醒的小表单（聊天气泡和设置页共用）。保存时后端会再检查一遍。
import { useState } from 'preact/hooks';
import type { Reminder, Repeat, Trigger } from '../lib/types';

export const ACTIVITIES: { id: string; name: string }[] = [
  { id: 'game', name: '玩游戏' },
  { id: 'coding', name: '写代码' },
  { id: 'video', name: '看视频' },
  { id: 'writing', name: '写作业 / 写东西' },
  { id: 'music', name: '听音乐' },
  { id: 'meeting', name: '开会' },
];
const WD = ['一', '二', '三', '四', '五', '六', '日'];

const pad = (n: number) => String(n).padStart(2, '0');

/** Unix 秒 → `<input type="datetime-local">` 的值（本地时间） */
export function toLocalInput(ts: number): string {
  const d = new Date(ts * 1000);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** `datetime-local` / `date` 的值 → Unix 秒（本地时间）；无效时返回 null */
export function fromLocalInput(v: string): number | null {
  if (!v) return null;
  const s = v.length === 10 ? `${v}T00:00` : v;
  const t = new Date(s).getTime();
  return Number.isFinite(t) ? Math.floor(t / 1000) : null;
}

export function emptyReminder(now = Date.now()): Reminder {
  // 默认一小时后、整点
  const d = new Date(now + 3600_000);
  d.setMinutes(0, 0, 0);
  return {
    id: 0,
    content: '',
    trigger: 'at',
    at: Math.floor(d.getTime() / 1000),
    repeat: { type: 'none' },
    activity: 'game',
    line: '',
    important: false,
    status: 'active',
    created_at: 0,
    last_fired_at: null,
  };
}

interface Props {
  initial: Reminder;
  onSave: (r: Reminder) => Promise<unknown>;
  onCancel: () => void;
}

export function ReminderForm({ initial, onSave, onCancel }: Props) {
  const [r, setR] = useState<Reminder>(initial);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const set = (patch: Partial<Reminder>) => setR({ ...r, ...patch });
  const val = (e: Event) => (e.currentTarget as HTMLInputElement).value;

  const setRepeatType = (t: Repeat['type']) => {
    const day = r.at ? new Date(r.at * 1000) : new Date();
    const wd = ((day.getDay() + 6) % 7) + 1;
    const rep: Repeat =
      t === 'weekly'
        ? { type: 'weekly', days: [wd] }
        : t === 'monthly'
          ? { type: 'monthly', day: day.getDate() }
          : ({ type: t } as Repeat);
    set({ repeat: rep });
  };

  const save = async () => {
    setBusy(true);
    setError('');
    try {
      // 改过的提醒台词让后端重新生成（内容可能变了）
      await onSave({ ...r, line: r.content === initial.content ? r.line : '' });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class="reminder-form">
      <label class="rf-row">
        <span>提醒我</span>
        <input
          value={r.content}
          maxLength={200}
          placeholder="比如：收衣服"
          aria-label="要提醒的事"
          onInput={(e) => set({ content: val(e) })}
        />
      </label>
      <label class="rf-row">
        <span>什么时候</span>
        <select
          aria-label="提醒方式"
          value={r.trigger}
          onChange={(e) => {
            const t = val(e) as Trigger;
            set({
              trigger: t,
              at: t === 'at' ? (r.at ?? emptyReminder().at) : null,
              repeat: { type: 'none' },
            });
          }}
        >
          <option value="at">某个时间</option>
          <option value="startup">开机时</option>
          <option value="activity">开始做某件事时</option>
        </select>
      </label>
      {r.trigger === 'at' && (
        <>
          <label class="rf-row">
            <span>时间</span>
            <input
              type="datetime-local"
              aria-label="提醒时间"
              value={r.at ? toLocalInput(r.at) : ''}
              onInput={(e) => set({ at: fromLocalInput(val(e)) })}
            />
          </label>
          <label class="rf-row">
            <span>重复</span>
            <select aria-label="重复" value={r.repeat.type} onChange={(e) => setRepeatType(val(e) as Repeat['type'])}>
              <option value="none">不重复</option>
              <option value="daily">每天</option>
              <option value="weekdays">工作日</option>
              <option value="weekly">每周</option>
              <option value="monthly">每月</option>
            </select>
          </label>
          {r.repeat.type === 'weekly' && (
            <div class="rf-row rf-days" role="group" aria-label="每周哪几天">
              {WD.map((n, i) => {
                const d = i + 1;
                const days = r.repeat.type === 'weekly' ? r.repeat.days : [];
                const on = days.includes(d);
                return (
                  <button
                    type="button"
                    key={d}
                    class={on ? 'on' : ''}
                    aria-pressed={on}
                    onClick={() =>
                      set({ repeat: { type: 'weekly', days: on ? days.filter((x) => x !== d) : [...days, d].sort() } })
                    }
                  >
                    {n}
                  </button>
                );
              })}
            </div>
          )}
          {r.repeat.type === 'monthly' && (
            <label class="rf-row">
              <span>每月几号</span>
              <input
                type="number"
                min={1}
                max={31}
                value={r.repeat.day}
                onInput={(e) => set({ repeat: { type: 'monthly', day: Number(val(e)) || 1 } })}
              />
            </label>
          )}
        </>
      )}
      {r.trigger === 'startup' && (
        <>
          <label class="rf-row">
            <span>从哪天起</span>
            <input
              type="date"
              aria-label="从哪天起"
              value={r.at ? toLocalInput(r.at).slice(0, 10) : ''}
              onInput={(e) => set({ at: fromLocalInput(val(e)) })}
            />
          </label>
          <label class="rf-check">
            <input
              type="checkbox"
              checked={r.repeat.type === 'daily'}
              onChange={(e) =>
                set({ repeat: (e.currentTarget as HTMLInputElement).checked ? { type: 'daily' } : { type: 'none' } })
              }
            />
            每天开机都提醒
          </label>
        </>
      )}
      {r.trigger === 'activity' && (
        <label class="rf-row">
          <span>开始</span>
          <select aria-label="活动" value={r.activity} onChange={(e) => set({ activity: val(e) })}>
            {ACTIVITIES.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name}
              </option>
            ))}
          </select>
        </label>
      )}
      <label class="rf-check">
        <input
          type="checkbox"
          checked={r.important}
          onChange={(e) => set({ important: (e.currentTarget as HTMLInputElement).checked })}
        />
        重要：到时同时发系统通知
      </label>
      {error && <div class="rf-error">{error}</div>}
      <div class="rf-actions">
        <button type="button" class="btn btn-small" disabled={busy || !r.content.trim()} onClick={save}>
          保存
        </button>
        <button type="button" class="btn btn-small btn-plain" onClick={onCancel}>
          取消
        </button>
      </div>
    </div>
  );
}
