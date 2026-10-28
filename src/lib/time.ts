// 聊天记录里的时间显示。和后端 memory::conversation::GAP_SECS 一致：隔 30 分钟以上算新的一次聊天。

export const GAP_SECS = 30 * 60;

export const nowSecs = () => Math.floor(Date.now() / 1000);

const pad = (n: number) => String(n).padStart(2, '0');

/** 今天「14:05」；昨天「昨天 14:05」；今年「3月8日 14:05」；更早「2025年3月8日 14:05」 */
export function formatTime(at: number, now: number): string {
  const d = new Date(at * 1000);
  const n = new Date(now * 1000);
  const hm = `${pad(d.getHours())}:${pad(d.getMinutes())}`;
  const startOfDay = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const days = Math.round((startOfDay(n) - startOfDay(d)) / 86_400_000);
  if (days === 0) return hm;
  if (days === 1) return `昨天 ${hm}`;
  const md = `${d.getMonth() + 1}月${d.getDate()}日 ${hm}`;
  return d.getFullYear() === n.getFullYear() ? md : `${d.getFullYear()}年${md}`;
}
