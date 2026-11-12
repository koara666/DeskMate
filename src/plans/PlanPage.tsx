// 一个计划的详情：进度、她的安排思路、内容、日历（过去做了多少 / 以后排了什么）、备注、设置、重新安排。
import { useEffect, useState } from 'preact/hooks';
import type { PlansApi } from '../lib/ipc';
import type { Goal, Plan, PlanBoot, PlanDayStat, PlanDraft, PlanItem, PlanUpcoming, PlanView } from '../lib/types';
import { ItemsEditor, PlanForm } from './editors';
import { DraftEditor } from './NewPlanPage';
import { addDays, daysBetween, dayLabel, mins, minsShort, parseDay, relDay } from './util';

type Api = Pick<PlansApi, 'get' | 'apply' | 'setStatus' | 'remove' | 'replan' | 'preview' | 'addNote' | 'deleteNote'>;
type Tab = 'items' | 'calendar' | 'notes' | 'settings';

const STATUS: Record<string, string> = { active: '进行中', paused: '暂停中', done: '已完成', archived: '已归档' };

/** 过去五周 + 以后两周的日历：颜色深浅 = 那天做完了多少 */
export function Calendar({ history, upcoming, today }: { history: PlanDayStat[]; upcoming: PlanUpcoming[]; today: string }) {
  const [pick, setPick] = useState<string | null>(null);
  const past = new Map(history.map((h) => [h.day, h]));
  const next = new Map(upcoming.map((u) => [u.day, u]));
  const t = parseDay(today);
  if (!t) return null;
  const monday = addDays(today, -((t.getDay() + 6) % 7));
  const start = addDays(monday, -28);
  const days = Array.from({ length: 49 }, (_, i) => addDays(start, i));
  const level = (s: PlanDayStat) => {
    // 今天还没过完：没做完的不算「没做」
    if (s.day === today && s.tasks_todo > 0) return s.tasks_done > 0 ? 'some' : 'open';
    if (s.tasks_done === 0) return s.tasks_todo > 0 ? 'miss' : 'skip';
    if (s.tasks_todo === 0) return 'full';
    return s.done >= s.planned / 2 ? 'most' : 'some';
  };
  const sel = pick ?? today;
  const ps = past.get(sel);
  const pu = next.get(sel);
  return (
    <div class="calendar">
      <div class="cal-grid" role="grid" aria-label="日历">
        {['一', '二', '三', '四', '五', '六', '日'].map((w) => (
          <div key={w} class="cal-head">
            {w}
          </div>
        ))}
        {days.map((d) => {
          const s = past.get(d);
          const u = next.get(d);
          const n = daysBetween(today, d);
          const cls = ['cal-cell', n === 0 ? 'today' : '', n > 0 ? 'future' : '', s ? `lv-${level(s)}` : '', u ? 'planned' : '', sel === d ? 'sel' : '']
            .filter(Boolean)
            .join(' ');
          const dd = parseDay(d);
          return (
            <button
              type="button"
              key={d}
              class={cls}
              role="gridcell"
              aria-label={`${dayLabel(d)}${s ? `，做完 ${s.tasks_done}/${s.tasks} 项` : u ? `，排了 ${u.entries.length} 项` : ''}`}
              onClick={() => setPick(d)}
            >
              {dd?.getDate()}
            </button>
          );
        })}
      </div>
      <div class="cal-side">
        <div class="cal-legend muted small">
          <span class="cal-dot lv-full" /> 全做完 <span class="cal-dot lv-most" /> 做了大半 <span class="cal-dot lv-some" /> 做了一点 <span class="cal-dot lv-miss" /> 没做
          <span class="cal-dot planned" /> 以后排了
        </div>
        <div class="cal-detail">
          <b>{relDay(sel, today)}</b>
          {ps && (
            <span>
              ：做完 {ps.tasks_done}/{ps.tasks} 项，{mins(ps.done)} / 计划 {mins(ps.planned)}
            </span>
          )}
          {pu && (
            <ul class="plain">
              {pu.entries.map((e, i) => (
                <li key={i}>
                  · {e.title} <span class="muted small">{minsShort(e.minutes)}</span>
                  {e.stored ? <span class="tag">已定</span> : null}
                </li>
              ))}
            </ul>
          )}
          {!ps && !pu && <span class="muted">：这天没有安排</span>}
        </div>
      </div>
    </div>
  );
}

export function PlanPage({
  api,
  boot,
  id,
  tick,
  goal = null,
  onOpenGoal,
  onDeleted,
  onError,
}: {
  api: Api;
  boot: PlanBoot;
  id: number;
  tick: number;
  /** 这是哪个计划书里的一条线（null = 单独的计划） */
  goal?: Goal | null;
  onOpenGoal?: (id: number) => void;
  onDeleted: () => void;
  onError: (e: unknown) => void;
}) {
  const [v, setV] = useState<PlanView | null>(null);
  const [tab, setTab] = useState<Tab>('items');
  const [items, setItems] = useState<PlanItem[] | null>(null);
  const [plan, setPlan] = useState<Plan | null>(null);
  const [note, setNote] = useState('');
  const [replan, setReplan] = useState<string | null>(null);
  const [draft, setDraft] = useState<PlanDraft | null>(null);
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState('');
  const [confirmDelete, setConfirmDelete] = useState(false);

  const load = () => {
    api
      .get(id)
      .then((x) => {
        setV(x);
        setItems(null);
        setPlan(null);
      })
      .catch(onError);
  };
  useEffect(() => {
    setDraft(null);
    setReplan(null);
    setConfirmDelete(false);
    setTab('items');
  }, [id]);
  useEffect(load, [api, id, tick]);

  if (!v) return <div class="muted">加载中…</div>;
  const p = v.plan;
  const g = v.progress;

  const save = async (nextPlan: Plan, nextItems: PlanItem[] | null, msg = '已保存') => {
    setBusy(true);
    try {
      await api.apply(nextPlan, nextItems);
      setSaved(msg);
      setTimeout(() => setSaved(''), 2500);
      load();
    } catch (e) {
      onError(e);
    } finally {
      setBusy(false);
    }
  };

  const doReplan = async () => {
    setBusy(true);
    try {
      setDraft(await api.replan(id, replan ?? ''));
    } catch (e) {
      onError(e);
    } finally {
      setBusy(false);
    }
  };

  if (draft) {
    return (
      <section>
        <header class="page-head">
          <h1>重新安排：{p.title}</h1>
        </header>
        <DraftEditor
          draft={draft}
          api={api}
          boot={boot}
          confirmLabel="按这个改"
          busy={busy}
          backLabel="不改了"
          onConfirm={async (np, ni) => {
            await save(np, ni, '已重新安排');
            setDraft(null);
            setReplan(null);
          }}
          onBack={() => setDraft(null)}
        />
      </section>
    );
  }

  const daysLeft = p.end_day ? daysBetween(boot.today, p.end_day) : null;
  const inGoal = !!p.goal_id;
  return (
    <section class="plan-page">
      {inGoal && goal && onOpenGoal && (
        <button type="button" class="link-btn crumb" onClick={() => onOpenGoal(goal.id)}>
          ← {goal.emoji} {goal.title}
        </button>
      )}
      <header class="page-head">
        <div>
          <h1>
            {p.emoji} {p.title}
          </h1>
          <div class="row small">
            <span class={`tag ${p.status === 'active' ? 'tag-accent' : ''}`}>{STATUS[p.status]}</span>
            <span class="muted">
              {dayLabel(p.start_day)} 开始
              {p.end_day ? ` · 截止 ${dayLabel(p.end_day)}${daysLeft !== null && daysLeft >= 0 ? `（还有 ${daysLeft} 天）` : ''}` : ''}
              {p.remind_at ? ` · 每天 ${p.remind_at}` : ''}
            </span>
          </div>
        </div>
        <div class="head-actions">
          {p.status === 'active' && !inGoal && (
            <button type="button" class="btn btn-small" onClick={() => setReplan(replan === null ? '' : null)}>
              重新安排
            </button>
          )}
          {inGoal && goal && onOpenGoal && (
            <button type="button" class="btn btn-small" onClick={() => onOpenGoal(goal.id)}>
              在计划书里看
            </button>
          )}
          {p.status === 'active' && (
            <button type="button" class="btn btn-small" onClick={() => api.setStatus(id, 'paused').then(load).catch(onError)}>
              暂停
            </button>
          )}
          {(p.status === 'paused' || p.status === 'archived' || p.status === 'done') && (
            <button type="button" class="btn btn-small" onClick={() => api.setStatus(id, 'active').then(load).catch(onError)}>
              {p.status === 'done' ? '再开始' : '恢复'}
            </button>
          )}
          {p.status !== 'archived' && (
            <button type="button" class="btn btn-small" onClick={() => api.setStatus(id, 'archived').then(load).catch(onError)}>
              归档
            </button>
          )}
        </div>
      </header>

      {replan !== null && (
        <div class="card replan">
          <h2>想怎么调整？</h2>
          <textarea
            rows={3}
            maxLength={1000}
            placeholder="可以不写，她会按现在的进度调整。也可以说：后面几章比较难，多给点时间 / 周末不想学了 / 截止提前到12月15日"
            value={replan}
            onInput={(e) => setReplan(e.currentTarget.value)}
            aria-label="想怎么调整"
            disabled={busy}
          />
          <div class="actions">
            <button type="button" class="btn btn-primary" disabled={busy} onClick={doReplan}>
              {busy ? '她在想…' : '让她重新排'}
            </button>
            <button type="button" class="btn" onClick={() => setReplan(null)}>
              算了
            </button>
          </div>
        </div>
      )}

      <div class="card progress-card">
        {p.mode === 'content' ? (
          <>
            <div class="row">
              <span class="big">{g.percent}%</span>
              <span class="muted">
                已做 {mins(g.done_minutes)} / 共约 {mins(g.total_minutes)} · 内容 {g.items_done}/{g.items_total}
              </span>
              {g.streak > 0 && <span class="streak small">🔥 连续 {g.streak} 天</span>}
            </div>
            <div class="bar her">
              <div class="fill" style={{ width: `${g.percent}%` }} />
            </div>
            <div class="row small">
              {p.status === 'done' && <span class="tag tag-ok">全部做完了 🎉</span>}
              {p.status === 'active' && g.finish && (
                <span>
                  按现在的进度，预计 <b>{dayLabel(g.finish)}</b> 做完
                </span>
              )}
              {g.overdue && <span class="warn-text">已经过了截止日期，还剩一些没做完。</span>}
            </div>
            {g.short_minutes > 0 && p.status === 'active' && (
              <div class="alert alert-error">
                按每天的时间，到截止还差大约 {mins(g.short_minutes)}。可以在「设置」里多给点时间、把截止往后挪，或者点「重新安排」让她想办法。
              </div>
            )}
          </>
        ) : (
          <div class="row">
            <span class="big">
              {g.week_done}/{g.week_days}
            </span>
            <span class="muted">最近 7 天有安排的日子里做完的天数</span>
            {g.streak > 0 && <span class="streak small">🔥 连续 {g.streak} 天</span>}
          </div>
        )}
      </div>

      {p.strategy && (
        <details class="card strategy">
          <summary>她的安排思路</summary>
          <p>{p.strategy}</p>
          {p.goal && (
            <p class="muted small">
              你当初写的：<span class="selectable">{p.goal}</span>
            </p>
          )}
        </details>
      )}

      <nav class="tabs" role="tablist">
        {(
          [
            ['items', p.mode === 'habit' ? '每天做什么' : '内容'],
            ['calendar', '日历'],
            ['notes', `备注${v.notes.length ? `（${v.notes.length}）` : ''}`],
            ['settings', '设置'],
          ] as [Tab, string][]
        ).map(([t, label]) => (
          <button type="button" key={t} role="tab" aria-selected={tab === t} class={tab === t ? 'on' : ''} onClick={() => setTab(t)}>
            {label}
          </button>
        ))}
        {saved && <span class="status status-ok">{saved}</span>}
      </nav>

      {tab === 'items' && (
        <div class="card">
          <ItemsEditor items={items ?? v.items} mode={p.mode} onChange={setItems} />
          {items && (
            <div class="actions">
              <button type="button" class="btn btn-primary" disabled={busy} onClick={() => save(p, items)}>
                保存内容
              </button>
              <button type="button" class="btn" onClick={() => setItems(null)}>
                撤销修改
              </button>
              <span class="muted small">保存后今天还没动的安排会按新内容重排。</span>
            </div>
          )}
        </div>
      )}

      {tab === 'calendar' && (
        <div class="card">
          <Calendar history={v.history} upcoming={v.upcoming} today={boot.today} />
        </div>
      )}

      {tab === 'notes' && (
        <div class="card">
          <form
            class="note-add"
            onSubmit={(e) => {
              e.preventDefault();
              if (!note.trim()) return;
              api
                .addNote({ plan_id: id, day: boot.today, content: note.trim() })
                .then(() => {
                  setNote('');
                  load();
                })
                .catch(onError);
            }}
          >
            <textarea rows={2} maxLength={2000} placeholder="进度、心得、卡住的地方、想调整的…" value={note} onInput={(e) => setNote(e.currentTarget.value)} aria-label="写一条备注" />
            <button type="submit" class="btn btn-small" disabled={!note.trim()}>
              记下
            </button>
          </form>
          {v.notes.length === 0 && <p class="muted small">还没有备注。</p>}
          <ul class="plain note-timeline">
            {v.notes.map((n) => (
              <li key={n.id}>
                <span class="note-day muted small">{relDay(n.day, boot.today)}</span>
                <span class="grow selectable">{n.content}</span>
                <button type="button" class="link-btn danger" aria-label="删掉这条备注" onClick={() => api.deleteNote(n.id).then(load).catch(onError)}>
                  删掉
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}

      {tab === 'settings' && (
        <div class="card">
          <PlanForm plan={plan ?? p} onChange={setPlan} today={boot.today} inGoal={inGoal} />
          <div class="actions">
            <button type="button" class="btn btn-primary" disabled={busy || !plan || !plan.title.trim()} onClick={() => plan && save(plan, null)}>
              保存设置
            </button>
            {plan && (
              <button type="button" class="btn" onClick={() => setPlan(null)}>
                撤销修改
              </button>
            )}
          </div>
          <div class="danger-zone">
            {!confirmDelete ? (
              <button type="button" class="btn btn-small btn-danger" onClick={() => setConfirmDelete(true)}>
                删掉这个计划
              </button>
            ) : (
              <div class="row">
                <span>{inGoal ? '这条线会从计划书里删掉，' : ''}删掉后内容、记录和备注都没了（不想继续可以「归档」）。确定吗？</span>
                <button type="button" class="btn btn-small btn-danger" onClick={() => api.remove(id).then(onDeleted).catch(onError)}>
                  确定删掉
                </button>
                <button type="button" class="btn btn-small" onClick={() => setConfirmDelete(false)}>
                  不删了
                </button>
              </div>
            )}
          </div>
        </div>
      )}
    </section>
  );
}
