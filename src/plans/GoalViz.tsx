// 计划书的三种图：时间线 / 泳道（默认）、树状图、流程图。都是内联 SVG（窗口只允许本地脚本），点节点选中，旁边的面板里看、改。
// 线的颜色按线的顺序固定（--lane-0 … --lane-7，浅色 / 深色各一套），每行都写了线的名字，不只靠颜色认。
import { useEffect, useMemo, useRef, useState } from 'preact/hooks';
import type { GoalFull, GoalViz as Viz } from '../lib/types';
import { MS_STATUS, defaultOpen, flowLayout, flowSteps, goalTree, laneColor, shortDay, timelineLayout, treeLayout, type TreeNode } from './goalUtil';
import { dayLabel, mins, minsShort } from './util';

/** 选中的东西 */
export type Sel =
  | { kind: 'goal' }
  | { kind: 'phase'; id: number }
  | { kind: 'track'; id: number }
  | { kind: 'cell'; phase: number | null; track: number }
  | { kind: 'milestone'; id: number }
  | { kind: 'buffer'; index: number }
  | { kind: 'item'; id: number; track: number };

export type VizKind = 'timeline' | 'tree' | 'flow';

const VIZ_KEY = 'deskmate.goal.viz';

function useWidth(ref: { current: HTMLElement | null }): number {
  const [w, setW] = useState(720);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    setW(el.clientWidth || 720);
    if (typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver((e) => setW(e[0].contentRect.width || 720));
    ro.observe(el);
    return () => ro.disconnect();
  }, [ref]);
  return w;
}

/** 按估计的字宽截断（中日文一个字宽 1em，其余 0.6em） */
export function fit(text: string, px: number, size = 12): string {
  let w = 0;
  let out = '';
  for (const ch of text) {
    const cw = /[⺀-鿿＀-￯　-〿]/.test(ch) ? size : size * 0.6;
    if (w + cw > px) return out.length ? `${out.slice(0, Math.max(0, out.length - 1))}…` : '';
    w += cw;
    out += ch;
  }
  return out;
}

/** 让 SVG 里的东西能用键盘点 */
function press(fn: () => void) {
  return {
    role: 'button' as const,
    tabIndex: 0,
    onClick: (e: MouseEvent) => {
      e.stopPropagation();
      fn();
    },
    onKeyDown: (e: KeyboardEvent) => {
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        fn();
      }
    },
  };
}

function same(a: Sel | null, b: Sel): boolean {
  return !!a && JSON.stringify(a) === JSON.stringify(b);
}

const MS_ICON: Record<string, string> = { todo: '', passed: '✓', missed: '✗', skipped: '–' };

// ---------------- 时间线 / 泳道 ----------------

export function Timeline({ full, viz, today, sel, onSel }: { full: GoalFull; viz: Viz; today: string; sel: Sel | null; onSel: (s: Sel) => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const width = useWidth(ref);
  const L = useMemo(() => timelineLayout(full, viz, width - 2, today), [full, viz, width, today]);
  const names = new Map(full.tracks.map((t) => [t.plan.id, t.plan.title]));
  const phaseNames = new Map(full.phases.map((p) => [p.id, p.title]));
  return (
    <div class="viz-scroll" ref={ref}>
      <svg class="viz timeline" width={L.width} height={L.height} viewBox={`0 0 ${L.width} ${L.height}`} aria-label="计划书时间线">
        <defs>
          <pattern id="tl-hatch" width="6" height="6" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
            <line x1="0" y1="0" x2="0" y2="6" class="hatch-line" />
          </pattern>
        </defs>
        {L.rows.map((r, i) => (
          <rect key={`bg${r.plan_id}`} class={`row-bg ${i % 2 ? 'odd' : ''}`} x={0} y={r.y} width={L.width} height={L.rowH} />
        ))}
        {L.months.map((m) => (
          <g key={m.label + m.x}>
            <line class="v-grid" x1={m.x} x2={m.x} y1={16} y2={L.height} />
            <text class="v-axis" x={m.x + 3} y={11}>
              {m.label}
            </text>
          </g>
        ))}
        {L.buffers.map((b) => (
          <g key={`b${b.index}`} class={`buffer ${same(sel, { kind: 'buffer', index: b.index }) ? 'sel' : ''}`} {...press(() => onSel({ kind: 'buffer', index: b.index }))} aria-label={`缓冲：${b.note || '少排'}`}>
            <rect x={b.x} y={L.msY - 10} width={b.w} height={L.height - L.msY + 10} fill="url(#tl-hatch)" />
            <title>
              {`缓冲 ${shortDay(full.goal.buffers[b.index].start)}–${shortDay(full.goal.buffers[b.index].end)}：${b.percent === 0 ? '不排' : `按平时的 ${b.percent}%`}${b.note ? `（${b.note}）` : ''}`}
            </title>
          </g>
        ))}
        {L.phases.map((p, i) => (
          <g
            key={`p${p.id}`}
            class={`phase-band k${i % 2} ${p.current ? 'current' : ''} ${same(sel, { kind: 'phase', id: p.id }) ? 'sel' : ''}`}
            {...press(() => onSel({ kind: 'phase', id: p.id }))}
            aria-label={`阶段：${p.title}`}
          >
            <rect x={p.x + 1} y={L.phaseY} width={Math.max(2, p.w - 2)} height={26} rx={6} />
            <text x={p.x + 8} y={L.phaseY + 17}>
              {fit(p.title, p.w - 14)}
            </text>
            <title>{`${p.title}${p.current ? '（现在）' : ''}`}</title>
          </g>
        ))}
        {L.cells.map((c) => {
          const s: Sel = { kind: 'cell', phase: c.phase, track: c.plan_id };
          const pct = Math.round(c.pct * 100);
          const label = `${names.get(c.plan_id) ?? ''} · ${c.phase === null ? '不分阶段' : phaseNames.get(c.phase) ?? ''}：做了 ${mins(c.done)} / ${mins(c.total)}（${pct}%）${c.rough ? `，${c.rough} 块还没细化` : ''}`;
          return (
            <g key={`c${c.phase}-${c.plan_id}`} class={`cell ${same(sel, s) ? 'sel' : ''} ${c.total === 0 ? 'empty' : ''}`} {...press(() => onSel(s))} aria-label={label}>
              <rect class="v-hit" x={c.x} y={c.y + 2} width={c.w} height={L.rowH - 4} />
              <rect class="cell-bg" x={c.x + 1} y={c.y + 9} width={Math.max(2, c.w - 2)} height={16} rx={4} fill={c.color} />
              {c.pct > 0 && <rect class="cell-fill" x={c.x + 1} y={c.y + 9} width={Math.max(4, (c.w - 2) * c.pct)} height={16} rx={4} fill={c.color} />}
              {c.w > 44 && c.total > 0 && (
                <text class="cell-text" x={c.x + 6} y={c.y + 21}>
                  {pct}%{c.rough && c.w > 120 ? ' · 待细化' : ''}
                </text>
              )}
              <title>{label}</title>
            </g>
          );
        })}
        {L.ticks.map((t, i) => (
          <circle key={`t${t.plan_id}-${i}`} class="v-tick" cx={t.x} cy={t.y + 17} r={3.5} fill={t.color} />
        ))}
        {L.rows.map((r) => (
          <g key={`l${r.plan_id}`} class={`lane-label ${same(sel, { kind: 'track', id: r.plan_id }) ? 'sel' : ''}`} {...press(() => onSel({ kind: 'track', id: r.plan_id }))} aria-label={`线：${r.title}`}>
            <rect class="label-bg" x={0} y={r.y} width={L.labelW} height={L.rowH} />
            <rect x={8} y={r.y + 12} width={10} height={10} rx={3} fill={r.color} />
            <text x={24} y={r.y + 21}>
              {fit(`${r.title}${r.routine ? ' 🔁' : ''}`, L.labelW - 30 - (r.short ? 14 : 0))}
              {r.short ? ' ⚠' : ''}
            </text>
            <title>{`${r.emoji} ${r.title}${r.routine ? '（每周固定的事）' : ''}${r.short ? '：按现在的时间排不完' : ''}`}</title>
          </g>
        ))}
        {L.milestones.map((m) => (
          <g key={`m${m.id}`} class={`ms ms-${m.status} ${same(sel, { kind: 'milestone', id: m.id }) ? 'sel' : ''}`} {...press(() => onSel({ kind: 'milestone', id: m.id }))} aria-label={`节点：${m.title}（${MS_STATUS[m.status]}）`}>
            <line class="ms-line" x1={m.x} x2={m.x} y1={L.msY + 7} y2={L.height - 4} />
            <rect class="v-hit" x={m.x - 10} y={L.msY - 10} width={20} height={20} />
            <path class="ms-mark" d={`M${m.x},${L.msY - 7} L${m.x + 7},${L.msY} L${m.x},${L.msY + 7} L${m.x - 7},${L.msY} Z`} />
            {MS_ICON[m.status] && (
              <text class="ms-icon" x={m.x} y={L.msY + 4} text-anchor="middle">
                {MS_ICON[m.status]}
              </text>
            )}
            {m.labelW > 0 && (
              <text class="ms-text" x={m.x + 10} y={L.msY + 4}>
                {fit(m.title, m.labelW, 11)}
              </text>
            )}
            <title>{`${dayLabel(m.day)} ${m.title}（${MS_STATUS[m.status]}）`}</title>
          </g>
        ))}
        {L.today !== null && (
          <g class="today" aria-hidden="true">
            <line x1={L.today} x2={L.today} y1={L.phaseY - 4} y2={L.height} />
            <text x={L.today + 3} y={L.phaseY - 6}>
              今天
            </text>
          </g>
        )}
      </svg>
    </div>
  );
}

// ---------------- 树状图 ----------------

function toSel(n: TreeNode): Sel {
  switch (n.kind) {
    case 'goal':
      return { kind: 'goal' };
    case 'phase':
      return n.ref.phase == null ? { kind: 'goal' } : { kind: 'phase', id: n.ref.phase };
    case 'cell':
      return { kind: 'cell', phase: n.ref.phase ?? null, track: n.ref.track ?? 0 };
    default:
      return { kind: 'item', id: n.ref.item ?? 0, track: n.ref.track ?? 0 };
  }
}

export function Tree({ full, viz, sel, onSel }: { full: GoalFull; viz: Viz; sel: Sel | null; onSel: (s: Sel) => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const width = useWidth(ref);
  const root = useMemo(() => goalTree(full, viz), [full, viz]);
  const [open, setOpen] = useState<Set<string>>(() => defaultOpen(root));
  const L = useMemo(() => treeLayout(root, open, width - 24), [root, open, width]);
  const toggle = (k: string) => {
    const s = new Set(open);
    if (s.has(k)) s.delete(k);
    else s.add(k);
    setOpen(s);
  };
  const { nodeW: w, nodeH: h } = L;
  return (
    <div class="viz-scroll" ref={ref}>
      <svg class="viz tree" width={L.width + 24} height={L.height} viewBox={`0 0 ${L.width + 24} ${L.height}`} aria-label="计划书树状图">
        {L.edges.map((e) => (
          <path
            key={`${e.from.node.key}-${e.to.node.key}`}
            class="v-edge"
            d={`M${e.from.x + w},${e.from.y + h / 2} C${e.from.x + w + 18},${e.from.y + h / 2} ${e.to.x - 18},${e.to.y + h / 2} ${e.to.x},${e.to.y + h / 2}`}
          />
        ))}
        {L.boxes.map((b) => {
          const n = b.node;
          const s = toSel(n);
          const pct = n.pct === null ? null : Math.round(n.pct * 100);
          return (
            <g key={n.key} class={`v-tnode k-${n.kind} ${n.done ? 'done' : ''} ${same(sel, s) && n.kind !== 'goal' ? 'sel' : ''}`}>
              <g {...press(() => onSel(s))} aria-label={`${n.title}${pct !== null ? `，${pct}%` : ''}`}>
                <rect class="v-box" x={b.x} y={b.y} width={w} height={h} rx={8} />
                {(n.kind === 'cell' || n.kind === 'item') && <rect x={b.x} y={b.y} width={4} height={h} rx={2} fill={n.color} />}
                <text class="t1" x={b.x + 10} y={b.y + 16}>
                  {fit(`${n.done ? '✓ ' : ''}${n.title}`, w - 20)}
                </text>
                <text class="t2" x={b.x + 10} y={b.y + 30}>
                  {fit(`${n.sub}${pct !== null && n.kind !== 'item' ? `  ${pct}%` : ''}`, w - 20, 10)}
                </text>
                {pct !== null && n.kind !== 'item' && (
                  <>
                    <rect class="v-bar-bg" x={b.x + 10} y={b.y + h - 5} width={w - 20} height={2} rx={1} />
                    <rect class="v-bar" x={b.x + 10} y={b.y + h - 5} width={Math.max(0, (w - 20) * (n.pct ?? 0))} height={2} rx={1} />
                  </>
                )}
                <title>{`${n.title}${n.sub ? ` · ${n.sub}` : ''}${pct !== null ? ` · ${pct}%` : ''}`}</title>
              </g>
              {b.hasKids && (
                <g class="v-toggle" {...press(() => toggle(n.key))} aria-label={b.open ? `收起：${n.title}` : `展开：${n.title}`} aria-expanded={b.open}>
                  <circle cx={b.x + w + 9} cy={b.y + h / 2} r={8} />
                  <text x={b.x + w + 9} y={b.y + h / 2 + 4} text-anchor="middle">
                    {b.open ? '−' : '+'}
                  </text>
                </g>
              )}
            </g>
          );
        })}
      </svg>
    </div>
  );
}

// ---------------- 流程图 ----------------

export function Flow({ full, viz, today, sel, onSel }: { full: GoalFull; viz: Viz; today: string; sel: Sel | null; onSel: (s: Sel) => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const width = useWidth(ref);
  const steps = useMemo(() => flowSteps(full, viz), [full, viz]);
  const L = useMemo(() => flowLayout(steps, width - 2), [steps, width]);
  if (!steps.length) return <p class="muted small">还没有阶段和节点。点「加阶段」「加节点」排一排吧。</p>;
  return (
    <div class="viz-scroll" ref={ref}>
      <svg class="viz flow" width={L.width} height={L.height} viewBox={`0 0 ${L.width} ${L.height}`} aria-label="计划书流程图">
        <defs>
          <marker id="flow-arrow" viewBox="0 0 8 8" refX="7" refY="4" markerWidth="8" markerHeight="8" orient="auto-start-reverse">
            <path d="M0,0 L8,4 L0,8 z" class="arrow-head" />
          </marker>
        </defs>
        {L.links.map((d, i) => (
          <path key={i} class="v-link" d={d} marker-end="url(#flow-arrow)" />
        ))}
        {L.boxes.map((b) => {
          if (b.step.kind === 'phase') {
            const p = b.step.phase;
            const s: Sel = { kind: 'phase', id: p.id };
            const pct = Math.round(b.step.pct * 100);
            return (
              <g key={`p${p.id}`} class={`v-fnode phase ${b.step.state} ${same(sel, s) ? 'sel' : ''}`} {...press(() => onSel(s))} aria-label={`阶段：${p.title}，${pct}%`}>
                <rect class="v-box" x={b.x} y={b.y} width={b.w} height={b.h} rx={10} />
                <text class="t1" x={b.x + 12} y={b.y + 22}>
                  {fit(p.title, b.w - 64)}
                </text>
                {b.step.state === 'current' && (
                  <text class="v-badge" x={b.x + b.w - 12} y={b.y + 22} text-anchor="end">
                    现在
                  </text>
                )}
                <text class="t2" x={b.x + 12} y={b.y + 40}>
                  {`${shortDay(p.start_day)} – ${shortDay(p.end_day)}`}
                </text>
                <text class="t2" x={b.x + 12} y={b.y + 57}>
                  {fit(p.objective || `${p.criteria.length} 条完成标准`, b.w - 24, 10)}
                </text>
                <rect class="v-bar-bg" x={b.x + 12} y={b.y + b.h - 18} width={b.w - 56} height={5} rx={2.5} />
                <rect class="v-bar" x={b.x + 12} y={b.y + b.h - 18} width={Math.max(0, (b.w - 56) * b.step.pct)} height={5} rx={2.5} />
                <text class="t2 pct" x={b.x + b.w - 12} y={b.y + b.h - 13} text-anchor="end">
                  {pct}%
                </text>
                <title>{`${p.title}（${shortDay(p.start_day)}–${shortDay(p.end_day)}）${p.objective ? `：${p.objective}` : ''}`}</title>
              </g>
            );
          }
          const m = b.step.milestone;
          const s: Sel = { kind: 'milestone', id: m.id };
          const soon = m.status === 'todo' && m.day >= today;
          return (
            <g key={`m${m.id}`} class={`v-fnode ms ms-${m.status} ${soon ? 'soon' : ''} ${same(sel, s) ? 'sel' : ''}`} {...press(() => onSel(s))} aria-label={`节点：${m.title}（${MS_STATUS[m.status]}）`}>
              <rect class="v-box" x={b.x} y={b.y} width={b.w} height={b.h} rx={b.h / 2} />
              <path class="ms-mark" d={`M${b.x + 20},${b.y + b.h / 2 - 7} L${b.x + 27},${b.y + b.h / 2} L${b.x + 20},${b.y + b.h / 2 + 7} L${b.x + 13},${b.y + b.h / 2} Z`} />
              <text class="t1" x={b.x + 36} y={b.y + 24}>
                {fit(m.title, b.w - 48)}
              </text>
              <text class="t2" x={b.x + 36} y={b.y + 41}>
                {`${shortDay(m.day)} · ${MS_ICON[m.status] ? `${MS_ICON[m.status]} ` : ''}${MS_STATUS[m.status]}`}
              </text>
              <title>{`${dayLabel(m.day)} ${m.title}${m.criteria ? `：${m.criteria}` : ''}`}</title>
            </g>
          );
        })}
      </svg>
    </div>
  );
}

// ---------------- 切换 + 图例 ----------------

function loadKind(): VizKind {
  try {
    const v = localStorage.getItem(VIZ_KEY);
    if (v === 'tree' || v === 'flow' || v === 'timeline') return v;
  } catch {
    /* 存不了就用默认 */
  }
  return 'timeline';
}

export function VizPanel({ full, viz, today, sel, onSel }: { full: GoalFull; viz: Viz; today: string; sel: Sel | null; onSel: (s: Sel) => void }) {
  const [kind, setKind] = useState<VizKind>(loadKind);
  const pick = (k: VizKind) => {
    setKind(k);
    try {
      localStorage.setItem(VIZ_KEY, k);
    } catch {
      /* 不记也行 */
    }
  };
  return (
    <div class="viz-panel">
      <div class="viz-head">
        <div class="seg left" role="radiogroup" aria-label="怎么看计划书">
          {(
            [
              ['timeline', '时间线'],
              ['tree', '树状图'],
              ['flow', '流程图'],
            ] as [VizKind, string][]
          ).map(([k, label]) => (
            <button type="button" key={k} role="radio" aria-checked={kind === k} class={kind === k ? 'on' : ''} onClick={() => pick(k)}>
              {label}
            </button>
          ))}
        </div>
        <div class="viz-legend small" aria-label="图例">
          {full.tracks.map((t, i) => (
            <span key={t.plan.id} class="lg">
              <i class="sw" style={{ background: laneColor(i) }} />
              {t.plan.title}
            </span>
          ))}
          <span class="lg">
            <i class="sw ms" /> 节点
          </span>
          {full.goal.buffers.length > 0 && (
            <span class="lg">
              <i class="sw hatch" /> 缓冲
            </span>
          )}
        </div>
      </div>
      {kind === 'timeline' ? (
        <Timeline full={full} viz={viz} today={today} sel={sel} onSel={onSel} />
      ) : kind === 'tree' ? (
        <Tree full={full} viz={viz} sel={sel} onSel={onSel} />
      ) : (
        <Flow full={full} viz={viz} today={today} sel={sel} onSel={onSel} />
      )}
      <p class="muted small viz-hint">点图上的阶段、线、节点、缓冲期，在下面看详情、改。{kind === 'timeline' ? '条的长短是时间，填满的部分是做了多少。' : ''}</p>
    </div>
  );
}

/** 往后两周每天排什么（按线上色） */
export function UpcomingDays({ full, viz, today }: { full: GoalFull; viz: Viz; today: string }) {
  const idx = new Map(full.tracks.map((t, i) => [t.plan.id, i]));
  if (!viz.days.length) return <p class="muted small">接下来两周没有排到的内容。</p>;
  return (
    <ul class="plain goal-days">
      {viz.days.slice(0, 14).map(([d, slots]) => (
        <li key={d}>
          <span class="pd-day">{d === today ? '今天' : dayLabel(d).replace(/^\d+月/, '')}</span>
          <span class="pd-items">
            {slots.map(([lane, title, m], i) => (
              <span key={i} class="pd-item">
                <i class="sw" style={{ background: laneColor(idx.get(lane) ?? -1) }} />
                {title}
                <small> {minsShort(m)}</small>
              </span>
            ))}
          </span>
        </li>
      ))}
    </ul>
  );
}
