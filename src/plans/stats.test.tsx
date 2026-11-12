import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/preact';
import { PlansApp, toView } from './PlansApp';
import { StatsPage, heatLevel, heatText, niceAxis, seriesColors } from './StatsPage';
import { addDays } from './util';
import type { PlansApi } from '../lib/ipc';
import type { Plan, PlanStats } from '../lib/types';

afterEach(cleanup);

const flush = () =>
  act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });

const TODAY = '2026-11-11'; // 周三
const START = '2026-07-27'; // 16 周前的周一

/** 虚构的统计：到周三为止，16 周 = 15 × 7 + 3 = 108 格 */
function mkStats(over: Partial<PlanStats> = {}): PlanStats {
  const special: Record<string, [number, number, number]> = {
    '2026-11-11': [30, 1, 2],
    '2026-11-10': [80, 3, 4],
    '2026-11-09': [150, 4, 4],
    '2026-11-02': [45, 1, 1],
  };
  const heat = Array.from({ length: 108 }, (_, i) => {
    const day = addDays(START, i);
    const [minutes, done, total] = special[day] ?? [0, 0, 0];
    return { day, minutes, done, total };
  });
  const part = (key: string, minutes: number) => ({ key, minutes });
  const week = (i: number, parts: { key: string; minutes: number }[]) => ({
    start: addDays('2026-09-21', 7 * i),
    minutes: parts.reduce((s, p) => s + p.minutes, 0),
    parts,
  });
  return {
    today: TODAY,
    heat_start: START,
    heat,
    series: [
      { key: 'p1', label: '📐 线性代数' },
      { key: 'p2', label: '📝 真题' },
      { key: 'loose', label: '随手的事' },
    ],
    weeks: [
      week(0, []),
      week(1, [part('p1', 120)]),
      week(2, [part('p1', 200), part('p2', 100)]),
      week(3, []),
      week(4, [part('p1', 300), part('p2', 100), part('loose', 50)]),
      week(5, [part('p1', 200), part('p2', 180)]),
      week(6, [part('p1', 150), part('loose', 110)]),
      week(7, [part('p1', 100), part('p2', 30), part('loose', 20)]),
    ],
    numbers: { week_minutes: 320, last_week_minutes: 260, rate: 82, rate_done: 23, rate_total: 28, streak: 4, best_streak: 9 },
    plans: [
      { plan_id: 1, title: '线性代数', emoji: '📐', status: 'active', goal_id: null, goal_title: null, weeks: [0, 0, 150, 100], total_minutes: 1070, done_items: 21 },
      { plan_id: 2, title: '真题', emoji: '📝', status: 'active', goal_id: 7, goal_title: '考证准备', weeks: [0, 100, 180, 30], total_minutes: 410, done_items: 6 },
      { plan_id: 3, title: '旧书', emoji: '📖', status: 'paused', goal_id: null, goal_title: null, weeks: [0, 0, 0, 0], total_minutes: 0, done_items: 0 },
    ],
    review: '上周做了 4 小时 20 分钟，比前一周多 40 分钟；做得最多的是周三（2 小时）；「线性代数」做得最多。',
    ...over,
  };
}

const noop = vi.fn();
const statsApi = (s: PlanStats) => ({ stats: async () => s }) as unknown as Pick<PlansApi, 'stats'>;

async function mount(s: PlanStats, extra: Partial<Parameters<typeof StatsPage>[0]> = {}) {
  const r = render(<StatsPage api={statsApi(s)} tick={0} onOpenPlan={noop} onOpenGoal={noop} onError={noop} {...extra} />);
  await flush();
  return r;
}

describe('统计页', () => {
  it('数字卡：本周、上周、完成率、连续 / 最长连续，和上周回顾', async () => {
    const { container } = await mount(mkStats());
    const cards = [...container.querySelectorAll('.stat-card')].map((c) => c.textContent);
    expect(cards).toHaveLength(4);
    expect(cards[0]).toContain('本周');
    expect(cards[0]).toContain('5 小时 20 分');
    expect(cards[1]).toContain('上周');
    expect(cards[1]).toContain('4 小时 20 分');
    expect(cards[2]).toContain('82%');
    expect(cards[2]).toContain('做完 23/28 项');
    expect(cards[3]).toContain('🔥 4 天');
    expect(cards[3]).toContain('最长连续 9 天');
    expect(container.querySelector('.stat-review')?.textContent).toContain('上周做了 4 小时 20 分钟，比前一周多 40 分钟');
  });

  it('没有安排过时完成率写横线，没有连续就不画火', async () => {
    const s = mkStats();
    s.numbers = { ...s.numbers, rate: null, rate_done: 0, rate_total: 0, streak: 0, best_streak: 0 };
    const { container } = await mount(s);
    const cards = [...container.querySelectorAll('.stat-card')].map((c) => c.textContent ?? '');
    expect(cards[2]).toContain('—');
    expect(cards[2]).toContain('过去 28 天还没有安排');
    expect(cards[3]).not.toContain('🔥');
    expect(cards[3]).toContain('0 天');
  });

  it('热力图：16 列 × 7 行减去以后的日子，今天描边，悬停和聚焦显示详情', async () => {
    const { container } = await mount(mkStats());
    const cells = container.querySelectorAll<HTMLElement>('.heat-cell');
    expect(cells).toHaveLength(16 * 7 - 4); // 周三之后还有周四到周日四格不画
    expect(container.querySelectorAll('.heat-cell.today')).toHaveLength(1);
    expect(container.querySelector('.heat-cell.today')?.getAttribute('aria-label')).toBe('11月11日 周三 · 30 分钟 · 做完 1/2 项');
    // 格子在第几列第几行：周一在第一行，往后一天往下一行
    const first = cells[0];
    expect([first.style.gridColumn, first.style.gridRow]).toEqual(['2', '2']);
    expect([cells[8].style.gridColumn, cells[8].style.gridRow]).toEqual(['3', '3']);
    // 颜色按分钟分档
    const tue = container.querySelector<HTMLElement>('[aria-label^="11月10日"]')!;
    expect(tue.className).toContain('lv-3');
    expect(container.querySelector('[aria-label^="11月9日"]')?.className).toContain('lv-4');
    expect(container.querySelector('[aria-label^="11月2日"]')?.className).toContain('lv-2');
    expect(first.className).toContain('lv-0');
    // 悬停
    expect(screen.queryByRole('tooltip')).toBeNull();
    fireEvent.mouseEnter(tue);
    expect(screen.getByRole('tooltip').textContent).toBe('11月10日 周二 · 1 小时 20 分 · 做完 3/4 项');
    fireEvent.mouseLeave(tue);
    expect(screen.queryByRole('tooltip')).toBeNull();
    // 键盘聚焦显示同样的详情
    act(() => first.focus());
    expect(screen.getByRole('tooltip').textContent).toBe('7月27日 周一 · 没有安排');
  });

  it('热力图：方向键在格子间移动，只有一个格子在 Tab 顺序里', async () => {
    const { container } = await mount(mkStats());
    const cells = [...container.querySelectorAll<HTMLElement>('.heat-cell')];
    expect(cells.filter((c) => c.tabIndex === 0)).toHaveLength(1);
    const today = container.querySelector<HTMLElement>('.heat-cell.today')!;
    expect(today.tabIndex).toBe(0);
    act(() => today.focus());
    fireEvent.keyDown(today, { key: 'ArrowUp' });
    expect(document.activeElement?.getAttribute('aria-label')).toMatch(/^11月10日/);
    fireEvent.keyDown(document.activeElement!, { key: 'ArrowLeft' });
    expect(document.activeElement?.getAttribute('aria-label')).toMatch(/^11月3日/);
    fireEvent.keyDown(document.activeElement!, { key: 'Home' });
    expect(document.activeElement?.getAttribute('aria-label')).toMatch(/^7月27日/);
    // 走到头不会越界
    fireEvent.keyDown(document.activeElement!, { key: 'ArrowLeft' });
    expect(document.activeElement?.getAttribute('aria-label')).toMatch(/^7月27日/);
    expect(cells.filter((c) => c.tabIndex === 0)).toHaveLength(1);
  });

  it('堆叠柱：8 根，每段一种计划的颜色，上面写总时长，图例每种颜色写名字，悬停列出每段', async () => {
    const s = mkStats();
    const { container } = await mount(s);
    expect(container.querySelectorAll('.bar-col')).toHaveLength(8);
    expect(container.querySelectorAll('.bar-seg')).toHaveLength(s.weeks.reduce((n, w) => n + w.parts.length, 0));
    // 总时长
    const totals = [...container.querySelectorAll('.bar-total')].map((t) => t.textContent);
    expect(totals).toEqual(['0', '2h', '5h', '0', '7.5h', '6.3h', '4.3h', '2.5h']);
    // 图例
    const legend = [...container.querySelectorAll('.stat-legend li')].map((li) => li.textContent);
    expect(legend).toEqual(['📐 线性代数', '📝 真题', '随手的事']);
    // 同一个计划在每根柱子里颜色一样；随手的事是中性灰
    const fills = (week: number) => [...container.querySelectorAll('.bar-col')[week].querySelectorAll<SVGElement>('.bar-seg')].map((e) => e.style.fill);
    expect(fills(4)).toEqual(['var(--lane-0)', 'var(--lane-1)', 'var(--lane-other)']);
    expect(fills(1)).toEqual(['var(--lane-0)']);
    expect(fills(7)).toEqual(['var(--lane-0)', 'var(--lane-1)', 'var(--lane-other)']);
    // 悬停：一根柱子的每一段
    const col = container.querySelector<SVGElement>('[data-week="2026-10-05"]')!;
    fireEvent.mouseEnter(col);
    const tip = screen.getByRole('tooltip').textContent ?? '';
    expect(tip).toContain('10月5日 – 10月11日');
    expect(tip).toContain('共 5 小时');
    expect(tip).toContain('3 小时 20 分');
    expect(tip).toContain('1 小时 40 分');
    // 本周那根写明是到今天为止
    fireEvent.focus(container.querySelector('[data-week="2026-11-09"]')!);
    expect(screen.getByRole('tooltip').textContent).toContain('本周');
    // 没有做完的事的那周也有说明
    fireEvent.focus(container.querySelector('[data-week="2026-09-21"]')!);
    expect(screen.getByRole('tooltip').textContent).toContain('没有做完的事');
    // 表格版：每周一行，值都在
    const rows = container.querySelectorAll('.stat-table tbody tr');
    expect(rows).toHaveLength(8);
    expect(rows[4].textContent).toContain('7 小时 30 分');
  });

  it('柱子的颜色：计划按图例顺序取线的颜色，其他计划深灰，随手的事中性灰', () => {
    const m = seriesColors([
      { key: 'p5', label: 'a' },
      { key: 'p9', label: 'b' },
      { key: 'other', label: '其他计划' },
      { key: 'loose', label: '随手的事' },
    ]);
    expect([...m.entries()]).toEqual([
      ['p5', 'var(--lane-0)'],
      ['p9', 'var(--lane-1)'],
      ['other', 'var(--stat-rest)'],
      ['loose', 'var(--lane-other)'],
    ]);
  });

  it('每个计划一行：名字、近 4 周的小条、总共做了多少；点名字跳到那个计划，计划书的线跳到计划书', async () => {
    const onOpenPlan = vi.fn();
    const onOpenGoal = vi.fn();
    const { container } = await mount(mkStats(), { onOpenPlan, onOpenGoal });
    const rows = [...container.querySelectorAll('.stat-plan')];
    expect(rows).toHaveLength(3);
    expect(rows[0].textContent).toContain('共做了 17 小时 50 分');
    expect(rows[0].textContent).toContain('做完 21 项');
    expect(rows[0].textContent).toContain('近 4 周 4 小时 10 分');
    expect(rows[1].textContent).toContain('计划书「考证准备」');
    expect(rows[2].textContent).toContain('暂停中');
    // 每行 4 根小条（没做的那周只画一条细线）
    expect(rows[0].querySelectorAll('.stat-mini-bar')).toHaveLength(2);
    expect(rows[0].querySelectorAll('.stat-mini-zero')).toHaveLength(2);
    expect(rows[2].querySelectorAll('.stat-mini-bar')).toHaveLength(0);
    // 小条共用一把尺：真题上上周 180 分钟最高，线代上周 150 就矮一截
    const h = (row: Element) => [...row.querySelectorAll('.stat-mini-bar')].map((b) => Number(b.getAttribute('height')));
    expect(h(rows[0])[0]).toBeCloseTo((150 / 180) * 22);
    expect(h(rows[1])[1]).toBeCloseTo(22);
    fireEvent.click(rows[0].querySelector('.stat-plan-name')!);
    expect(onOpenPlan).toHaveBeenCalledWith(1);
    fireEvent.click(rows[1].querySelector('.stat-plan-name')!);
    expect(onOpenGoal).toHaveBeenCalledWith(7);
    expect(onOpenPlan).toHaveBeenCalledTimes(1);
  });

  it('空数据：一句提示，不画空图', async () => {
    const empty = mkStats();
    empty.heat = empty.heat.map((h) => ({ ...h, minutes: 0, done: 0, total: 0 }));
    empty.weeks = empty.weeks.map((w) => ({ ...w, minutes: 0, parts: [] }));
    empty.series = [];
    empty.plans = [];
    const { container } = await mount(empty);
    expect(screen.getByText('这里还没有记录。')).toBeTruthy();
    expect(container.querySelectorAll('.heat-cell')).toHaveLength(0);
    expect(container.querySelector('svg')).toBeNull();
    expect(container.querySelector('.stat-card')).toBeNull();
  });

  it('有安排但这 8 周没有做完的：柱状图那一块写一句提示，热力图还在', async () => {
    const s = mkStats();
    s.weeks = s.weeks.map((w) => ({ ...w, minutes: 0, parts: [] }));
    s.series = [];
    s.plans = [];
    const { container } = await mount(s);
    expect(screen.getByText('这 8 周还没有做完的事，做完一件就会出现在这里。')).toBeTruthy();
    expect(container.querySelectorAll('.bar-col')).toHaveLength(0);
    expect(container.querySelectorAll('.heat-cell')).toHaveLength(108);
    expect(screen.getByText('还没有进行中的计划。')).toBeTruthy();
  });

  it('计划有变化（tick 变了）重新取数据，先留着旧的图', async () => {
    let n = 0;
    const api = { stats: async () => mkStats({ numbers: { ...mkStats().numbers, week_minutes: 60 * ++n } }) } as unknown as Pick<PlansApi, 'stats'>;
    const { container, rerender } = render(<StatsPage api={api} tick={0} onOpenPlan={noop} onOpenGoal={noop} onError={noop} />);
    await flush();
    expect(container.querySelector('.stat-card')?.textContent).toContain('1 小时');
    rerender(<StatsPage api={api} tick={1} onOpenPlan={noop} onOpenGoal={noop} onError={noop} />);
    // 重新取的时候还是旧的内容
    expect(container.querySelector('.stat-card')?.textContent).toContain('1 小时');
    await flush();
    expect(container.querySelector('.stat-card')?.textContent).toContain('2 小时');
    expect(container.querySelector('.stats.loading')).toBeNull();
  });

  it('取数据失败：报错，不白屏', async () => {
    const onError = vi.fn();
    const api = { stats: async () => Promise.reject('读不出来') } as unknown as Pick<PlansApi, 'stats'>;
    const { container } = render(<StatsPage api={api} tick={0} onOpenPlan={noop} onOpenGoal={noop} onError={onError} />);
    await flush();
    expect(onError).toHaveBeenCalledWith('读不出来');
    expect(container.textContent).toContain('加载中');
  });
});

describe('统计页的小工具', () => {
  it('页面名', () => {
    expect(toView('stats')).toEqual({ kind: 'stats' });
  });

  it('颜色档和详情的写法', () => {
    expect([0, 1, 29, 30, 59, 60, 119, 120, 600].map(heatLevel)).toEqual([0, 1, 1, 2, 2, 3, 3, 4, 4]);
    expect(heatText({ day: '2026-11-03', minutes: 80, done: 3, total: 4 })).toBe('11月3日 周二 · 1 小时 20 分 · 做完 3/4 项');
    expect(heatText({ day: '2026-11-03', minutes: 0, done: 0, total: 0 })).toBe('11月3日 周二 · 没有安排');
  });

  it('纵轴刻度取整小时，最多四段', () => {
    expect(niceAxis(0)).toEqual({ max: 30, ticks: [0, 30] });
    expect(niceAxis(450).max).toBe(480);
    expect(niceAxis(380).max).toBe(480);
    expect(niceAxis(380).ticks).toEqual([0, 120, 240, 360, 480]);
    expect(niceAxis(60 * 30).ticks).toEqual([0, 600, 1200, 1800]);
    // 刻度顶上总是不低于最高的那根
    for (const m of [1, 59, 60, 61, 119, 500, 1234, 5000]) expect(niceAxis(m).max).toBeGreaterThanOrEqual(m);
  });
});

describe('统计页在计划窗口里', () => {
  const plan = (id: number, title: string, extra: Record<string, unknown> = {}) =>
    ({ id, title, emoji: '📐', mode: 'content', status: 'active', goal_id: null, share: 1, ...extra }) as unknown as Plan;
  const progress = { percent: 10, streak: 0, short_minutes: 0 };

  function appApi() {
    const calls = { get: [] as number[], goalGet: [] as number[] };
    const off = async () => () => {};
    const api = {
      boot: async () => ({ character: { id: 'x', name: '千秋', color: '#e88fb0' }, images: {}, view: 'stats', settings: {}, today: TODAY }),
      list: async () => [
        { plan: plan(1, '线性代数'), progress },
        { plan: plan(2, '真题', { goal_id: 7, emoji: '📝' }), progress },
      ],
      goalList: async () => [{ goal: { id: 7, title: '考证准备', emoji: '🎯', status: 'active' }, percent: 30, tracks: [2], phase: '', next: null, short: false }],
      stats: async () => mkStats(),
      get: (id: number) => {
        calls.get.push(id);
        return new Promise(() => {});
      },
      goalGet: (id: number) => {
        calls.goalGet.push(id);
        return new Promise(() => {});
      },
      focusState: async () => null,
      onChanged: off,
      onView: off,
      onCharacterChanged: off,
      onFocus: off,
    };
    return { api: api as unknown as PlansApi, calls };
  }

  it('左边有「统计」；点计划名字跳到那个计划，计划书的线跳到计划书；再点「统计」回来', async () => {
    const { api, calls } = appApi();
    const { container } = render(<PlansApp api={api} />);
    await flush();
    await flush();
    const current = () => container.querySelector('.plans-nav [aria-current="page"]')?.textContent ?? '';
    expect(current()).toContain('统计');
    expect(screen.getByRole('heading', { name: '统计' })).toBeTruthy();
    const names = container.querySelectorAll('.stat-plan-name');
    fireEvent.click(names[0]);
    await flush();
    expect(calls.get).toEqual([1]);
    expect(current()).toContain('线性代数');
    expect(screen.queryByRole('heading', { name: '统计' })).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: '📊 统计' }));
    await flush();
    expect(current()).toContain('统计');
    fireEvent.click(container.querySelectorAll('.stat-plan-name')[1]);
    await flush();
    expect(calls.goalGet).toEqual([7]);
    expect(current()).toContain('考证准备');
  });
});
