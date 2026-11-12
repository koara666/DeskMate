// 计划窗口：左边是她、今天、各个计划；右边是内容。计划在别处变了（她那边记了完成、换了一天）会自动刷新。
import { useCallback, useEffect, useRef, useState } from 'preact/hooks';
import type { PlansApi } from '../lib/ipc';
import type { GoalSummary, PlanBoot, PlanFocus, PlanSummary, PlanTaskView } from '../lib/types';
import { CalendarPage } from './CalendarPage';
import { GoalPage } from './GoalPage';
import { NewPlanPage } from './NewPlanPage';
import { PlanPage } from './PlanPage';
import { StatsPage } from './StatsPage';
import { SupervisePage } from './SupervisePage';
import { TodayPage } from './TodayPage';

export type View =
  | { kind: 'today' }
  | { kind: 'stats' }
  | { kind: 'calendar'; day?: string }
  | { kind: 'new'; text: string }
  | { kind: 'plan'; id: number }
  | { kind: 'goal'; id: number; focus?: string }
  | { kind: 'settings' };

/** 后端 / 别的窗口传来的页面名 → 页面 */
export function toView(v: string | null | undefined): View | null {
  if (!v) return null;
  if (v === 'today') return { kind: 'today' };
  if (v === 'stats') return { kind: 'stats' };
  if (v === 'calendar') return { kind: 'calendar' };
  const c = /^calendar:(\d{4}-\d{2}-\d{2})$/.exec(v);
  if (c) return { kind: 'calendar', day: c[1] };
  if (v === 'settings') return { kind: 'settings' };
  if (v === 'new') return { kind: 'new', text: '' };
  if (v.startsWith('new:')) return { kind: 'new', text: v.slice(4) };
  const m = /^plan:(\d+)$/.exec(v);
  if (m) return { kind: 'plan', id: Number(m[1]) };
  const g = /^goal:(\d+)(?::(week|adjust|m\d+))?$/.exec(v);
  if (g) return { kind: 'goal', id: Number(g[1]), focus: g[2] };
  return null;
}

/** 专注计时条：还剩多久、做完了、停下 */
function FocusBar({ focus, api, onError }: { focus: PlanFocus; api: Pick<PlansApi, 'focusStop'>; onError: (e: unknown) => void }) {
  const skew = useRef(focus.now - Date.now() / 1000);
  const [, tick] = useState(0);
  useEffect(() => {
    skew.current = focus.now - Date.now() / 1000;
    const t = setInterval(() => tick((x) => x + 1), 1000);
    return () => clearInterval(t);
  }, [focus]);
  const left = Math.max(0, Math.round(focus.ends_at - (Date.now() / 1000 + skew.current)));
  const mm = String(Math.floor(left / 60)).padStart(2, '0');
  const ss = String(left % 60).padStart(2, '0');
  return (
    <div class="focus-bar" role="timer" aria-label={`专注中：${focus.title}，还剩 ${mm} 分 ${ss} 秒`}>
      <span class="time">
        🍅 {mm}:{ss}
      </span>
      <span class="grow">
        专注中：<b>{focus.title}</b>
        <div class="muted small">这段时间她不闲聊；跑去玩游戏会被叫回来。</div>
      </span>
      <button type="button" class="btn btn-small btn-primary" onClick={() => api.focusStop(true).catch(onError)}>
        做完了
      </button>
      <button type="button" class="btn btn-small" onClick={() => api.focusStop(false).catch(onError)}>
        停下
      </button>
    </div>
  );
}

export function PlansApp({ api }: { api: PlansApi }) {
  const [boot, setBoot] = useState<PlanBoot | null>(null);
  const [view, setView] = useState<View>({ kind: 'today' });
  const [plans, setPlans] = useState<PlanSummary[]>([]);
  const [goals, setGoals] = useState<GoalSummary[]>([]);
  const [openGoals, setOpenGoals] = useState<Set<number>>(new Set());
  const [tick, setTick] = useState(0);
  const [error, setError] = useState('');
  const [showOld, setShowOld] = useState(false);
  const [focus, setFocus] = useState<PlanFocus | null>(null);
  const mainRef = useRef<HTMLElement>(null);
  // 换了一页：回到顶上（不然停在上一页滚到的位置）
  useEffect(() => {
    if (mainRef.current) mainRef.current.scrollTop = 0;
  }, [view]);

  const reboot = useCallback(() => {
    api
      .boot()
      .then((b) => {
        setBoot(b);
        document.documentElement.style.setProperty('--her', /^#[0-9a-f]{3,8}$/i.test(b.character.color) ? b.character.color : '#e88fb0');
        const v = toView(b.view);
        if (v) setView(v);
      })
      .catch((e) => setError(String(e)));
  }, [api]);

  useEffect(() => {
    reboot();
    const offs = [
      api.onChanged(() => setTick((t) => t + 1)),
      api.onView((v) => {
        const next = toView(v);
        if (next) setView(next);
      }),
      api.onCharacterChanged(reboot),
      api.onFocus((f) => {
        setFocus(f);
        setTick((t) => t + 1);
      }),
    ];
    api
      .focusState()
      .then(setFocus)
      .catch(() => {});
    // 跨过午夜（凌晨 5 点换天）或者从睡眠里醒来：回到窗口时刷新一次
    const onVis = () => {
      if (document.visibilityState === 'visible') setTick((t) => t + 1);
    };
    document.addEventListener('visibilitychange', onVis);
    return () => {
      document.removeEventListener('visibilitychange', onVis);
      for (const o of offs) o.then((f) => f()).catch(() => {});
    };
  }, [api, reboot]);

  useEffect(() => {
    api
      .list()
      .then(setPlans)
      .catch(() => {});
    api
      .goalList()
      .then(setGoals)
      .catch(() => {});
  }, [api, tick, view]);

  const onError = (e: unknown) => {
    setError(String(e));
    setTimeout(() => setError(''), 6000);
  };
  const changed = () => setTick((t) => t + 1);
  const startFocus = (t: PlanTaskView) => {
    api
      .focusStart(t.id, null)
      .then(setFocus)
      .catch(onError);
  };

  if (!boot) {
    return <div class="plans-loading muted">{error || '加载中…'}</div>;
  }

  // 计划书里的线放在计划书下面，不单独列
  const single = plans.filter((p) => !p.plan.goal_id);
  const current = single.filter((p) => p.plan.status === 'active' || p.plan.status === 'paused');
  const old = single.filter((p) => p.plan.status === 'done' || p.plan.status === 'archived');
  const liveGoals = goals.filter((g) => g.goal.status === 'active' || g.goal.status === 'paused');
  const oldGoals = goals.filter((g) => g.goal.status === 'done' || g.goal.status === 'archived');
  const avatar = boot.images.idle ?? Object.values(boot.images)[0];
  const isPlan = (id: number) => view.kind === 'plan' && view.id === id;
  const isGoal = (id: number) => view.kind === 'goal' && view.id === id;
  const goalOf = (planId: number) => goals.find((g) => g.tracks.includes(planId));
  const trackOpen = (g: GoalSummary) => openGoals.has(g.goal.id) || isGoal(g.goal.id) || g.tracks.some((t) => isPlan(t));
  const toggleGoal = (id: number) => {
    const s = new Set(openGoals);
    if (s.has(id)) s.delete(id);
    else s.add(id);
    setOpenGoals(s);
  };

  return (
    <div class="plans">
      <nav class="plans-nav" aria-label="计划">
        <div class="her-card">
          {avatar ? <img src={avatar} alt="" /> : <div class="her-dot" />}
          <div>
            <div class="her-name">{boot.character.name || 'DeskMate'}</div>
            <div class="muted small">陪你做计划</div>
          </div>
        </div>
        <button type="button" class={`nav-item ${view.kind === 'today' ? 'active' : ''}`} aria-current={view.kind === 'today' ? 'page' : undefined} onClick={() => setView({ kind: 'today' })}>
          ☀ 今天
        </button>
        <button type="button" class={`nav-item ${view.kind === 'calendar' ? 'active' : ''}`} aria-current={view.kind === 'calendar' ? 'page' : undefined} onClick={() => setView({ kind: 'calendar' })}>
          📅 日历
        </button>
        <button type="button" class={`nav-item ${view.kind === 'stats' ? 'active' : ''}`} aria-current={view.kind === 'stats' ? 'page' : undefined} onClick={() => setView({ kind: 'stats' })}>
          📊 统计
        </button>
        {liveGoals.length > 0 && <div class="nav-group">计划书</div>}
        {liveGoals.map((g) => (
          <div key={g.goal.id} class="nav-goal">
            <button
              type="button"
              class={`nav-item plan-item goal-item ${isGoal(g.goal.id) ? 'active' : ''} ${g.goal.status === 'paused' ? 'paused' : ''}`}
              aria-current={isGoal(g.goal.id) ? 'page' : undefined}
              onClick={() => setView({ kind: 'goal', id: g.goal.id })}
            >
              <span class="pi-title">
                {g.goal.emoji} {g.goal.title}
              </span>
              <span class="pi-meta">
                {g.goal.status === 'paused' ? '暂停中' : `${g.percent}%`}
                {g.short && g.goal.status === 'active' ? ' ⚠' : ''}
              </span>
              <span class="pi-bar">
                <span style={{ width: `${g.percent}%` }} />
              </span>
              {g.phase && <span class="pi-sub muted">{g.phase}</span>}
            </button>
            {g.tracks.length > 0 && (
              <button type="button" class="nav-expand" aria-expanded={trackOpen(g)} aria-label={`${trackOpen(g) ? '收起' : '展开'}「${g.goal.title}」的线`} onClick={() => toggleGoal(g.goal.id)}>
                {trackOpen(g) ? '▾' : '▸'}
              </button>
            )}
            {trackOpen(g) &&
              g.tracks.map((tid) => {
                const s = plans.find((p) => p.plan.id === tid);
                if (!s) return null;
                return (
                  <button
                    type="button"
                    key={tid}
                    class={`nav-item plan-item track-item ${isPlan(tid) ? 'active' : ''}`}
                    aria-current={isPlan(tid) ? 'page' : undefined}
                    onClick={() => setView({ kind: 'plan', id: tid })}
                  >
                    <span class="pi-title">
                      {s.plan.mode === 'habit' ? '🔁' : s.plan.emoji} {s.plan.title}
                    </span>
                    <span class="pi-meta">{s.plan.mode === 'content' ? `${s.progress.percent}%` : ''}</span>
                  </button>
                );
              })}
          </div>
        ))}
        <div class="nav-group">我的计划</div>
        {current.length === 0 && <div class="muted small nav-empty">还没有计划</div>}
        {current.map((s) => (
          <button
            type="button"
            key={s.plan.id}
            class={`nav-item plan-item ${isPlan(s.plan.id) ? 'active' : ''} ${s.plan.status === 'paused' ? 'paused' : ''}`}
            aria-current={isPlan(s.plan.id) ? 'page' : undefined}
            onClick={() => setView({ kind: 'plan', id: s.plan.id })}
          >
            <span class="pi-title">
              {s.plan.emoji} {s.plan.title}
            </span>
            <span class="pi-meta">
              {s.plan.status === 'paused'
                ? '暂停中'
                : s.plan.mode === 'content'
                  ? `${s.progress.percent}%`
                  : s.progress.streak > 0
                    ? `🔥${s.progress.streak}`
                    : ''}
              {s.progress.short_minutes > 0 && s.plan.status === 'active' ? ' ⚠' : ''}
            </span>
            {s.plan.mode === 'content' && (
              <span class="pi-bar">
                <span style={{ width: `${s.progress.percent}%` }} />
              </span>
            )}
          </button>
        ))}
        <button type="button" class={`nav-item new-btn ${view.kind === 'new' ? 'active' : ''}`} onClick={() => setView({ kind: 'new', text: '' })}>
          ＋ 新建计划
        </button>
        {old.length + oldGoals.length > 0 && (
          <>
            <button type="button" class="nav-group as-btn" aria-expanded={showOld} onClick={() => setShowOld(!showOld)}>
              已完成 / 归档（{old.length + oldGoals.length}）{showOld ? '▾' : '▸'}
            </button>
            {showOld &&
              oldGoals.map((g) => (
                <button
                  type="button"
                  key={`g${g.goal.id}`}
                  class={`nav-item plan-item old ${isGoal(g.goal.id) ? 'active' : ''}`}
                  onClick={() => setView({ kind: 'goal', id: g.goal.id })}
                >
                  <span class="pi-title">
                    {g.goal.emoji} {g.goal.title}
                  </span>
                  <span class="pi-meta">{g.goal.status === 'done' ? '✓' : ''}</span>
                </button>
              ))}
            {showOld &&
              old.map((s) => (
                <button
                  type="button"
                  key={s.plan.id}
                  class={`nav-item plan-item old ${isPlan(s.plan.id) ? 'active' : ''}`}
                  onClick={() => setView({ kind: 'plan', id: s.plan.id })}
                >
                  <span class="pi-title">
                    {s.plan.emoji} {s.plan.title}
                  </span>
                  <span class="pi-meta">{s.plan.status === 'done' ? '✓' : ''}</span>
                </button>
              ))}
          </>
        )}
        <div class="nav-spacer" />
        <button type="button" class={`nav-item ${view.kind === 'settings' ? 'active' : ''}`} onClick={() => setView({ kind: 'settings' })}>
          ⚙ {boot.character.name || '她'}怎么陪你
        </button>
      </nav>
      <main class="page plans-page" ref={mainRef}>
        {error && (
          <div class="alert alert-error" role="alert">
            {error}
          </div>
        )}
        {focus && <FocusBar focus={focus} api={api} onError={onError} />}
        {view.kind === 'today' ? (
          <TodayPage
            api={api}
            boot={boot}
            tick={tick}
            onOpenPlan={(id) => setView({ kind: 'plan', id })}
            onNew={() => setView({ kind: 'new', text: '' })}
            onOpenCalendar={(day) => setView({ kind: 'calendar', day })}
            onError={onError}
            onFocus={focus ? undefined : startFocus}
          />
        ) : view.kind === 'calendar' ? (
          <CalendarPage api={api} today={boot.today} name={boot.character.name || '她'} tick={tick} initialDay={view.day} onError={onError} />
        ) : view.kind === 'stats' ? (
          <StatsPage api={api} tick={tick} onOpenPlan={(id) => setView({ kind: 'plan', id })} onOpenGoal={(id) => setView({ kind: 'goal', id })} onError={onError} />
        ) : view.kind === 'new' ? (
          <NewPlanPage
            api={api}
            boot={boot}
            initialText={view.text}
            onCreated={(id) => {
              changed();
              setView({ kind: 'plan', id });
            }}
            onCreatedGoal={(id) => {
              changed();
              setView({ kind: 'goal', id });
            }}
            onError={onError}
          />
        ) : view.kind === 'plan' ? (
          <PlanPage
            api={api}
            boot={boot}
            id={view.id}
            tick={tick}
            goal={goalOf(view.id)?.goal ?? null}
            onOpenGoal={(id) => setView({ kind: 'goal', id })}
            onDeleted={() => {
              changed();
              setView({ kind: 'today' });
            }}
            onError={onError}
          />
        ) : view.kind === 'goal' ? (
          <GoalPage
            api={api}
            boot={boot}
            id={view.id}
            tick={tick}
            focus={view.focus}
            onOpenPlan={(id) => setView({ kind: 'plan', id })}
            onDeleted={() => {
              changed();
              setView({ kind: 'today' });
            }}
            onError={onError}
          />
        ) : (
          <SupervisePage api={api} boot={boot} onSaved={(s) => setBoot({ ...boot, settings: s })} onError={onError} />
        )}
      </main>
    </div>
  );
}
