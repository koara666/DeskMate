import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/preact';
import { AgendaSoon, CalendarPage, leftText, monthGrid, monthOf, remindDays, remindHint, shiftMonth } from './CalendarPage';
import { toView } from './PlansApp';
import type { AgendaEntry } from '../lib/types';

afterEach(cleanup);

/** 保存 → 重新取 → 画出来：重新取是在 act 结束、副作用跑完以后才发出的，所以要等两轮 */
const flush = async () => {
  for (let round = 0; round < 2; round++) {
    await act(async () => {
      for (let i = 0; i < 3; i++) await new Promise((r) => setTimeout(r, 0));
    });
  }
};

const TODAY = '2026-11-12'; // 周四

function entry(id: number, day: string, title: string, over: Partial<AgendaEntry> = {}): AgendaEntry {
  return { id, day, title, note: '', done: false, done_at: null, created_at: 0, updated_at: 0, ...over };
}

/** 假的后端：存在内存里，记下每次调用 */
function fakeApi(initial: AgendaEntry[] = []) {
  let rows = [...initial];
  let next = 100;
  const api = {
    agendaRange: vi.fn(async (from: string, to: string) => rows.filter((e) => e.day >= from && e.day <= to)),
    agendaSave: vi.fn(async (e: AgendaEntry) => {
      if (e.id > 0) {
        rows = rows.map((r) => (r.id === e.id ? { ...e, updated_at: r.updated_at + 1 } : r));
        return e;
      }
      const saved = { ...e, id: next++ };
      rows.push(saved);
      return saved;
    }),
    agendaDelete: vi.fn(async (id: number) => {
      rows = rows.filter((r) => r.id !== id);
    }),
    agendaDone: vi.fn(async (id: number, done: boolean) => {
      rows = rows.map((r) => (r.id === id ? { ...r, done, updated_at: r.updated_at + 1 } : r));
    }),
  };
  return api;
}

describe('日历的小工具', () => {
  it('一个月的格子从 1 号那周的周一开始，6 周 42 天', () => {
    const g = monthGrid('2026-11-01'); // 11 月 1 日是周日
    expect(g).toHaveLength(42);
    expect(g[0]).toBe('2026-10-26');
    expect(g[6]).toBe('2026-11-01');
    expect(g[41]).toBe('2026-12-06');
    expect(monthGrid('2026-06-01')[0]).toBe('2026-06-01'); // 6 月 1 日正好是周一
  });

  it('翻月跨年', () => {
    expect(shiftMonth('2026-12-01', 1)).toBe('2027-01-01');
    expect(shiftMonth('2026-01-01', -1)).toBe('2025-12-01');
    expect(monthOf('2026-11-25')).toBe('2026-11-01');
  });

  it('还剩几天的说法', () => {
    expect(leftText(TODAY, TODAY)).toBe('今天');
    expect(leftText('2026-11-13', TODAY)).toBe('明天');
    expect(leftText('2026-11-25', TODAY)).toBe('还有 13 天');
    expect(leftText('2026-11-09', TODAY)).toBe('3 天前');
  });

  it('提醒的日子：7、3、1 天前和当天；已经在 7 天内的从下一档开始（和后端一致）', () => {
    expect(remindDays('2026-11-25', TODAY)).toEqual(['2026-11-18', '2026-11-22', '2026-11-24', '2026-11-25']);
    expect(remindDays('2026-11-17', TODAY)).toEqual(['2026-11-14', '2026-11-16', '2026-11-17']);
    expect(remindDays('2026-11-13', TODAY)).toEqual(['2026-11-13']);
    expect(remindDays(TODAY, TODAY)).toEqual([]);
    expect(remindDays('2026-11-01', TODAY)).toEqual([]);
    expect(remindHint('2026-11-25', TODAY, '千秋')).toBe('千秋会在11月18日、11月22日、11月24日和当天各提醒你一次（那天第一次打开桌宠时）。');
    expect(remindHint('2026-11-13', TODAY, '千秋')).toBe('千秋会在当天各提醒你一次（那天第一次打开桌宠时）。');
    expect(remindHint('2026-11-01', TODAY, '千秋')).toContain('不会提醒');
  });

  it('计划窗口的页面名能跳到日历的某一天', () => {
    expect(toView('calendar')).toEqual({ kind: 'calendar' });
    expect(toView('calendar:2026-11-25')).toEqual({ kind: 'calendar', day: '2026-11-25' });
    expect(toView('calendar:11.25')).toBeNull();
  });
});

describe('日历页', () => {
  it('按月显示，格子里有记下的事；点某一天看那天的、能记新的', async () => {
    const api = fakeApi([entry(1, '2026-11-25', '数据结构作业截止'), entry(2, '2026-12-01', '线代课停一次')]);
    render(<CalendarPage api={api} today={TODAY} name="千秋" tick={0} onError={() => {}} />);
    await flush();
    expect(screen.getByText('2026年11月')).toBeTruthy();
    expect(document.querySelectorAll('.agenda-cell')).toHaveLength(42);
    expect(api.agendaRange).toHaveBeenCalledWith('2026-10-26', '2026-12-06');
    // 11 月的格子里就能看到 12 月 1 日那条（下个月头几天也在格子里）
    expect(screen.getByText('数据结构作业截止')).toBeTruthy();
    expect(screen.getByText('线代课停一次')).toBeTruthy();
    // 今天选中、有描边
    const today = document.querySelector(`[data-day="${TODAY}"]`)!;
    expect(today.className).toContain('today');
    expect(today.getAttribute('aria-pressed')).toBe('true');
    // 点 11 月 25 日：下面是那天的
    fireEvent.click(document.querySelector('[data-day="2026-11-25"]')!);
    await flush();
    expect(screen.getByRole('heading', { level: 2, name: /11月25日 周三/ })).toBeTruthy();
    expect(screen.getByText('（还有 13 天）')).toBeTruthy();
    expect(screen.getByText(/千秋会在11月18日、11月22日、11月24日和当天/)).toBeTruthy();
    // 这天再记一条
    fireEvent.input(screen.getByLabelText('这天有什么事'), { target: { value: '  交实验报告  ' } });
    fireEvent.input(screen.getByLabelText('备注（可以不写）'), { target: { value: '要打印' } });
    fireEvent.click(screen.getByRole('button', { name: '记下' }));
    await flush();
    expect(api.agendaSave).toHaveBeenCalledWith(expect.objectContaining({ id: 0, day: '2026-11-25', title: '交实验报告', note: '要打印' }));
    expect(screen.getAllByText('交实验报告').length).toBeGreaterThan(0);
    expect((screen.getByLabelText('这天有什么事') as HTMLInputElement).value).toBe('');
  });

  it('搞定了 / 改一改 / 删掉', async () => {
    const api = fakeApi([entry(1, TODAY, '体育课改到下午')]);
    render(<CalendarPage api={api} today={TODAY} name="千秋" tick={0} onError={() => {}} />);
    await flush();
    fireEvent.click(screen.getByRole('checkbox', { name: '搞定了：体育课改到下午' }));
    await flush();
    expect(api.agendaDone).toHaveBeenCalledWith(1, true);
    expect(screen.getByRole('checkbox', { name: '改回没搞定：体育课改到下午' }).getAttribute('aria-checked')).toBe('true');
    fireEvent.click(screen.getByRole('button', { name: '改一改：体育课改到下午' }));
    fireEvent.input(screen.getByLabelText('改成什么'), { target: { value: '体育课改到周五' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await flush();
    expect(api.agendaSave).toHaveBeenCalledWith(expect.objectContaining({ id: 1, title: '体育课改到周五' }));
    expect(screen.getAllByText('体育课改到周五').length).toBeGreaterThan(0);
    fireEvent.click(screen.getByRole('button', { name: '删掉：体育课改到周五' }));
    await flush();
    expect(api.agendaDelete).toHaveBeenCalledWith(1);
    expect(screen.getByText('这天还没记东西。')).toBeTruthy();
  });

  it('翻到下个月重新取；带着某一天打开时先选中那天', async () => {
    const api = fakeApi();
    render(<CalendarPage api={api} today={TODAY} name="她" tick={0} initialDay="2027-01-15" onError={() => {}} />);
    await flush();
    expect(screen.getByText('2027年1月')).toBeTruthy();
    expect(document.querySelector('[data-day="2027-01-15"]')!.getAttribute('aria-pressed')).toBe('true');
    fireEvent.click(screen.getByRole('button', { name: '下个月' }));
    await flush();
    expect(screen.getByText('2027年2月')).toBeTruthy();
    // 2 月 1 日正好是周一：格子从它开始
    expect(api.agendaRange).toHaveBeenLastCalledWith('2027-02-01', '2027-03-14');
    fireEvent.click(screen.getByRole('button', { name: '回到今天' }));
    await flush();
    expect(screen.getByText('2026年11月')).toBeTruthy();
  });

  it('过去的日子只记不提醒', async () => {
    const api = fakeApi();
    render(<CalendarPage api={api} today={TODAY} name="她" tick={0} onError={() => {}} />);
    await flush();
    fireEvent.click(document.querySelector('[data-day="2026-11-02"]')!);
    await flush();
    expect(screen.getByText('过去的日子只是记下来，不会提醒。')).toBeTruthy();
  });
});

describe('「今天」页上的日历小卡片', () => {
  it('只列接下来 7 天、没搞定的；点了打开那一天；没有就不显示', async () => {
    const api = fakeApi([
      entry(1, '2026-11-13', '交实验报告'),
      entry(2, '2026-11-16', '线代课停一次'),
      entry(3, '2026-11-14', '已经交了', { done: true }),
      entry(4, '2026-11-30', '考试'),
    ]);
    const onOpen = vi.fn();
    render(<AgendaSoon api={api} today={TODAY} tick={0} onOpen={onOpen} onError={() => {}} />);
    await flush();
    expect(api.agendaRange).toHaveBeenCalledWith(TODAY, '2026-11-19');
    expect(screen.getByText('交实验报告')).toBeTruthy();
    expect(screen.getByText('线代课停一次')).toBeTruthy();
    expect(screen.queryByText('已经交了')).toBeNull();
    expect(screen.queryByText('考试')).toBeNull();
    expect(screen.getByText('还有 4 天')).toBeTruthy();
    fireEvent.click(screen.getByText('线代课停一次'));
    expect(onOpen).toHaveBeenCalledWith('2026-11-16');
    cleanup();
    render(<AgendaSoon api={fakeApi()} today={TODAY} tick={0} onOpen={onOpen} onError={() => {}} />);
    await flush();
    expect(document.querySelector('.agenda-soon')).toBeNull();
  });
});
