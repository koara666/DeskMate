// 「今天」：她的一句话、今天的事项（所有计划 + 单独的事情）、随手加一件事、今天的备注、接下来几天。
import { useEffect, useState } from 'preact/hooks';
import type { PlansApi } from '../lib/ipc';
import type { PlanBoot, PlanTaskView, PlanToday } from '../lib/types';
import { AgendaSoon } from './CalendarPage';
import { TaskRow } from './TaskRow';
import { clampInt, dayLabel, emptyTask, mins, minsShort, relDay } from './util';

type Api = Pick<
  PlansApi,
  'today' | 'doneTask' | 'taskStatus' | 'moveTask' | 'deleteTask' | 'saveTask' | 'addNote' | 'deleteNote'
> &
  Partial<Pick<PlansApi, 'agendaRange'>>;

export function TodayPage({
  api,
  boot,
  tick,
  onOpenPlan,
  onNew,
  onOpenCalendar,
  onError,
  onFocus,
}: {
  api: Api;
  boot: PlanBoot;
  /** 计划有变化时加一，重新读 */
  tick: number;
  onOpenPlan: (id: number) => void;
  onNew: () => void;
  /** 点了日历上的事：打开日历的那一天 */
  onOpenCalendar?: (day: string) => void;
  onError: (e: unknown) => void;
  onFocus?: (t: PlanTaskView) => void;
}) {
  const [data, setData] = useState<PlanToday | null>(null);
  const [text, setText] = useState('');
  const [minutes, setMinutes] = useState('');
  const [start, setStart] = useState('');
  const [planId, setPlanId] = useState('');
  const [note, setNote] = useState('');

  const load = () => {
    api
      .today()
      .then(setData)
      .catch(onError);
  };
  useEffect(load, [api, tick]);

  if (!data) return <div class="muted">加载中…</div>;

  const live = data.tasks.filter((t) => t.status !== 'skipped');
  const doneCount = live.filter((t) => t.status === 'done').length;
  const allDone = live.length > 0 && doneCount === live.length;
  const pct = data.planned > 0 ? Math.min(100, Math.round((data.done / data.planned) * 100)) : 0;
  const active = data.plans.filter((p) => p.plan.status === 'active');
  const avatar = (allDone ? boot.images.happy : null) ?? boot.images.idle ?? Object.values(boot.images)[0];

  const add = async (e: Event) => {
    e.preventDefault();
    if (!text.trim()) return;
    try {
      await api.saveTask({
        ...emptyTask(data.day),
        title: text.trim(),
        minutes: clampInt(minutes || 0, 0, 720),
        start,
        plan_id: planId ? Number(planId) : null,
      });
      setText('');
      setMinutes('');
      setStart('');
      load();
    } catch (err) {
      onError(err);
    }
  };

  const addNote = async (e: Event) => {
    e.preventDefault();
    if (!note.trim()) return;
    try {
      await api.addNote({ plan_id: null, day: data.day, content: note.trim() });
      setNote('');
      load();
    } catch (err) {
      onError(err);
    }
  };

  return (
    <section class="today">
      <header class="page-head">
        <div>
          <h1>今天</h1>
          <div class="muted">{dayLabel(data.day)}</div>
        </div>
        {data.streak > 0 && (
          <div class="streak" title="连续把每天的安排都做完的天数">
            🔥 连续 {data.streak} 天
          </div>
        )}
      </header>

      <div class="her-say">
        {avatar && <img class="her-avatar" src={avatar} alt="" />}
        <div class="bubble" aria-live="polite">
          {data.say}
        </div>
      </div>

      {data.planned > 0 && (
        <div class="day-progress" aria-label="今天的进度">
          <div class="row">
            <span>
              做完 <b>{doneCount}</b> / {live.length} 项
            </span>
            <span class="muted small">
              {mins(data.done)} / {mins(data.planned)}
            </span>
          </div>
          <div class="bar her">
            <div class="fill" style={{ width: `${pct}%` }} />
          </div>
        </div>
      )}

      <div class="card">
        <h2>今天的事项</h2>
        {data.tasks.length === 0 && (
          <div class="empty">
            <p class="muted">今天还没有安排。</p>
            {active.length === 0 && (
              <button type="button" class="btn btn-primary" onClick={onNew}>
                做一个计划
              </button>
            )}
          </div>
        )}
        <ul class="plain task-list">
          {data.tasks.map((t) => (
            <TaskRow key={`${t.id}-${t.status}-${t.title}`} task={t} today={data.day} api={api} onChanged={load} onError={onError} onFocus={onFocus} />
          ))}
        </ul>
        <form class="quick-add" onSubmit={add}>
          <input
            class="grow"
            placeholder="随手加一件事，比如「交实验报告」"
            maxLength={100}
            value={text}
            onInput={(e) => setText(e.currentTarget.value)}
            aria-label="加一件事"
          />
          <input
            type="number"
            min={0}
            max={720}
            step={5}
            placeholder="分钟"
            class="num"
            value={minutes}
            onInput={(e) => setMinutes(e.currentTarget.value)}
            aria-label="大概多少分钟"
          />
          <input type="time" value={start} onInput={(e) => setStart(e.currentTarget.value)} aria-label="几点开始" />
          {active.length > 0 && (
            <select value={planId} onChange={(e) => setPlanId(e.currentTarget.value)} aria-label="属于哪个计划">
              <option value="">单独一件事</option>
              {active.map((p) => (
                <option key={p.plan.id} value={String(p.plan.id)}>
                  {p.plan.emoji} {p.plan.title}
                </option>
              ))}
            </select>
          )}
          <button type="submit" class="btn" disabled={!text.trim()}>
            加上
          </button>
        </form>
        <p class="muted small">单独的事情今天没做完，明天会自动挪过去；计划里没做完的内容会排到后面的日子。</p>
      </div>

      {api.agendaRange && onOpenCalendar && (
        <AgendaSoon api={{ agendaRange: api.agendaRange }} today={data.day} tick={tick} onOpen={onOpenCalendar} onError={onError} />
      )}

      <div class="two-col">
        <div class="card">
          <h2>今天的备注</h2>
          {data.notes.length === 0 && <p class="muted small">今天学了什么、状态怎么样、明天想注意什么，都可以记在这里。</p>}
          <ul class="plain note-list">
            {data.notes.map((n) => (
              <li key={n.id}>
                <span class="grow">{n.content}</span>
                <button
                  type="button"
                  class="link-btn danger"
                  aria-label="删掉这条备注"
                  onClick={() =>
                    api
                      .deleteNote(n.id)
                      .then(load)
                      .catch(onError)
                  }
                >
                  删掉
                </button>
              </li>
            ))}
          </ul>
          <form class="note-add" onSubmit={addNote}>
            <textarea rows={2} maxLength={2000} placeholder="写点什么…" value={note} onInput={(e) => setNote(e.currentTarget.value)} aria-label="今天的备注" />
            <button type="submit" class="btn btn-small" disabled={!note.trim()}>
              记下
            </button>
          </form>
        </div>

        <div class="card">
          <h2>接下来几天</h2>
          {data.upcoming.length === 0 && <p class="muted small">后面几天还没有安排。</p>}
          <ul class="plain upcoming">
            {data.upcoming.map((u) => {
              const total = u.entries.reduce((s, e) => s + e.minutes, 0);
              return (
                <li key={u.day}>
                  <div class="up-head">
                    <b>{relDay(u.day, data.day)}</b>
                    <span class="muted small">
                      {u.entries.length} 项{total > 0 ? ` · ${minsShort(total)}` : ''}
                    </span>
                  </div>
                  <div class="up-items">
                    {u.entries.map((e, i) => (
                      <button
                        type="button"
                        key={i}
                        class={`up-item ${e.stored ? 'stored' : ''}`}
                        title={e.stored ? '已经定下的' : '按现在的进度推算的'}
                        onClick={() => e.plan_id && onOpenPlan(e.plan_id)}
                        disabled={!e.plan_id}
                      >
                        {e.label ? `${e.label.split(' ')[0]} ` : ''}
                        {e.title}
                        {e.minutes > 0 ? ` · ${minsShort(e.minutes)}` : ''}
                      </button>
                    ))}
                  </div>
                </li>
              );
            })}
          </ul>
        </div>
      </div>
    </section>
  );
}
