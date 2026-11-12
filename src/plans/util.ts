// 计划窗口的小工具：日期、时长的显示（日期一律是 YYYY-MM-DD 的「计划日」字符串，后端给的「今天」为准）。

const WEEK = ['日', '一', '二', '三', '四', '五', '六'];
/** 周一到周日（和后端 week_minutes 的顺序一致） */
export const WEEK_MON = ['一', '二', '三', '四', '五', '六', '日'];

export function parseDay(s: string): Date | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(s);
  if (!m) return null;
  return new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3]));
}

export function dayStr(d: Date): string {
  const p = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

export function addDays(day: string, n: number): string {
  const d = parseDay(day);
  if (!d) return day;
  d.setDate(d.getDate() + n);
  return dayStr(d);
}

export function daysBetween(a: string, b: string): number {
  const x = parseDay(a);
  const y = parseDay(b);
  if (!x || !y) return 0;
  return Math.round((y.getTime() - x.getTime()) / 86_400_000);
}

/** 「11月9日 周一」 */
export function dayLabel(day: string): string {
  const d = parseDay(day);
  if (!d) return day;
  return `${d.getMonth() + 1}月${d.getDate()}日 周${WEEK[d.getDay()]}`;
}

/** 相对今天：「今天」「明天」「后天」「周三」「11月20日」 */
export function relDay(day: string, today: string): string {
  const n = daysBetween(today, day);
  if (n === 0) return '今天';
  if (n === 1) return '明天';
  if (n === 2) return '后天';
  if (n === -1) return '昨天';
  const d = parseDay(day);
  if (!d) return day;
  if (n > 0 && n < 7) return `周${WEEK[d.getDay()]}`;
  return `${d.getMonth() + 1}月${d.getDate()}日`;
}

/** 90 → 「1 小时 30 分」 */
export function mins(m: number): string {
  if (m <= 0) return '0 分钟';
  const h = Math.floor(m / 60);
  const r = m % 60;
  if (h === 0) return `${r} 分钟`;
  if (r === 0) return `${h} 小时`;
  return `${h} 小时 ${r} 分`;
}

/** 短一点的：「1.5h」「45分」 */
export function minsShort(m: number): string {
  if (m < 60) return `${m}分`;
  const h = m / 60;
  return `${Number.isInteger(h) ? h : h.toFixed(1)}h`;
}

export function clampInt(v: string | number, lo: number, hi: number): number {
  const n = Math.round(Number(v));
  if (!Number.isFinite(n)) return lo;
  return Math.max(lo, Math.min(hi, n));
}

export function emptyPlan(today: string): import('../lib/types').Plan {
  return {
    id: 0,
    title: '',
    goal: '',
    emoji: '📌',
    mode: 'content',
    start_day: today,
    end_day: '',
    week_minutes: [60, 60, 60, 60, 60, 60, 60],
    off_days: [],
    pace: 'even',
    remind_at: '',
    supervise: true,
    strategy: '',
    status: 'active',
    sort: 0,
    created_at: 0,
    updated_at: 0,
  };
}

export function emptyItem(title = ''): import('../lib/types').PlanItem {
  return { id: 0, plan_id: 0, title, detail: '', minutes: 60, status: 'todo', sort: 0, done_at: null };
}

export function emptyTask(day: string): import('../lib/types').PlanTask {
  return {
    id: 0,
    plan_id: null,
    item_id: null,
    day,
    orig_day: '',
    title: '',
    minutes: 0,
    start: '',
    status: 'todo',
    spent: 0,
    finishes: false,
    note: '',
    source: 'user',
    sort: 0,
    reminded: false,
    done_at: null,
    created_at: 0,
  };
}
