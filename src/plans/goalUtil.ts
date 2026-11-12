// 计划书的小工具：线的颜色、临时 id、三种图（时间线 / 树状图 / 流程图）的布局。布局都是纯函数（有测试），组件只管画。
import type { GoalFull, GoalMilestone, GoalPhase, GoalViz, Plan, PlanItem } from '../lib/types';
import { addDays, dayLabel, daysBetween, parseDay } from './util';

/** 线的颜色：按线在计划书里的顺序取（第 9 条起不再分颜色，用灰色），CSS 里 --lane-0 … --lane-7 分浅色 / 深色两套 */
export function laneColor(index: number): string {
  return index >= 0 && index < 8 ? `var(--lane-${index})` : 'var(--lane-other)';
}

// 后端发的临时 id 在 -1 … -3,000,000 之间，这里从更小的开始，免得撞上
let tmp = -10_000_000;
/** 新建的阶段、线、节点、条目用负数当临时 id（保存时换成真的） */
export function tmpId(): number {
  tmp -= 1;
  return tmp;
}

export function isRoutine(p: Plan): boolean {
  return p.mode === 'habit';
}

/** 计划书显示到哪天：结束日，或者最晚的阶段 / 节点 / 预计完成日（都没有就开始后 90 天） */
export function goalEnd(full: GoalFull, viz: GoalViz | null): string {
  const g = full.goal;
  let end = g.end_day || addDays(g.start_day, 90);
  const later = (d: string) => {
    if (d && d > end) end = d;
  };
  full.phases.forEach((p) => later(p.end_day));
  full.milestones.forEach((m) => later(m.day));
  viz?.tracks.forEach((t) => later(t.finish));
  return end;
}

// ---------------- 时间线 / 泳道 ----------------

export interface TimelineBox {
  x: number;
  w: number;
}

export interface TimelineLayout {
  width: number;
  height: number;
  labelW: number;
  start: string;
  end: string;
  /** 月份刻度 */
  months: { x: number; label: string }[];
  /** 阶段那一行 */
  phaseY: number;
  phases: (TimelineBox & { id: number; title: string; current: boolean })[];
  /** 节点那一行 */
  msY: number;
  /** labelW：名字能占多宽（到下一个节点为止；0 = 不写名字） */
  milestones: { id: number; x: number; title: string; status: string; day: string; labelW: number }[];
  /** 每条线一行 */
  rows: { plan_id: number; y: number; title: string; emoji: string; color: string; routine: boolean; short: boolean }[];
  rowH: number;
  tracksTop: number;
  /** 每条线每个阶段的一段（phase = null：不属于阶段的） */
  cells: (TimelineBox & { plan_id: number; phase: number | null; y: number; pct: number; total: number; done: number; rough: number; color: string })[];
  /** 每周固定的事：每周一个点 */
  ticks: { plan_id: number; x: number; y: number; color: string }[];
  buffers: (TimelineBox & { index: number; note: string; percent: number })[];
  today: number | null;
}

const ROW_H = 34;

/** 时间线的布局：横轴是日期，每条线一行，阶段、节点、缓冲期标在上面 */
export function timelineLayout(full: GoalFull, viz: GoalViz, width: number, today: string): TimelineLayout {
  const w = Math.max(560, Math.round(width));
  const labelW = Math.min(150, Math.max(96, Math.round(w * 0.2)));
  const x0 = labelW + 8;
  const plotW = w - x0 - 14;
  const start = full.goal.start_day;
  const end = goalEnd(full, viz);
  const span = Math.max(1, daysBetween(start, end) + 1);
  const x = (day: string) => x0 + (Math.min(span, Math.max(0, daysBetween(start, day))) / span) * plotW;
  const box = (a: string, b: string): TimelineBox => {
    const xa = x(a);
    return { x: xa, w: Math.max(3, x(addDays(b, 1)) - xa) };
  };
  // 月份刻度（太密就隔一个）
  const months: { x: number; label: string }[] = [];
  const s = parseDay(start);
  if (s) {
    const d = new Date(s.getFullYear(), s.getMonth() + 1, 1);
    const all: { x: number; label: string }[] = [];
    for (let n = 0; n < 60; n++) {
      const ds = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-01`;
      if (ds > end) break;
      all.push({ x: x(ds), label: d.getMonth() === 0 ? `${d.getFullYear()}年1月` : `${d.getMonth() + 1}月` });
      d.setMonth(d.getMonth() + 1);
    }
    const step = all.length > 1 && (all[1].x - all[0].x) < 36 ? 2 : 1;
    all.forEach((m, i) => {
      if (i % step === 0) months.push(m);
    });
  }
  const phaseY = 26;
  const msY = 62;
  const tracksTop = 92;
  const shortLanes = new Set(viz.short.map((s) => s.lane));
  const rows = full.tracks.map((t, i) => ({
    plan_id: t.plan.id,
    y: tracksTop + i * ROW_H,
    title: t.plan.title,
    emoji: t.plan.emoji,
    color: laneColor(i),
    routine: isRoutine(t.plan),
    short: shortLanes.has(t.plan.id),
  }));
  const rowOf = new Map(rows.map((r) => [r.plan_id, r]));
  const phases = full.phases.map((p) => ({
    id: p.id,
    title: p.title,
    current: today >= p.start_day && today <= p.end_day,
    ...box(p.start_day, p.end_day),
  }));
  const cells: TimelineLayout['cells'] = [];
  for (const pv of viz.phases) {
    for (const c of pv.cells) {
      const r = rowOf.get(c.plan_id);
      if (!r || (c.items === 0 && c.share === 0)) continue;
      cells.push({
        plan_id: c.plan_id,
        phase: pv.phase.id,
        y: r.y,
        pct: c.total ? c.done / c.total : 0,
        total: c.total,
        done: c.done,
        rough: c.rough,
        color: r.color,
        ...box(pv.phase.start_day, pv.phase.end_day),
      });
    }
  }
  for (const tv of viz.tracks) {
    const r = rowOf.get(tv.plan_id);
    if (!r || tv.routine || tv.loose.items === 0) continue;
    cells.push({
      plan_id: tv.plan_id,
      phase: null,
      y: r.y,
      pct: tv.loose.total ? tv.loose.done / tv.loose.total : 0,
      total: tv.loose.total,
      done: tv.loose.done,
      rough: tv.loose.rough,
      color: r.color,
      ...box(start, tv.loose.finish || tv.finish || end),
    });
  }
  // 每周固定的事：从开始到结束，每周排到的第一天画一个点
  const ticks: TimelineLayout['ticks'] = [];
  full.tracks.forEach((t, i) => {
    if (!isRoutine(t.plan)) return;
    const r = rows[i];
    const days = t.plan.week_minutes.map((m, k) => (m > 0 ? k : -1)).filter((k) => k >= 0);
    if (!days.length) return;
    const from = t.plan.start_day || start;
    const to = t.plan.end_day && t.plan.end_day < end ? t.plan.end_day : end;
    const f = parseDay(from);
    if (!f) return;
    // 第一个排到的日子
    let d = from;
    for (let k = 0; k < 7; k++) {
      const wd = ((parseDay(d)!.getDay() + 6) % 7);
      if (days.includes(wd)) break;
      d = addDays(d, 1);
    }
    for (let n = 0; n < 200 && d <= to; n++, d = addDays(d, 7)) {
      ticks.push({ plan_id: t.plan.id, x: x(d) + 1, y: r.y, color: r.color });
    }
  });
  // 节点：名字只写到下一个节点之前（太挤就不写，悬停 / 点开能看全名）
  const sorted = [...full.milestones].sort((a, b) => a.day.localeCompare(b.day));
  const xs = sorted.map((m) => x(m.day));
  const milestones = sorted.map((m, i) => {
    const room = (i + 1 < xs.length ? xs[i + 1] : w - 4) - xs[i] - 16;
    return { id: m.id, x: xs[i], title: m.title, status: m.status, day: m.day, labelW: room >= 40 ? Math.min(140, room) : 0 };
  });
  const buffers = full.goal.buffers.map((b, index) => ({ index, note: b.note, percent: b.percent, ...box(b.start, b.end) }));
  return {
    width: w,
    height: tracksTop + rows.length * ROW_H + 8,
    labelW,
    start,
    end,
    months,
    phaseY,
    phases,
    msY,
    milestones,
    rows,
    rowH: ROW_H,
    tracksTop,
    cells,
    ticks,
    buffers,
    today: today >= start && today <= end ? x(today) : null,
  };
}

// ---------------- 树状图 ----------------

export interface TreeNode {
  key: string;
  /** goal / phase / cell / item */
  kind: 'goal' | 'phase' | 'cell' | 'item';
  title: string;
  sub: string;
  pct: number | null;
  color: string;
  /** 点开时选中的东西 */
  ref: { phase?: number | null; track?: number; item?: number };
  children: TreeNode[];
  done?: boolean;
}

/** 目标 → 阶段 → 线 → 条目 */
export function goalTree(full: GoalFull, viz: GoalViz): TreeNode {
  const color = new Map(full.tracks.map((t, i) => [t.plan.id, laneColor(i)]));
  const tracks = full.tracks.filter((t) => !isRoutine(t.plan));
  const itemNode = (it: PlanItem, track: number): TreeNode => ({
    key: `i${it.id}`,
    kind: 'item',
    title: it.title,
    sub: `${it.minutes}分${it.rough ? ' · 粗' : ''}`,
    pct: it.status === 'done' ? 1 : null,
    color: color.get(track) ?? 'var(--lane-other)',
    ref: { item: it.id, track },
    children: [],
    done: it.status === 'done',
  });
  const cellsFor = (phase: number | null): TreeNode[] =>
    tracks
      .map((t): TreeNode | null => {
        const items = t.items.filter((i) => (i.phase_id ?? null) === phase && i.status !== 'skipped');
        if (!items.length) return null;
        const total = items.reduce((s, i) => s + i.minutes, 0);
        const cell =
          phase === null
            ? viz.tracks.find((x) => x.plan_id === t.plan.id)?.loose
            : viz.phases.find((p) => p.phase.id === phase)?.cells.find((c) => c.plan_id === t.plan.id);
        const done = cell?.done ?? 0;
        return {
          key: `c${phase ?? 'x'}-${t.plan.id}`,
          kind: 'cell',
          title: `${t.plan.emoji ? `${t.plan.emoji} ` : ''}${t.plan.title}`,
          sub: `${items.filter((i) => i.status === 'done').length}/${items.length} 条`,
          pct: total ? done / total : 0,
          color: color.get(t.plan.id) ?? 'var(--lane-other)',
          ref: { phase, track: t.plan.id },
          children: items.map((i) => itemNode(i, t.plan.id)),
        };
      })
      .filter((x): x is TreeNode => !!x);
  const phases: TreeNode[] = viz.phases.map((pv) => ({
    key: `p${pv.phase.id}`,
    kind: 'phase',
    title: pv.phase.title,
    sub: `${shortDay(pv.phase.start_day)}–${shortDay(pv.phase.end_day)}`,
    pct: pv.total ? pv.done / pv.total : 0,
    color: 'var(--her)',
    ref: { phase: pv.phase.id },
    children: cellsFor(pv.phase.id),
  }));
  const loose = cellsFor(null);
  if (loose.length) {
    phases.push({ key: 'px', kind: 'phase', title: '不分阶段', sub: '', pct: null, color: 'var(--her)', ref: { phase: null }, children: loose });
  }
  return {
    key: 'g',
    kind: 'goal',
    title: `${full.goal.emoji ? `${full.goal.emoji} ` : ''}${full.goal.title}`,
    sub: `${shortDay(full.goal.start_day)}–${shortDay(full.goal.end_day)}`,
    pct: viz.total ? viz.done / viz.total : 0,
    color: 'var(--her)',
    ref: {},
    children: phases,
  };
}

export interface TreeBox {
  node: TreeNode;
  x: number;
  y: number;
  depth: number;
  open: boolean;
  hasKids: boolean;
}

export interface TreeLayout {
  width: number;
  height: number;
  boxes: TreeBox[];
  edges: { from: TreeBox; to: TreeBox }[];
  nodeW: number;
  nodeH: number;
}

/** 整齐的横向树：叶子按顺序往下排，父节点在孩子的中间；`open` 里的才展开。节点宽度按 `width` 收放（最窄 120） */
export function treeLayout(root: TreeNode, open: Set<string>, width = 800): TreeLayout {
  const depthOf = (n: TreeNode): number => (open.has(n.key) && n.children.length ? 1 + Math.max(...n.children.map(depthOf)) : 0);
  const visible = depthOf(root);
  const nodeW = Math.max(120, Math.min(170, Math.floor((width - 40) / (visible + 1)) - 30));
  const nodeH = 38;
  const colW = nodeW + 30;
  const rowH = 46;
  const boxes: TreeBox[] = [];
  const edges: { from: TreeBox; to: TreeBox }[] = [];
  let next = 0;
  const place = (n: TreeNode, depth: number): TreeBox => {
    const isOpen = open.has(n.key);
    // 先放孩子（叶子按顺序往下排），自己放在孩子中间
    const placed = (isOpen ? n.children : []).map((k) => place(k, depth + 1));
    const y = placed.length ? (placed[0].y + placed[placed.length - 1].y) / 2 : 8 + next++ * rowH;
    const b: TreeBox = { node: n, x: 8 + depth * colW, y, depth, open: isOpen, hasKids: n.children.length > 0 };
    boxes.push(b);
    placed.forEach((p) => edges.push({ from: b, to: p }));
    return b;
  };
  place(root, 0);
  const depth = Math.max(...boxes.map((b) => b.depth));
  return {
    width: 16 + depth * colW + nodeW,
    height: Math.max(nodeH + 16, next * rowH + 16),
    boxes,
    edges,
    nodeW,
    nodeH,
  };
}

/** 默认展开到「阶段 → 线」（一条条内容点 ＋ 再看） */
export function defaultOpen(root: TreeNode): Set<string> {
  return new Set<string>(['g', ...root.children.map((p) => p.key)]);
}

// ---------------- 流程图 ----------------

export type FlowStep =
  | { kind: 'phase'; phase: GoalPhase; pct: number; state: string; finish: string }
  | { kind: 'milestone'; milestone: GoalMilestone };

/** 按时间排好的阶段和节点（一样的日子阶段在前） */
export function flowSteps(full: GoalFull, viz: GoalViz): FlowStep[] {
  const steps: { day: string; order: number; step: FlowStep }[] = [];
  for (const pv of viz.phases) {
    steps.push({ day: pv.phase.start_day, order: 0, step: { kind: 'phase', phase: pv.phase, pct: pv.total ? pv.done / pv.total : 0, state: pv.state, finish: pv.finish } });
  }
  for (const m of full.milestones) {
    steps.push({ day: m.day, order: 1, step: { kind: 'milestone', milestone: m } });
  }
  steps.sort((a, b) => a.day.localeCompare(b.day) || a.order - b.order);
  return steps.map((s) => s.step);
}

export interface FlowBox {
  step: FlowStep;
  x: number;
  y: number;
  w: number;
  h: number;
  row: number;
}

export interface FlowLayout {
  width: number;
  height: number;
  boxes: FlowBox[];
  /** 相邻两步之间的连线（SVG path） */
  links: string[];
}

/** 蛇形排：一行放得下几个就放几个，下一行反过来走，转弯处竖着连 */
export function flowLayout(steps: FlowStep[], width: number): FlowLayout {
  const w = Math.max(240, Math.round(width));
  const boxW = 180;
  const gap = 34;
  const cols = Math.max(1, Math.floor((w - 16 + gap) / (boxW + gap)));
  const rowH = 128;
  const boxes: FlowBox[] = steps.map((step, i) => {
    const row = Math.floor(i / cols);
    const k = i % cols;
    const col = row % 2 === 0 ? k : cols - 1 - k;
    const h = step.kind === 'phase' ? 92 : 58;
    return { step, x: 8 + col * (boxW + gap), y: 8 + row * rowH + (92 - h) / 2, w: boxW, h, row };
  });
  const links: string[] = [];
  for (let i = 1; i < boxes.length; i++) {
    const a = boxes[i - 1];
    const b = boxes[i];
    const ay = a.y + a.h / 2;
    const by = b.y + b.h / 2;
    if (a.row === b.row) {
      const ltr = b.x > a.x;
      const x1 = ltr ? a.x + a.w : a.x;
      const x2 = ltr ? b.x : b.x + b.w;
      links.push(`M${x1},${ay} L${x2},${by}`);
    } else {
      // 换行：从下边出来，竖着下到下一个的上边（蛇形时两个在同一列）
      const cx = a.x + a.w / 2;
      links.push(`M${cx},${a.y + a.h} L${cx},${b.y}`);
    }
  }
  const rows = boxes.length ? boxes[boxes.length - 1].row + 1 : 1;
  return { width: 16 + cols * boxW + (cols - 1) * gap, height: 16 + rows * rowH - (rowH - 92), boxes, links };
}

// ---------------- 其他 ----------------

/** 「11/9」 */
export function shortDay(day: string): string {
  const d = parseDay(day);
  return d ? `${d.getMonth() + 1}/${d.getDate()}` : '';
}

/** 「11月9日 周一 — 12月31日 周四」 */
export function rangeLabel(a: string, b: string): string {
  return `${dayLabel(a)} — ${dayLabel(b)}`;
}

/** 一条线在一个阶段里的份额（阶段没写 = 线自己的） */
export function shareOf(phase: GoalPhase, plan: Plan): number {
  const keys = Object.keys(phase.shares);
  if (!keys.length) return plan.share ?? 1;
  return phase.shares[String(plan.id)] ?? 0;
}

/** 份额换成百分比 */
export function sharePct(phase: GoalPhase, plans: Plan[]): Map<number, number> {
  const content = plans.filter((p) => !isRoutine(p));
  const ws = content.map((p) => shareOf(phase, p));
  const sum = ws.reduce((a, b) => a + b, 0) || 1;
  return new Map(content.map((p, i) => [p.id, Math.round((ws[i] * 100) / sum)]));
}

export const WEEKDAY_FULL = ['周一', '周二', '周三', '周四', '周五', '周六', '周日'];

export const MS_STATUS: Record<string, string> = { todo: '待确认', passed: '达标', missed: '没达标', skipped: '跳过' };
