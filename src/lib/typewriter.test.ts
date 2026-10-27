import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { Typewriter } from './typewriter';

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe('Typewriter', () => {
  it('逐步显示，显示完后停止计时器', () => {
    const seen: string[] = [];
    const w = new Typewriter((t) => seen.push(t), { tickMs: 10 });
    w.push('一二三');
    expect(seen).toEqual([]);
    vi.advanceTimersByTime(10);
    expect(seen.at(-1)).toBe('一');
    vi.advanceTimersByTime(100);
    expect(seen.at(-1)).toBe('一二三');
    expect(vi.getTimerCount()).toBe(0);
  });

  it('积压多时加速', () => {
    let last = '';
    const w = new Typewriter((t) => (last = t), { tickMs: 10 });
    w.push('x'.repeat(60));
    vi.advanceTimersByTime(10);
    expect(last.length).toBe(10);
  });

  it('flush 立即显示全部，end 在显示完后回调', () => {
    let last = '';
    const done = vi.fn();
    const w = new Typewriter((t) => (last = t), { tickMs: 10 });
    w.push('abcdef');
    w.end(done);
    expect(done).not.toHaveBeenCalled();
    w.flush();
    expect(last).toBe('abcdef');
    expect(done).toHaveBeenCalledTimes(1);
  });

  it('不会把 emoji 劈成两半', () => {
    const seen: string[] = [];
    const w = new Typewriter((t) => seen.push(t), { tickMs: 10 });
    w.push('😀😀');
    vi.advanceTimersByTime(10);
    expect(seen[0]).toBe('😀');
  });
});
