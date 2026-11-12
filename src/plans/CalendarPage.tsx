// 「日历」：提前知道的事（作业截止、某节课停课、考试……）。按月看，点某一天写下来；
// 她会在那天的 7 天前、3 天前、1 天前和当天各提醒一次（那天第一次打开桌宠时），搞定了可以勾掉、不再提醒。
// 和「提醒」分开：提醒管当天到点的急事，这里管提前知道、要提前几天开始惦记的事。
import { useEffect, useState } from 'preact/hooks';
import type { PlansApi } from '../lib/ipc';
import type { AgendaEntry } from '../lib/types';
import { addDays, dayLabel, dayStr, daysBetween, parseDay, relDay } from './util';

type Api = Pick<PlansApi, 'agendaRange' | 'agendaSave' | 'agendaDelete' | 'agendaDone'>;

/** 提前几天说（和后端 agenda.rs 的 AHEAD 一致） */
export const AHEAD = [7, 3, 1, 0];
const WEEK_HEAD = ['一', '二', '三', '四', '五', '六', '日'];

/** 某一天所在月的 1 号 */
export function monthOf(day: string): string {
  return `${day.slice(0, 8)}01`;
}

/** 往前 / 往后翻几个月 */
export function shiftMonth(month: string, n: number): string {
  const d = parseDay(month);
  if (!d) return month;
  d.setDate(1);
  d.setMonth(d.getMonth() + n);
  return dayStr(d);
}

/** 这个月的格子：从 1 号所在那周的周一开始，6 周 42 天 */
export function monthGrid(month: string): string[] {
  const d = parseDay(month);
  if (!d) return [];
  const back = (d.getDay() + 6) % 7;
  const start = addDays(month, -back);
  return Array.from({ length: 42 }, (_, i) => addDays(start, i));
}

/** 「今天」「明天」「还有 13 天」「3 天前」 */
export function leftText(day: string, today: string): string {
  const n = daysBetween(today, day);
  if (n === 0) return '今天';
  if (n === 1) return '明天';
  if (n === 2) return '后天';
  if (n > 0) return `还有 ${n} 天`;
  if (n === -1) return '昨天';
  return `${-n} 天前`;
}

/** 刚记下的这一条，她会在哪几天提醒（已经在 7 天内的，这一档不说，从下一档开始；和后端 fresh_reminded 一致） */
export function remindDays(day: string, today: string): string[] {
  const left = daysBetween(today, day);
  if (left < 0) return [];
  const stage = left <= AHEAD[0] ? Math.min(...AHEAD.filter((t) => t >= left)) : null;
  return AHEAD.filter((t) => stage === null || t < stage).map((t) => addDays(day, -t));
}

/** 「她会在 11月18日、11月22日、11月24日和当天提醒你」 */
export function remindHint(day: string, today: string, name: string): string {
  if (daysBetween(today, day) < 0) return '过去的日子只是记下来，不会提醒。';
  const days = remindDays(day, today);
  if (days.length === 0) return `今天的事：${name}今天就不再另外提醒了。`;
  const parts = days.map((d) => (d === day ? '当天' : dayLabel(d).split(' ')[0]));
  const last = parts.pop() ?? '';
  return `${name}会在${parts.length ? `${parts.join('、')}和` : ''}${last}各提醒你一次（那天第一次打开桌宠时）。`;
}

function emptyEntry(day: string): AgendaEntry {
  return { id: 0, day, title: '', note: '', done: false, done_at: null, created_at: 0, updated_at: 0 };
}

function EntryRow({ e, api, onChanged, onError }: { e: AgendaEntry; api: Api; onChanged: () => void; onError: (x: unknown) => void }) {
  const [edit, setEdit] = useState(false);
  const [title, setTitle] = useState(e.title);
  const [note, setNote] = useState(e.note);
  const run = (p: Promise<unknown>) =>
    p
      .then(() => {
        setEdit(false);
        onChanged();
      })
      .catch(onError);
  if (edit) {
    return (
      <li class="agenda-item editing">
        <form
          class="agenda-form"
          onSubmit={(ev) => {
            ev.preventDefault();
            if (title.trim()) run(api.agendaSave({ ...e, title: title.trim(), note }));
          }}
        >
          <input value={title} maxLength={100} onInput={(x) => setTitle(x.currentTarget.value)} aria-label="改成什么" />
          <textarea rows={2} maxLength={1000} value={note} onInput={(x) => setNote(x.currentTarget.value)} aria-label="备注" placeholder="备注（可以不写）" />
          <div class="actions">
            <button type="submit" class="btn btn-primary btn-small" disabled={!title.trim()}>
              保存
            </button>
            <button type="button" class="btn btn-small" onClick={() => setEdit(false)}>
              算了
            </button>
          </div>
        </form>
      </li>
    );
  }
  return (
    <li class={`agenda-item ${e.done ? 'done' : ''}`}>
      <button
        type="button"
        class={`check ${e.done ? 'on' : ''}`}
        role="checkbox"
        aria-checked={e.done}
        aria-label={e.done ? `改回没搞定：${e.title}` : `搞定了：${e.title}`}
        title={e.done ? '改回没搞定' : '搞定了（之后不再提醒）'}
        onClick={() => run(api.agendaDone(e.id, !e.done))}
      >
        {e.done ? '✓' : ''}
      </button>
      <div class="agenda-main">
        <div class="agenda-title">{e.title}</div>
        {e.note && <div class="agenda-note muted small">{e.note}</div>}
      </div>
      <div class="agenda-actions">
        <button
          type="button"
          class="link-btn"
          aria-label={`改一改：${e.title}`}
          onClick={() => {
            setTitle(e.title);
            setNote(e.note);
            setEdit(true);
          }}
        >
          改
        </button>
        <button type="button" class="link-btn danger" aria-label={`删掉：${e.title}`} onClick={() => run(api.agendaDelete(e.id))}>
          删掉
        </button>
      </div>
    </li>
  );
}

export function CalendarPage({
  api,
  today,
  name,
  tick,
  initialDay,
  onError,
}: {
  api: Api;
  /** 计划日（后端给的「今天」） */
  today: string;
  /** 她的名字（提示里用） */
  name: string;
  /** 计划有变化时加一，重新读 */
  tick: number;
  /** 打开时先选中哪一天（从提醒气泡里点「打开日历」带过来的） */
  initialDay?: string;
  onError: (e: unknown) => void;
}) {
  const start = initialDay && parseDay(initialDay) ? initialDay : today;
  const [month, setMonth] = useState(monthOf(start));
  const [sel, setSel] = useState(start);
  const [entries, setEntries] = useState<AgendaEntry[]>([]);
  const [reload, setReload] = useState(0);
  const [title, setTitle] = useState('');
  const [note, setNote] = useState('');

  // 别处要求跳到某一天（比如又点了一次提醒里的「打开日历」）
  useEffect(() => {
    if (initialDay && parseDay(initialDay)) {
      setMonth(monthOf(initialDay));
      setSel(initialDay);
    }
  }, [initialDay]);

  const grid = monthGrid(month);
  useEffect(() => {
    if (grid.length === 0) return;
    api
      .agendaRange(grid[0], grid[grid.length - 1])
      .then(setEntries)
      .catch(onError);
    // grid 由 month 决定
  }, [api, month, tick, reload]);

  const byDay = new Map<string, AgendaEntry[]>();
  for (const e of entries) {
    const v = byDay.get(e.day) ?? [];
    v.push(e);
    byDay.set(e.day, v);
  }
  const changed = () => setReload((r) => r + 1);
  const pick = (d: string) => {
    setSel(d);
    if (monthOf(d) !== month) setMonth(monthOf(d));
  };
  const selItems = byDay.get(sel) ?? [];
  const [y, m] = month.split('-').map(Number);

  const add = async (e: Event) => {
    e.preventDefault();
    if (!title.trim()) return;
    try {
      await api.agendaSave({ ...emptyEntry(sel), title: title.trim(), note: note.trim() });
      setTitle('');
      setNote('');
      changed();
    } catch (err) {
      onError(err);
    }
  };

  return (
    <section class="agenda">
      <header class="page-head">
        <div>
          <h1>日历</h1>
          <div class="muted small">提前知道的事记在这里：作业截止、哪节课停课、考试……点一下那一天就能写。</div>
        </div>
      </header>

      <div class="card agenda-month">
        <div class="agenda-head">
          <button type="button" class="icon-btn" aria-label="上个月" onClick={() => setMonth(shiftMonth(month, -1))}>
            ‹
          </button>
          <h2 aria-live="polite">
            {y}年{m}月
          </h2>
          <button type="button" class="icon-btn" aria-label="下个月" onClick={() => setMonth(shiftMonth(month, 1))}>
            ›
          </button>
          {(monthOf(today) !== month || sel !== today) && (
            <button type="button" class="btn btn-small" onClick={() => pick(today)}>
              回到今天
            </button>
          )}
        </div>
        <div class="agenda-grid">
          {WEEK_HEAD.map((w) => (
            <div key={w} class="agenda-wd" aria-hidden="true">
              {w}
            </div>
          ))}
          {grid.map((d) => {
            const items = byDay.get(d) ?? [];
            const live = items.filter((e) => !e.done);
            const other = monthOf(d) !== month;
            const past = daysBetween(today, d) < 0;
            const num = Number(d.slice(8));
            const cls = ['agenda-cell', other && 'other', past && 'past', d === today && 'today', d === sel && 'sel', items.length > 0 && 'has']
              .filter(Boolean)
              .join(' ');
            const label = `${dayLabel(d)}${d === today ? '（今天）' : ''}${items.length ? `，${items.length} 件：${items.map((e) => e.title + (e.done ? '（搞定了）' : '')).join('、')}` : ''}`;
            return (
              <button type="button" key={d} class={cls} aria-label={label} aria-pressed={d === sel} data-day={d} onClick={() => pick(d)}>
                <span class="agenda-num">{num}</span>
                {items.slice(0, 2).map((e) => (
                  <span key={e.id} class={`agenda-chip ${e.done ? 'done' : ''}`}>
                    {e.title}
                  </span>
                ))}
                {items.length > 2 && <span class="agenda-more">+{items.length - 2}</span>}
                {live.length > 0 && <span class="agenda-dot" aria-hidden="true" />}
              </button>
            );
          })}
        </div>
      </div>

      <div class="card agenda-day">
        <h2>
          {dayLabel(sel)} <span class="muted small">（{leftText(sel, today)}）</span>
        </h2>
        {selItems.length === 0 && <p class="muted small">这天还没记东西。</p>}
        <ul class="plain agenda-list">
          {selItems.map((e) => (
            <EntryRow key={`${e.id}-${e.updated_at}`} e={e} api={api} onChanged={changed} onError={onError} />
          ))}
        </ul>
        <form class="agenda-form agenda-add" onSubmit={add}>
          <input
            placeholder="这天有什么事？比如「数据结构作业截止」「线代课停一次」"
            maxLength={100}
            value={title}
            onInput={(e) => setTitle(e.currentTarget.value)}
            aria-label="这天有什么事"
          />
          <textarea rows={2} maxLength={1000} placeholder="备注（可以不写）" value={note} onInput={(e) => setNote(e.currentTarget.value)} aria-label="备注（可以不写）" />
          <div class="actions">
            <button type="submit" class="btn btn-primary btn-small" disabled={!title.trim()}>
              记下
            </button>
          </div>
        </form>
        <p class="muted small">{remindHint(sel, today, name)}</p>
      </div>
    </section>
  );
}

/** 「今天」页上的小卡片：接下来 7 天日历上的事（没有就不显示） */
export function AgendaSoon({
  api,
  today,
  tick,
  onOpen,
  onError,
}: {
  api: Pick<PlansApi, 'agendaRange'>;
  today: string;
  tick: number;
  onOpen: (day: string) => void;
  onError: (e: unknown) => void;
}) {
  const [list, setList] = useState<AgendaEntry[]>([]);
  useEffect(() => {
    api
      .agendaRange(today, addDays(today, AHEAD[0]))
      .then((v) => setList(v.filter((e) => !e.done)))
      .catch(onError);
  }, [api, today, tick]);
  if (list.length === 0) return null;
  return (
    <div class="card agenda-soon">
      <h2>日历上接下来的事</h2>
      <ul class="plain">
        {list.map((e) => (
          <li key={e.id}>
            <button type="button" class="link-btn" onClick={() => onOpen(e.day)}>
              <b>{relDay(e.day, today)}</b> {e.title}
            </button>
            <span class="muted small">{leftText(e.day, today) === relDay(e.day, today) ? '' : leftText(e.day, today)}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}
