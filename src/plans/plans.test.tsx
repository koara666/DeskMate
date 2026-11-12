import { afterEach, describe, expect, it } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/preact';
import { PlansApp, toView } from './PlansApp';
import { lineToItem } from './editors';
import { flowLayout, flowSteps, goalTree, defaultOpen, timelineLayout, treeLayout } from './goalUtil';
import { addDays, mins, relDay } from './util';
import type { PlansApi } from '../lib/ipc';
import type {
  GoalDraft,
  GoalFull,
  GoalViz,
  Plan,
  PlanDraft,
  PlanFocus,
  PlanItem,
  PlanItemView,
  PlanProgress,
  PlanSettings,
  PlanTask,
  PlanTaskView,
  PlanToday,
  PlanView,
} from '../lib/types';

afterEach(cleanup);

const flush = () =>
  act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });

const TODAY = '2026-11-09';

const settings: PlanSettings = {
  supervise: true,
  strictness: 'normal',
  start_remind: true,
  distraction: true,
  checkin: true,
  checkin_at: '21:30',
  celebrate: true,
  chat_context: true,
  briefing: true,
  focus_minutes: 25,
  milestone_check: true,
  review: true,
  review_at: '20:00',
};

/** 虚构的计划书：两条内容线 + 一条每周固定的事、两个阶段、一段缓冲、一个节点 */
function mkGoalFull(): GoalFull {
  const plan = (id: number, title: string, extra: Partial<Plan> = {}): Plan => ({ ...mkPlan(id, title), goal_id: 7, share: 1, ...extra });
  const item = (id: number, plan_id: number, title: string, minutes: number, phase_id: number, extra = {}) => ({
    id,
    plan_id,
    title,
    detail: '',
    minutes,
    status: 'todo' as const,
    sort: 0,
    done_at: null,
    phase_id,
    rough: false,
    ...extra,
  });
  return {
    goal: {
      id: 7,
      title: '考证准备',
      emoji: '🎯',
      summary: '半年后通过考试',
      source: '',
      doc: '# 🎯 考证准备\n',
      strategy: '分两段，几条线轮流来。',
      start_day: TODAY,
      end_day: '2027-03-31',
      week_minutes: [60, 60, 60, 60, 60, 90, 90],
      off_days: [],
      buffers: [{ start: '2027-01-11', end: '2027-01-17', percent: 0, note: '期末考试' }],
      resources: [],
      checklist: ['这周排的完成了几成？', '下周最重要的一件事是什么？'],
      answers: [],
      remind_at: '20:00',
      review_day: 6,
      status: 'active',
      sort: 0,
      created_at: 0,
      updated_at: 0,
      expanded_until: '',
    },
    tracks: [
      { plan: plan(11, '专业课', { emoji: '📚' }), items: [item(101, 11, '概念一', 300, 21, { status: 'done' }), item(102, 11, '概念二、三', 600, 22, { rough: true, detail: '概念二、概念三' })] },
      { plan: plan(12, '真题', { emoji: '📝' }), items: [item(201, 12, '第一套', 400, 21)] },
      { plan: plan(13, '联系老师', { mode: 'habit', week_minutes: [0, 0, 60, 0, 0, 0, 0], end_day: '2027-02-28', share: 0 }), items: [] },
    ],
    phases: [
      { id: 21, goal_id: 7, title: '打基础', start_day: TODAY, end_day: '2026-12-31', objective: '把基础打好', criteria: ['三个概念都讲得清楚'], week_minutes: null, shares: { '11': 60, '12': 40 }, sort: 0 },
      { id: 22, goal_id: 7, title: '冲刺', start_day: '2027-01-01', end_day: '2027-03-31', objective: '', criteria: [], week_minutes: [90, 90, 90, 90, 90, 150, 150], shares: {}, sort: 1 },
    ],
    milestones: [{ id: 31, goal_id: 7, phase_id: 21, plan_id: 11, title: '讲清楚概念一', day: '2026-11-30', criteria: '讲给同学听', status: 'todo', result: '', checked_at: null, sort: 0 }],
  };
}

function mkViz(full: GoalFull): GoalViz {
  const cell = (plan_id: number, share: number, total: number, done: number) => ({ plan_id, share, total, done, items: 1, items_done: done === total ? 1 : 0, rough: 0, finish: '2026-12-20' });
  return {
    phases: [
      { phase: full.phases[0], state: 'current', cells: [cell(11, 60, 300, 300), cell(12, 40, 400, 100)], total: 700, done: 400, percent: 57, finish: '2026-12-20' },
      { phase: full.phases[1], state: 'future', cells: [cell(11, 1, 600, 0), cell(12, 1, 0, 0)], total: 600, done: 0, percent: 0, finish: '' },
    ],
    tracks: full.tracks.map((t) => ({
      plan_id: t.plan.id,
      routine: t.plan.mode === 'habit',
      total: 0,
      done: 0,
      percent: 30,
      finish: '2027-02-20',
      loose: { ...cell(t.plan.id, 1, 0, 0), items: 0, items_done: 0 },
      items: [],
    })),
    short: [],
    missed: [],
    total: 1300,
    done: 400,
    percent: 30,
    finish: '2027-02-20',
    current_phase: 21,
    days: [[TODAY, [[12, '第一套（继续）', 60]]]],
  };
}

const progress: PlanProgress = {
  total_minutes: 480,
  done_minutes: 120,
  items_total: 4,
  items_done: 1,
  percent: 25,
  finish: '2026-11-20',
  short_minutes: 0,
  overdue: false,
  streak: 2,
  week_days: 3,
  week_done: 2,
};

function mkPlan(id: number, title: string): Plan {
  return {
    id,
    title,
    goal: '复习线性代数第1-4章',
    emoji: '📐',
    mode: 'content',
    start_day: TODAY,
    end_day: '2026-12-20',
    week_minutes: [60, 60, 60, 60, 60, 120, 120],
    off_days: [],
    pace: 'even',
    remind_at: '20:00',
    supervise: true,
    strategy: '先把前两章打牢，后面难的多给点时间。',
    status: 'active',
    sort: 1,
    created_at: 0,
    updated_at: 0,
  };
}

function mkTask(id: number, title: string, extra: Partial<PlanTaskView> = {}): PlanTaskView {
  return {
    id,
    plan_id: 1,
    item_id: 11,
    day: TODAY,
    orig_day: '',
    title,
    minutes: 60,
    start: '20:00',
    status: 'todo',
    spent: 0,
    finishes: false,
    note: '',
    source: 'auto',
    sort: id,
    reminded: false,
    done_at: null,
    created_at: 0,
    label: '📐 线性代数',
    ...extra,
  };
}

function fakeApi() {
  const calls: Record<string, unknown[]> = {};
  const log = (k: string, ...a: unknown[]) => {
    (calls[k] ??= []).push(a.length === 1 ? a[0] : a);
  };
  let tasks: PlanTaskView[] = [mkTask(1, '第1章（第一部分）'), mkTask(2, '交实验报告', { plan_id: null, item_id: null, label: '', start: '', minutes: 0, source: 'user' })];
  const plans = [mkPlan(1, '线性代数')];
  const items: PlanItemView[] = [
    { id: 11, plan_id: 1, title: '第1章 行列式', detail: '', minutes: 120, status: 'done', sort: 0, done_at: 1, spent: 120, left: 0 },
    { id: 12, plan_id: 1, title: '第2章 矩阵', detail: '', minutes: 120, status: 'todo', sort: 1, done_at: null, spent: 30, left: 90 },
  ];
  const today = (): PlanToday => ({
    day: TODAY,
    tasks,
    notes: [],
    streak: 3,
    planned: 60,
    done: tasks.filter((t) => t.status === 'done').reduce((s, t) => s + t.minutes, 0),
    upcoming: [{ day: addDays(TODAY, 1), entries: [{ plan_id: 1, label: '📐 线性代数', title: '第2章 矩阵（继续）', minutes: 60, stored: false }] }],
    plans: plans.map((plan) => ({ plan, progress })),
    say: '今天有 2 项要做，从哪个开始？',
  });
  const view = (id: number): PlanView => ({
    plan: plans.find((p) => p.id === id)!,
    items,
    progress,
    upcoming: today().upcoming,
    history: [{ day: addDays(TODAY, -1), planned: 60, done: 60, tasks: 1, tasks_done: 1, tasks_todo: 0 }],
    notes: [],
  });
  const draft = (text: string): PlanDraft => ({
    plan: { ...mkPlan(0, '读完人类简史'), goal: text, strategy: '每天读 30 页左右，周末多读一点。' },
    items: [
      { id: 0, plan_id: 0, title: '第1–30页', detail: '', minutes: 45, status: 'todo', sort: 0, done_at: null },
      { id: 0, plan_id: 0, title: '第31–60页', detail: '', minutes: 45, status: 'todo', sort: 1, done_at: null },
    ],
    source: 'ai',
    notes: ['没说每天多久，先按每天 1 小时排，可以在下面改。'],
    preview: { days: [[TODAY, [['第1–30页', 45]]]], finish: '2026-11-10', short_minutes: 0, total_minutes: 90, per_week: '每周 7 天，平均每天 1 小时' },
  });
  const noop = async () => () => {};
  let focusCb: ((f: PlanFocus | null) => void) | null = null;
  let goalSaved = false;
  const goalDraft = (): GoalDraft => {
    const full = mkGoalFull();
    full.goal.id = 0;
    return { full, viz: mkViz(full), source: 'ai', notes: ['没说考试周的具体日子，先按 1 月中旬留了缓冲。'] };
  };
  const api = {
    boot: async () => ({
      character: { id: 'nanami-chiaki', name: '千秋', color: '#e88fb0' },
      images: { idle: 'data:image/png;base64,AA==' },
      view: null,
      settings,
      today: TODAY,
    }),
    today: async () => today(),
    list: async () => plans.map((plan) => ({ plan, progress })),
    get: async (id: number) => view(id),
    draft: async (text: string, fixed: unknown) => {
      log('draft', text, fixed);
      return draft(text);
    },
    replan: async (id: number, text: string) => {
      log('replan', id, text);
      return { ...draft(''), plan: { ...mkPlan(1, '线性代数'), strategy: '后面两章难，多给了时间。' } };
    },
    preview: async (plan: Plan, its: PlanItem[]) => {
      log('preview', plan, its);
      return draft('').preview;
    },
    apply: async (plan: Plan, its: PlanItem[] | null) => {
      log('apply', plan, its);
      if (plan.id === 0) plans.push({ ...plan, id: 2 });
      return plan.id || 2;
    },
    setStatus: async (id: number, s: string) => log('setStatus', id, s),
    remove: async (id: number) => log('remove', id),
    saveTask: async (t: PlanTask) => {
      log('saveTask', t);
      tasks = [...tasks, { ...t, id: 99, label: '' }];
      return 99;
    },
    doneTask: async (id: number, spent: number | null, finished: boolean | null) => {
      log('doneTask', id, spent, finished);
      tasks = tasks.map((t) => (t.id === id ? { ...t, status: 'done', spent: spent ?? t.minutes } : t));
      return { all_today: false, plan_finished: false };
    },
    taskStatus: async (id: number, s: string) => {
      log('taskStatus', id, s);
      tasks = tasks.map((t) => (t.id === id ? { ...t, status: s as PlanTask['status'] } : t));
    },
    moveTask: async (id: number, day: string) => {
      log('moveTask', id, day);
      tasks = tasks.filter((t) => t.id !== id);
    },
    deleteTask: async (id: number) => log('deleteTask', id),
    addNote: async (n: unknown) => {
      log('addNote', n);
      return 1;
    },
    deleteNote: async (id: number) => log('deleteNote', id),
    saveSettings: async (s: PlanSettings) => {
      log('saveSettings', s);
      return s;
    },
    focusStart: async (id: number, m: number | null) => {
      log('focusStart', id, m);
      focusCb?.({ task_id: id, title: '第1章（第一部分）', ends_at: 1000 + 25 * 60, minutes: 25, now: 1000 });
      return { task_id: id, title: '第1章（第一部分）', ends_at: 1000 + 25 * 60, minutes: 25, now: 1000 };
    },
    focusStop: async (done: boolean) => {
      log('focusStop', done);
      focusCb?.(null);
    },
    focusState: async () => null,
    onFocus: async (cb: (f: PlanFocus | null) => void) => {
      focusCb = cb;
      return () => {};
    },
    openSettings: async () => log('openSettings'),
    onChanged: noop,
    onView: noop,
    onCharacterChanged: noop,
    onThinking: noop,
    think: async (text: string, _fixed: unknown, answers: unknown, complex: boolean | null) => {
      log('think', text, answers, complex);
      if (text.length < 40) return { kind: 'plan', draft: draft(text) };
      if (answers === null) return { kind: 'questions', questions: [{ q: '考试周是什么时候？', options: ['1月中旬期末考试'] }, { q: '现在的基础怎么样？', options: [] }] };
      return { kind: 'goal', draft: goalDraft() };
    },
    goalList: async () =>
      goalSaved ? [{ goal: mkGoalFull().goal, percent: 30, tracks: [11, 12, 13], phase: '打基础', next: ['讲清楚概念一', '2026-11-30'], short: false }] : [],
    goalGet: async (id: number) => {
      log('goalGet', id);
      const full = mkGoalFull();
      return { full, viz: mkViz(full), week: { from: TODAY, to: addDays(TODAY, 6), planned: 240, done: 120, tasks: 4, tasks_done: 2, tracks: [{ plan_id: 11, planned: 120, done: 120, tasks: 2, tasks_done: 2 }, { plan_id: 12, planned: 120, done: 0, tasks: 2, tasks_done: 0 }] }, notes: [] };
    },
    goalPreview: async (full: GoalFull) => {
      log('goalPreview', full);
      return mkViz(full);
    },
    goalApply: async (full: GoalFull, withItems: boolean) => {
      log('goalApply', full, withItems);
      goalSaved = true;
      return 7;
    },
    goalSetStatus: async (id: number, s: string) => log('goalSetStatus', id, s),
    goalDelete: async (id: number) => log('goalDelete', id),
    goalExpand: async (id: number, text: string) => {
      log('goalExpand', id, text);
      const d = goalDraft();
      d.full.goal.id = 7;
      d.full.goal.strategy = '这周真题落下了，下周先补真题。';
      return d;
    },
    goalRethink: async (id: number, doc: string) => {
      log('goalRethink', id, doc);
      return goalDraft();
    },
    goalRenderDoc: async () => '# 重新生成的原文',
    milestoneCheck: async (id: number, s: string) => log('milestoneCheck', id, s),
  };
  return { api: api as unknown as PlansApi, calls };
}

describe('计划窗口', () => {
  it('今天：她的一句话、事项、勾掉、只做了一部分、推到明天', async () => {
    const f = fakeApi();
    render(<PlansApp api={f.api} />);
    await flush();
    await flush();
    expect(screen.getByText('今天有 2 项要做，从哪个开始？')).toBeTruthy();
    expect(screen.getByText('🔥 连续 3 天')).toBeTruthy();
    expect(screen.getByText('第1章（第一部分）')).toBeTruthy();
    expect(screen.getByRole('button', { name: '📐 第2章 矩阵（继续） · 1h' })).toBeTruthy();
    // 左边有计划和进度
    expect(screen.getByText('25%')).toBeTruthy();

    fireEvent.click(screen.getByRole('checkbox', { name: '做完了：第1章（第一部分）' }));
    await flush();
    expect(f.calls.doneTask).toEqual([[1, null, null]]);
    expect(screen.getByRole('checkbox', { name: '改回没做：第1章（第一部分）' }).getAttribute('aria-checked')).toBe('true');

    // 单独的事情：只做了一部分（没有关联内容，不问「做完了没」）
    fireEvent.click(screen.getByRole('button', { name: '更多：交实验报告' }));
    fireEvent.click(screen.getByRole('menuitem', { name: '推到明天' }));
    await flush();
    expect(f.calls.moveTask).toEqual([[2, '2026-11-10']]);
  });

  it('随手加一件事、写备注', async () => {
    const f = fakeApi();
    render(<PlansApp api={f.api} />);
    await flush();
    await flush();
    fireEvent.input(screen.getByLabelText('加一件事'), { target: { value: '取快递' } });
    fireEvent.input(screen.getByLabelText('大概多少分钟'), { target: { value: '15' } });
    fireEvent.click(screen.getByRole('button', { name: '加上' }));
    await flush();
    const t = f.calls.saveTask[0] as PlanTask;
    expect([t.title, t.minutes, t.day, t.plan_id]).toEqual(['取快递', 15, TODAY, null]);
    expect(screen.getByText('取快递')).toBeTruthy();

    fireEvent.input(screen.getByLabelText('今天的备注'), { target: { value: '状态不错' } });
    fireEvent.click(screen.getByRole('button', { name: '记下' }));
    await flush();
    expect(f.calls.addNote).toEqual([{ plan_id: null, day: TODAY, content: '状态不错' }]);
  });

  it('新建计划：写一句话 → 她安排 → 看一看 → 开始', async () => {
    const f = fakeApi();
    render(<PlansApp api={f.api} />);
    await flush();
    await flush();
    fireEvent.click(screen.getByRole('button', { name: '＋ 新建计划' }));
    await flush();
    const go = screen.getByRole('button', { name: '让她来安排' }) as HTMLButtonElement;
    expect(go.disabled).toBe(true);
    fireEvent.input(screen.getByLabelText('想做什么计划'), { target: { value: '一个月读完人类简史' } });
    fireEvent.click(go);
    await flush();
    expect(f.calls.think[0]).toEqual(['一个月读完人类简史', null, null]);
    expect(screen.getByText('每天读 30 页左右，周末多读一点。')).toBeTruthy();
    expect(screen.getByText('没说每天多久，先按每天 1 小时排，可以在下面改。')).toBeTruthy();
    expect(screen.getByText('11月10日 周二')).toBeTruthy();
    // 改一条内容的时长：重新推算
    fireEvent.input(screen.getByLabelText('第 2 条要多少分钟'), { target: { value: '90' } });
    await act(async () => {
      await new Promise((r) => setTimeout(r, 350));
    });
    expect(f.calls.preview?.length).toBe(1);
    fireEvent.click(screen.getByRole('button', { name: '就这样，开始！' }));
    await flush();
    const [plan, items] = f.calls.apply[0] as [Plan, PlanItem[]];
    expect(plan.title).toBe('读完人类简史');
    expect(items.map((i) => i.minutes)).toEqual([45, 90]);
    await flush();
    // 跳到新计划的详情
    expect(screen.getByRole('tab', { name: '内容' })).toBeTruthy();
  });

  it('计划详情：进度、改内容、重新安排、暂停', async () => {
    const f = fakeApi();
    render(<PlansApp api={f.api} />);
    await flush();
    await flush();
    fireEvent.click(screen.getByRole('button', { name: /线性代数/ }));
    await flush();
    expect(screen.getAllByText('25%').length).toBe(2);
    expect(screen.getByText(/按现在的进度，预计/)).toBeTruthy();
    fireEvent.input(screen.getByLabelText('第 2 条的名字'), { target: { value: '第2章 矩阵和逆' } });
    fireEvent.click(screen.getByRole('button', { name: '保存内容' }));
    await flush();
    const [, its] = f.calls.apply[0] as [Plan, PlanItem[]];
    expect(its[1].title).toBe('第2章 矩阵和逆');

    fireEvent.click(screen.getByRole('button', { name: '重新安排' }));
    fireEvent.input(screen.getByLabelText('想怎么调整'), { target: { value: '周末不想学了' } });
    fireEvent.click(screen.getByRole('button', { name: '让她重新排' }));
    await flush();
    expect(f.calls.replan).toEqual([[1, '周末不想学了']]);
    expect(screen.getByText('后面两章难，多给了时间。')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '按这个改' }));
    await flush();
    expect(f.calls.apply.length).toBe(2);

    fireEvent.click(screen.getByRole('button', { name: '暂停' }));
    await flush();
    expect(f.calls.setStatus).toEqual([[1, 'paused']]);

    fireEvent.click(screen.getByRole('tab', { name: '日历' }));
    expect(screen.getByRole('grid', { name: '日历' })).toBeTruthy();
  });

  it('专注计时：开始、显示倒计时、做完了', async () => {
    const f = fakeApi();
    render(<PlansApp api={f.api} />);
    await flush();
    await flush();
    fireEvent.click(screen.getByRole('button', { name: '开始专注：第1章（第一部分）' }));
    await flush();
    expect(f.calls.focusStart).toEqual([[1, null]]);
    expect(screen.getByRole('timer')).toBeTruthy();
    // 专注中不能再开一个
    expect(screen.queryByRole('button', { name: '开始专注：交实验报告' })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: '做完了' }));
    await flush();
    expect(f.calls.focusStop).toEqual([true]);
    expect(screen.queryByRole('timer')).toBeNull();
  });

  it('她怎么陪你：改了马上保存', async () => {
    const f = fakeApi();
    render(<PlansApp api={f.api} />);
    await flush();
    await flush();
    fireEvent.click(screen.getByRole('button', { name: '⚙ 千秋怎么陪你' }));
    fireEvent.click(screen.getByRole('radio', { name: '严格' }));
    await flush();
    expect((f.calls.saveSettings[0] as PlanSettings).strictness).toBe('strict');
  });
});

describe('计划书', () => {
  const LONG = '【目标】半年后通过专业资格考试\n【要做的】\n1. 专业课\n2. 真题\n【时间】2026年11月9日〜2027年3月31日，分阶段';

  it('复杂的计划：先问 → 回答 → 计划书草稿（时间线）→ 点阶段改份额 → 开始', async () => {
    const f = fakeApi();
    render(<PlansApp api={f.api} />);
    await flush();
    await flush();
    fireEvent.click(screen.getByRole('button', { name: '＋ 新建计划' }));
    await flush();
    fireEvent.input(screen.getByLabelText('想做什么计划'), { target: { value: LONG } });
    fireEvent.click(screen.getByRole('button', { name: '让她来安排' }));
    await flush();
    expect(screen.getByText('先问你几句')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '1月中旬期末考试' }));
    fireEvent.click(screen.getByRole('button', { name: '回答好了，开始排' }));
    await flush();
    const [, answers, complex] = f.calls.think[1] as unknown[];
    expect(answers).toEqual([
      { q: '考试周是什么时候？', a: '1月中旬期末考试' },
      { q: '现在的基础怎么样？', a: '' },
    ]);
    expect(complex).toBe(true);
    expect(screen.getByText('分两段，几条线轮流来。')).toBeTruthy();
    expect(screen.getByLabelText('计划书时间线')).toBeTruthy();
    // 点阶段：下面出来阶段的编辑，改份额就重新推算
    fireEvent.click(screen.getByRole('button', { name: '阶段：打基础' }));
    await flush();
    expect(screen.getByText(/阶段：打基础/, { selector: 'h2' })).toBeTruthy();
    fireEvent.input(screen.getByLabelText('真题的份额'), { target: { value: '60' } });
    await act(async () => {
      await new Promise((r) => setTimeout(r, 400));
    });
    const previewed = f.calls.goalPreview?.at(-1) as GoalFull;
    expect(previewed.phases[0].shares).toEqual({ '11': 60, '12': 60 });
    fireEvent.click(screen.getByRole('button', { name: '就这样，开始！' }));
    await flush();
    const [full, withItems] = f.calls.goalApply[0] as [GoalFull, boolean];
    expect(withItems).toBe(true);
    expect(full.phases[0].shares['12']).toBe(60);
    await flush();
    // 跳到计划书，左边列出计划书
    expect(f.calls.goalGet).toEqual([7]);
    expect(screen.getByRole('tab', { name: '计划书' })).toBeTruthy();
    expect(screen.getByRole('button', { name: /🎯 考证准备/ })).toBeTruthy();
  });

  it('跳过问题也能排；简单的照旧一步到位', async () => {
    const f = fakeApi();
    render(<PlansApp api={f.api} />);
    await flush();
    await flush();
    fireEvent.click(screen.getByRole('button', { name: '＋ 新建计划' }));
    fireEvent.input(screen.getByLabelText('想做什么计划'), { target: { value: LONG } });
    fireEvent.click(screen.getByRole('button', { name: '让她来安排' }));
    await flush();
    fireEvent.click(screen.getByRole('button', { name: '跳过，直接排' }));
    await flush();
    expect((f.calls.think[1] as unknown[])[1]).toEqual([]);
    expect(screen.getByText('计划书：看看这样排行不行')).toBeTruthy();
  });

  it('计划书页：三种图、点节点改状态、调整下周、只保存原文', async () => {
    const f = fakeApi();
    render(<PlansApp api={f.api} />);
    await flush();
    await f.api.goalApply(mkGoalFull(), true);
    fireEvent.click(screen.getByRole('button', { name: '＋ 新建计划' }));
    await flush();
    fireEvent.click(screen.getByRole('button', { name: /🎯 考证准备/ }));
    await flush();
    expect(screen.getByText('57%')).toBeTruthy();
    expect(screen.getByRole('button', { name: /◆ 讲清楚概念一/ })).toBeTruthy();
    // 时间线上有线、阶段、节点、缓冲
    expect(screen.getByRole('button', { name: '线：专业课' })).toBeTruthy();
    expect(screen.getByRole('button', { name: '缓冲：期末考试' })).toBeTruthy();
    fireEvent.click(screen.getByRole('radio', { name: '树状图' }));
    expect(screen.getByLabelText('计划书树状图')).toBeTruthy();
    expect(screen.getByRole('button', { name: /打基础，57%/ })).toBeTruthy();
    fireEvent.click(screen.getByRole('radio', { name: '流程图' }));
    expect(screen.getByLabelText('计划书流程图')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '节点：讲清楚概念一（待确认）' }));
    await flush();
    fireEvent.click(screen.getByRole('radio', { name: '达标' }));
    fireEvent.click(screen.getByRole('button', { name: '保存修改' }));
    await flush();
    const [saved, withItems] = f.calls.goalApply.at(-1) as [GoalFull, boolean];
    expect(saved.milestones[0].status).toBe('passed');
    expect(withItems).toBe(false);
    // 这周：落下的线标出来；调整下周
    fireEvent.click(screen.getByRole('tab', { name: '这周' }));
    expect(screen.getByText(/0\/2 项 · 0 分钟 · 落下了/)).toBeTruthy();
    expect(screen.getByText('这周排的完成了几成？')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '写一句要求' }));
    fireEvent.input(screen.getByLabelText('想怎么调整下周'), { target: { value: '下周三有考试' } });
    fireEvent.click(screen.getByRole('button', { name: '让她调整' }));
    await flush();
    expect(f.calls.goalExpand).toEqual([[7, '下周三有考试']]);
    expect(screen.getByText('这周真题落下了，下周先补真题。')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '按这个改' }));
    await flush();
    expect((f.calls.goalApply.at(-1) as unknown[])[1]).toBe(true);
    // 原文：改了只保存文字
    fireEvent.click(screen.getByRole('tab', { name: '原文' }));
    fireEvent.input(screen.getByLabelText('计划书原文'), { target: { value: '# 改过的原文' } });
    fireEvent.click(screen.getByRole('button', { name: '只保存文字' }));
    await flush();
    const [d] = f.calls.goalApply.at(-1) as [GoalFull, boolean];
    expect(d.goal.doc).toBe('# 改过的原文');
  });

  it('三种图的布局', () => {
    const full = mkGoalFull();
    const viz = mkViz(full);
    const t = timelineLayout(full, viz, 800, '2026-11-20');
    expect(t.rows.map((r) => r.title)).toEqual(['专业课', '真题', '联系老师']);
    expect(t.phases[0].x).toBeLessThan(t.phases[1].x);
    expect(t.phases[0].current).toBe(true);
    expect(t.today).not.toBeNull();
    expect(t.cells.filter((c) => c.plan_id === 11).length).toBe(2);
    expect(t.cells.find((c) => c.plan_id === 11 && c.phase === 21)?.pct).toBe(1);
    // 每周三联系老师：到 2 月底，每周一个点
    const ticks = t.ticks.filter((x) => x.plan_id === 13);
    expect(ticks.length).toBeGreaterThan(14);
    expect(ticks.length).toBeLessThan(18);
    expect(t.months[0].label).toBe('12月');
    expect(t.months.some((m) => m.label === '2027年1月')).toBe(true);
    // 树：父节点在孩子中间
    const root = goalTree(full, viz);
    expect(root.children.map((c) => c.title)).toEqual(['打基础', '冲刺']);
    const open = defaultOpen(root);
    expect([...open]).toEqual(['g', 'p21', 'p22']);
    const tl = treeLayout(root, open, 640);
    expect(tl.width).toBeLessThanOrEqual(640);
    const phase = tl.boxes.find((b) => b.node.key === 'p21')!;
    const kids = tl.edges.filter((e) => e.from === phase).map((e) => e.to.y);
    expect(phase.y).toBe((Math.min(...kids) + Math.max(...kids)) / 2);
    // 流程图：按日期排（节点在它的阶段后面），一行放不下就蛇形往回走
    const steps = flowSteps(full, viz);
    expect(steps.map((s) => s.kind)).toEqual(['phase', 'milestone', 'phase']);
    const fl = flowLayout(steps, 450);
    expect(fl.boxes[0].row).toBe(0);
    expect(fl.boxes[2].row).toBe(1);
    expect(fl.boxes[2].x).toBeGreaterThan(fl.boxes[0].x);
    expect(fl.links.length).toBe(2);
  });
});

describe('小工具', () => {
  it('页面名、日期、时长', () => {
    expect(toView('plan:12')).toEqual({ kind: 'plan', id: 12 });
    expect(toView('goal:3:m5')).toEqual({ kind: 'goal', id: 3, focus: 'm5' });
    expect(toView('goal:3')).toEqual({ kind: 'goal', id: 3, focus: undefined });
    expect(toView('goal:3:x')).toBeNull();
    expect(toView('new:每天背单词')).toEqual({ kind: 'new', text: '每天背单词' });
    expect(toView('../x')).toBeNull();
    expect(relDay('2026-11-10', TODAY)).toBe('明天');
    expect(relDay('2026-11-12', TODAY)).toBe('周四');
    expect(relDay('2026-12-01', TODAY)).toBe('12月1日');
    expect(mins(90)).toBe('1 小时 30 分');
  });

  it('一行一条地加内容', () => {
    expect(lineToItem('1. 第4章 特征值 2小时')).toMatchObject({ title: '第4章 特征值', minutes: 120 });
    expect(lineToItem('- 第5章（90分钟）')).toMatchObject({ title: '第5章', minutes: 90 });
    expect(lineToItem('刷题 1.5h')).toMatchObject({ title: '刷题', minutes: 90 });
    expect(lineToItem('整理笔记')).toMatchObject({ title: '整理笔记', minutes: 60 });
    expect(lineToItem('   ')).toBeNull();
  });
});
