import { afterEach, describe, expect, it } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/preact';
import { RemindersPage } from './RemindersPage';
import type { Reminder, ReminderView } from '../lib/types';

afterEach(cleanup);

const base: ReminderView = {
  id: 1,
  content: '倒垃圾',
  trigger: 'at',
  at: 1_800_000_000,
  repeat: { type: 'weekly', days: [5] },
  activity: '',
  line: '',
  important: false,
  status: 'active',
  created_at: 0,
  last_fired_at: null,
  when: '每周五 20:00',
};

function fakeApi(items: ReminderView[]) {
  const calls = { saved: [] as Reminder[], deleted: [] as number[], status: [] as [number, string][] };
  let list = [...items];
  const api = {
    reminderList: async () => list,
    reminderSave: async (r: Reminder) => {
      calls.saved.push(r);
      list = [...list, { ...r, id: 9, when: '明天 9:00' }];
      return { reminder: { ...r, id: 9, when: '明天 9:00' }, line: '' };
    },
    reminderDelete: async (id: number) => {
      calls.deleted.push(id);
      list = list.filter((x) => x.id !== id);
    },
    reminderSetStatus: async (id: number, s: 'active' | 'paused') => {
      calls.status.push([id, s]);
      list = list.map((x) => (x.id === id ? { ...x, status: s } : x));
    },
  };
  return { api, calls };
}

const flush = () =>
  act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });

describe('提醒页', () => {
  it('列出提醒，可以暂停、删除、新建', async () => {
    const f = fakeApi([base, { ...base, id: 2, content: '早就做完的', status: 'done', last_fired_at: 1_700_000_000 }]);
    render(<RemindersPage api={f.api} />);
    await flush();
    expect(screen.getByText('倒垃圾')).toBeTruthy();
    expect(screen.getByText(/每周五 20:00/)).toBeTruthy();
    expect(screen.getByText('已完成（1）')).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: '暂停' }));
    await flush();
    expect(f.calls.status).toEqual([[1, 'paused']]);
    expect(screen.getByRole('button', { name: '恢复' })).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: '新建提醒' }));
    fireEvent.input(screen.getByLabelText('要提醒的事'), { target: { value: '交作业' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await flush();
    expect(f.calls.saved[0].content).toBe('交作业');
    expect(screen.getByText('交作业')).toBeTruthy();

    fireEvent.click(screen.getAllByRole('button', { name: '删除' })[0]);
    await flush();
    expect(f.calls.deleted).toEqual([1]);
  });
});
