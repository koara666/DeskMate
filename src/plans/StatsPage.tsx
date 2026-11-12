// 「统计」：最近 16 周每天做了多少（热力图）、最近 8 周每周按计划分色的堆叠柱、几个数字、每个计划的近况、上周回顾。
// 数据来自后端 plan_stats（只统计到今天为止）。图都是自己画的格子 / 内联 SVG，没有第三方库；
// 悬停和键盘聚焦显示同样的详情，柱状图另有表格版，所以看数字不用靠颜色。
import { useEffect, useLayoutEffect, useRef, useState } from 'preact/hooks';
import type { PlansApi } from '../lib/ipc';
import type { PlanStats } from '../lib/types';
import { laneColor } from './goalUtil';
import { WEEK_MON, addDays, dayLabel, mins, minsShort, parseDay } from './util';

type Api = Pick<PlansApi, 'stats'>;

/** 悬停 / 聚焦时浮出来的详情（位置是相对图所在盒子的像素） */
interface Tip {
  x: number;
  y: number;
  head: string;
  /** 正文一行（热力图） */
  text?: string;
  rows?: { color: string; label: string; value: string }[];
}

/** 热力图的颜色档：0 = 没做，1–4 按这天做完的分钟 */
export function heatLevel(m: number): 0 | 1 | 2 | 3 | 4 {
  if (m <= 0) return 0;
  if (m < 30) return 1;
  if (m < 60) return 2;
  if (m < 120) return 3;
  return 4;
}

/** 「11月3日 周一 · 1 小时 20 分 · 做完 3/4 项」 */
export function heatText(h: PlanStats['heat'][number]): string {
  const head = dayLabel(h.day);
  return h.total === 0 ? `${head} · 没有安排` : `${head} · ${mins(h.minutes)} · 做完 ${h.done}/${h.total} 项`;
}

/** 柱状图的纵轴：刻度取整小时，最多 4 段 */
export function niceAxis(maxMinutes: number): { max: number; ticks: number[] } {
  const hours = Math.max(maxMinutes, 1) / 60;
  const step = [0.5, 1, 2, 5, 10, 20, 50, 100, 200, 500].find((s) => Math.ceil(hours / s) <= 4) ?? 1000;
  const n = Math.max(1, Math.ceil(hours / step));
  return { max: n * step * 60, ticks: Array.from({ length: n + 1 }, (_, k) => k * step * 60) };
}

/** 每个颜色对应的真颜色：计划按在图里的顺序取线的 8 种颜色（顺序是固定的，不会因为谁做得多就换色），其他计划用深灰，随手的事用中性灰 */
export function seriesColors(series: PlanStats['series']): Map<string, string> {
  const m = new Map<string, string>();
  let i = 0;
  for (const s of series) {
    m.set(s.key, s.key === 'loose' ? 'var(--lane-other)' : s.key === 'other' ? 'var(--stat-rest)' : laneColor(i++));
  }
  return m;
}

const md = (day: string) => {
  const d = parseDay(day);
  return d ? `${d.getMonth() + 1}/${d.getDate()}` : day;
};
const mdLong = (day: string) => {
  const d = parseDay(day);
  return d ? `${d.getMonth() + 1}月${d.getDate()}日` : day;
};

/** 浮出来的详情：值在前，名字在后 */
function TipBox({ tip }: { tip: Tip | null }) {
  if (!tip) return null;
  return (
    <div class="stat-tip" role="tooltip" style={{ left: `${tip.x}px`, top: `${tip.y}px` }}>
      {tip.text ? (
        tip.text
      ) : (
        <>
          <div class="tip-head">{tip.head}</div>
          {tip.rows?.map((r) => (
            <div class="tip-row" key={r.label}>
              <i class="tip-key" style={{ background: r.color }} />
              <b>{r.value}</b>
              <span class="muted">{r.label}</span>
            </div>
          ))}
        </>
      )}
    </div>
  );
}

/** 把详情摆在某个元素上方，左右不超出盒子 */
function placeTip(box: HTMLElement | null, el: Element): { x: number; y: number } | null {
  if (!box) return null;
  const a = el.getBoundingClientRect();
  const b = box.getBoundingClientRect();
  const half = 150;
  const x = a.left - b.left + a.width / 2;
  return { x: Math.max(half, Math.min(Math.max(half, b.width - half), x)), y: a.top - b.top };
}

function Heat({ stats }: { stats: PlanStats }) {
  const box = useRef<HTMLDivElement>(null);
  const [tip, setTip] = useState<Tip | null>(null);
  const len = stats.heat.length;
  const [cursor, setCursor] = useState(Math.max(0, len - 1));
  const cols = Math.max(1, Math.ceil(len / 7));
  const month = (c: number) => parseDay(addDays(stats.heat_start, 7 * c))?.getMonth() ?? 0;
  const labels: { col: number; text: string }[] = [];
  for (let c = 0; c < cols; c++) {
    if (c === 0 || month(c) !== month(c - 1)) labels.push({ col: c, text: `${month(c) + 1}月` });
  }
  // 第一列的月份只占一两格，后面紧跟着下一个月时不写，免得挤在一起
  if (labels.length > 1 && labels[1].col < 3) labels.shift();
  const total = stats.heat.reduce((s, h) => s + h.minutes, 0);

  const show = (el: HTMLElement, h: PlanStats['heat'][number]) => {
    const p = placeTip(box.current, el);
    if (p) setTip({ ...p, head: dayLabel(h.day), text: heatText(h) });
  };
  // 方向键在格子间移动：左右跳一周，上下跳一天
  const onKey = (e: KeyboardEvent) => {
    const i = Number((e.target as HTMLElement).dataset.i);
    if (Number.isNaN(i)) return;
    const step: Record<string, number> = { ArrowRight: 7, ArrowLeft: -7, ArrowDown: 1, ArrowUp: -1 };
    let next: number;
    if (e.key === 'Home') next = 0;
    else if (e.key === 'End') next = len - 1;
    else if (e.key in step) next = i + step[e.key];
    else return;
    next = Math.max(0, Math.min(len - 1, next));
    e.preventDefault();
    setCursor(next);
    box.current?.querySelector<HTMLElement>(`[data-i="${next}"]`)?.focus();
  };

  return (
    <div class="stat-chart" ref={box}>
      <div class="stat-heat" role="group" aria-label="最近 16 周每天做完的时长，一天一格" onKeyDown={onKey}>
        {labels.map((l) => (
          <span key={l.col} class="heat-month" style={{ gridColumn: `${l.col + 2} / span 3`, gridRow: '1' }}>
            {l.text}
          </span>
        ))}
        {WEEK_MON.map((w, r) => (
          <span key={w} class="heat-wd" style={{ gridColumn: '1', gridRow: String(r + 2) }}>
            {w}
          </span>
        ))}
        {stats.heat.map((h, i) => (
          <button
            type="button"
            key={h.day}
            class={`heat-cell lv-${heatLevel(h.minutes)}${h.day === stats.today ? ' today' : ''}`}
            style={{ gridColumn: String(Math.floor(i / 7) + 2), gridRow: String((i % 7) + 2) }}
            data-i={i}
            tabIndex={i === cursor ? 0 : -1}
            aria-label={heatText(h)}
            onMouseEnter={(e) => show(e.currentTarget, h)}
            onMouseLeave={() => setTip(null)}
            onFocus={(e) => {
              setCursor(i);
              show(e.currentTarget, h);
            }}
            onBlur={() => setTip(null)}
          />
        ))}
      </div>
      <TipBox tip={tip} />
      <div class="heat-legend muted small">
        少
        {[0, 1, 2, 3, 4].map((l) => (
          <i key={l} class={`heat-sw lv-${l}`} />
        ))}
        多
      </div>
      <p class="muted small heat-legend-note">每格一天，颜色按这天做完的时长分五档（30 分钟、1 小时、2 小时）；描边的是今天。</p>
      {total === 0 && <p class="muted small">这 16 周还没有做完的事。</p>}
    </div>
  );
}

const BAR_H = 220;
const PAD = { top: 26, right: 6, bottom: 26, left: 36 };
const BAR_MAX = 24;
const GAP = 2;

function Bars({ stats }: { stats: PlanStats }) {
  const box = useRef<HTMLDivElement>(null);
  const svg = useRef<SVGSVGElement>(null);
  const [width, setWidth] = useState(560);
  const [tip, setTip] = useState<Tip | null>(null);
  // 按盒子的真实宽度排版，文字不会被拉伸；盒子变窄时跟着变
  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    const measure = () => {
      const w = el.clientWidth;
      if (w > 0) setWidth(Math.max(280, w));
    };
    measure();
    if (typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const colors = seriesColors(stats.series);
  const names = new Map(stats.series.map((s) => [s.key, s.label]));
  const n = stats.weeks.length;
  const axis = niceAxis(Math.max(0, ...stats.weeks.map((w) => w.minutes)));
  const plotH = BAR_H - PAD.top - PAD.bottom;
  const plotW = width - PAD.left - PAD.right;
  const slot = plotW / Math.max(1, n);
  const bw = Math.min(BAR_MAX, slot * 0.6);
  const base = PAD.top + plotH;
  const scale = plotH / axis.max;
  const hourLabel = (m: number) => (m === 0 ? '0' : `${m / 60}h`);

  const weekHead = (i: number) => {
    const w = stats.weeks[i];
    return i === n - 1 ? `本周（${mdLong(w.start)} 起，到今天）` : `${mdLong(w.start)} – ${mdLong(addDays(w.start, 6))}`;
  };
  const weekText = (i: number) => {
    const w = stats.weeks[i];
    return `${weekHead(i)}：${w.minutes > 0 ? `共 ${mins(w.minutes)}，${w.parts.map((p) => `${names.get(p.key) ?? p.key} ${mins(p.minutes)}`).join('，')}` : '没有做完的事'}`;
  };
  // top：这根柱子顶端在图里的纵坐标；详情摆在它（和它上方写的总时长）的上面
  const showWeek = (i: number, cx: number, top: number) => {
    const w = stats.weeks[i];
    const half = 150;
    const svgTop = svg.current && box.current ? svg.current.getBoundingClientRect().top - box.current.getBoundingClientRect().top : 0;
    setTip({
      x: Math.max(half, Math.min(Math.max(half, width - half), cx)),
      y: svgTop + top - 20,
      head: `${weekHead(i)}${w.minutes > 0 ? ` · 共 ${mins(w.minutes)}` : ''}`,
      rows: w.parts.length
        ? [...w.parts].reverse().map((p) => ({ color: colors.get(p.key) ?? 'var(--lane-other)', label: names.get(p.key) ?? p.key, value: mins(p.minutes) }))
        : [{ color: 'transparent', label: '没有做完的事', value: '0 分钟' }],
    });
  };

  return (
    <div class="stat-chart" ref={box}>
      <ul class="plain stat-legend" aria-label="图例">
        {stats.series.map((s) => (
          <li key={s.key}>
            <i class="sw" style={{ background: colors.get(s.key) }} />
            {s.label}
          </li>
        ))}
      </ul>
      <svg class="stat-bars" ref={svg} width="100%" height={BAR_H} viewBox={`0 0 ${width} ${BAR_H}`} role="group" aria-label="最近 8 周每周做完的时长，按计划分色">
        {axis.ticks.map((t) => (
          <g key={t}>
            <line class="axis-line" x1={PAD.left} x2={width - PAD.right} y1={base - t * scale} y2={base - t * scale} />
            <text class="axis-text" x={PAD.left - 6} y={base - t * scale + 4} text-anchor="end">
              {hourLabel(t)}
            </text>
          </g>
        ))}
        {stats.weeks.map((w, i) => {
          const cx = PAD.left + slot * (i + 0.5);
          const x = cx - bw / 2;
          let acc = 0;
          const top = base - w.minutes * scale;
          return (
            <g
              key={w.start}
              class="bar-col"
              data-week={w.start}
              tabindex={0}
              role="img"
              aria-label={weekText(i)}
              onMouseEnter={() => showWeek(i, cx, w.minutes > 0 ? top : base)}
              onMouseLeave={() => setTip(null)}
              onFocus={() => showWeek(i, cx, w.minutes > 0 ? top : base)}
              onBlur={() => setTip(null)}
            >
              <rect class="bar-hit" x={cx - slot / 2} y={0} width={slot} height={base + 1} />
              {w.parts.map((p, j) => {
                const y1 = base - acc * scale;
                acc += p.minutes;
                const isTop = j === w.parts.length - 1;
                const h = Math.max(p.minutes * scale - (isTop ? 0 : GAP), 1.5);
                const y = y1 - h;
                const fill = colors.get(p.key) ?? 'var(--lane-other)';
                if (!isTop) return <rect key={p.key} class="bar-seg" x={x} y={y} width={bw} height={h} style={{ fill }} />;
                const r = Math.min(4, h / 2, bw / 2);
                return (
                  <path
                    key={p.key}
                    class="bar-seg"
                    style={{ fill }}
                    d={`M${x},${y + h} L${x},${y + r} Q${x},${y} ${x + r},${y} L${x + bw - r},${y} Q${x + bw},${y} ${x + bw},${y + r} L${x + bw},${y + h} Z`}
                  />
                );
              })}
              <text class={`bar-total${w.minutes === 0 ? ' zero' : ''}`} x={cx} y={(w.minutes === 0 ? base : top) - 6} text-anchor="middle">
                {w.minutes === 0 ? '0' : minsShort(w.minutes)}
              </text>
              <text class="axis-text" x={cx} y={BAR_H - 8} text-anchor="middle">
                {i === n - 1 ? '本周' : md(w.start)}
              </text>
            </g>
          );
        })}
      </svg>
      <TipBox tip={tip} />
      <details class="stat-table">
        <summary>用表格看</summary>
        <div class="stat-table-scroll">
          <table>
            <thead>
              <tr>
                <th scope="col">从周一</th>
                {stats.series.map((s) => (
                  <th scope="col" key={s.key}>
                    {s.label}
                  </th>
                ))}
                <th scope="col">合计</th>
              </tr>
            </thead>
            <tbody>
              {stats.weeks.map((w) => (
                <tr key={w.start}>
                  <th scope="row">{mdLong(w.start)}</th>
                  {stats.series.map((s) => {
                    const m = w.parts.find((p) => p.key === s.key)?.minutes ?? 0;
                    return <td key={s.key}>{m > 0 ? mins(m) : '–'}</td>;
                  })}
                  <td>{w.minutes > 0 ? mins(w.minutes) : '–'}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </details>
    </div>
  );
}

const WEEKS_AGO = ['3 周前', '2 周前', '上周', '本周'];

/** 每个计划一行：名字、近 4 周的小条、总共做了多少 */
function PlanRows({
  stats,
  onOpenPlan,
  onOpenGoal,
}: {
  stats: PlanStats;
  onOpenPlan: (id: number) => void;
  onOpenGoal: (id: number) => void;
}) {
  const colors = seriesColors(stats.series);
  // 小条共用一把尺：同样高就是同样多
  const top = Math.max(1, ...stats.plans.flatMap((p) => p.weeks));
  if (stats.plans.length === 0) return <p class="muted small">还没有进行中的计划。</p>;
  return (
    <ul class="plain stat-plans">
      {stats.plans.map((p) => {
        const color = colors.get(`p${p.plan_id}`) ?? 'var(--lane-other)';
        const sum = p.weeks.reduce((s, m) => s + m, 0);
        return (
          <li key={p.plan_id} class={`stat-plan${p.status === 'paused' ? ' paused' : ''}`}>
            <div class="stat-plan-title">
              <span class="stat-plan-id">
                <i class="sw" style={{ background: color }} />
                <button
                  type="button"
                  class="stat-plan-name"
                  title={p.goal_id ? `打开计划书「${p.goal_title ?? ''}」` : '打开这个计划'}
                  onClick={() => (p.goal_id ? onOpenGoal(p.goal_id) : onOpenPlan(p.plan_id))}
                >
                  {p.emoji} {p.title}
                </button>
              </span>
              {p.goal_title && <span class="tag">计划书「{p.goal_title}」</span>}
              {p.status === 'paused' && <span class="tag">暂停中</span>}
            </div>
            <div class="stat-plan-recent">
              <svg class="stat-mini" width="52" height="26" viewBox="0 0 52 26" role="img" aria-label={`近 4 周每周：${p.weeks.map((m, i) => `${WEEKS_AGO[i]} ${mins(m)}`).join('，')}`}>
                {p.weeks.map((m, i) => {
                  const h = m > 0 ? Math.max(2, (m / top) * 22) : 0;
                  return (
                    <g key={i}>
                      <title>{`${WEEKS_AGO[i]} ${mins(m)}`}</title>
                      {m > 0 ? <rect class="stat-mini-bar" x={i * 14} y={24 - h} width="10" height={h} rx="2" style={{ fill: color }} /> : <rect class="stat-mini-zero" x={i * 14} y="22" width="10" height="2" rx="1" />}
                    </g>
                  );
                })}
              </svg>
              <span class="muted small">近 4 周 {mins(sum)}</span>
            </div>
            <div class="stat-plan-total muted small">
              共做了 {mins(p.total_minutes)} · 做完 {p.done_items} 项
            </div>
          </li>
        );
      })}
    </ul>
  );
}

export function StatsPage({
  api,
  tick,
  onOpenPlan,
  onOpenGoal,
  onError,
}: {
  api: Api;
  /** 计划有变化时加一，重新读 */
  tick: number;
  onOpenPlan: (id: number) => void;
  onOpenGoal: (id: number) => void;
  onError: (e: unknown) => void;
}) {
  const [stats, setStats] = useState<PlanStats | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    let alive = true;
    setBusy(true);
    api
      .stats()
      .then((s) => alive && setStats(s))
      .catch((e) => alive && onError(e))
      .finally(() => alive && setBusy(false));
    return () => {
      alive = false;
    };
  }, [api, tick]);

  if (!stats) return <div class="muted">加载中…</div>;
  const n = stats.numbers;
  const empty = stats.heat.every((h) => h.total === 0);
  return (
    <section class={`stats${busy ? ' loading' : ''}`}>
      <header class="page-head">
        <div>
          <h1>统计</h1>
          <div class="muted">{empty ? '到今天为止' : '到今天为止，最近 16 周'}</div>
        </div>
      </header>

      {empty ? (
        <div class="card stat-empty">
          <p>这里还没有记录。</p>
          <p class="muted small">做完一件事，这里就会长出你的足迹：每天做了多少、每周花在哪个计划上、连续做了几天。</p>
        </div>
      ) : (
        <>
          <div class="stat-cards">
            <div class="stat-card">
              <div class="stat-label">本周</div>
              <div class="stat-value">{mins(n.week_minutes)}</div>
              <div class="stat-sub muted small">周一到今天</div>
            </div>
            <div class="stat-card">
              <div class="stat-label">上周</div>
              <div class="stat-value">{mins(n.last_week_minutes)}</div>
              <div class="stat-sub muted small">周一到周日</div>
            </div>
            <div class="stat-card">
              <div class="stat-label">完成率</div>
              <div class="stat-value">{n.rate === null ? '—' : `${n.rate}%`}</div>
              <div class="stat-sub muted small">{n.rate === null ? '过去 28 天还没有安排' : `过去 28 天，做完 ${n.rate_done}/${n.rate_total} 项`}</div>
            </div>
            <div class="stat-card">
              <div class="stat-label">连续</div>
              <div class="stat-value">
                {n.streak > 0 ? '🔥 ' : ''}
                {n.streak} 天
              </div>
              <div class="stat-sub muted small">最长连续 {n.best_streak} 天</div>
            </div>
          </div>

          <div class="stat-review">
            <b>上周回顾</b>
            <span>{stats.review}</span>
          </div>

          <div class="card">
            <h2>每天做了多少</h2>
            <Heat stats={stats} />
          </div>

          <div class="card">
            <h2>每周做了多少</h2>
            {stats.weeks.every((w) => w.minutes === 0) ? <p class="muted small">这 8 周还没有做完的事，做完一件就会出现在这里。</p> : <Bars stats={stats} />}
          </div>

          <div class="card">
            <h2>每个计划</h2>
            <PlanRows stats={stats} onOpenPlan={onOpenPlan} onOpenGoal={onOpenGoal} />
          </div>
        </>
      )}
    </section>
  );
}
