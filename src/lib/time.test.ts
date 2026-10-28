import { describe, expect, it } from 'vitest';
import { formatTime } from './time';

// 用本地时间构造，避免受测试机器时区影响
const at = (y: number, mo: number, d: number, h: number, mi: number) => new Date(y, mo - 1, d, h, mi).getTime() / 1000;

describe('formatTime', () => {
  const now = at(2026, 10, 28, 9, 30);
  it('今天只显示时间', () => {
    expect(formatTime(at(2026, 10, 28, 8, 5), now)).toBe('08:05');
  });
  it('昨天（包括跨过午夜不到 24 小时）', () => {
    expect(formatTime(at(2026, 10, 27, 23, 50), now)).toBe('昨天 23:50');
  });
  it('今年更早显示月日', () => {
    expect(formatTime(at(2026, 3, 8, 14, 0), now)).toBe('3月8日 14:00');
  });
  it('往年显示年份', () => {
    expect(formatTime(at(2025, 12, 31, 7, 9), now)).toBe('2025年12月31日 07:09');
  });
});
