import { afterEach, describe, expect, it } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/preact';
import { ActivityPage } from './ActivityPage';
import type { ActivityOverview, ActivitySettings } from '../lib/types';
import { testView } from './fixtures';

afterEach(cleanup);

const flush = () =>
  act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });

describe('活动感知页', () => {
  it('改类型规则、把最近用过的程序设为回避、删除活动日志', async () => {
    const saved: ActivitySettings[] = [];
    const deleted: string[] = [];
    const ov: ActivityOverview = {
      current: { category: 'coding', label: 'VS Code', since: 0 },
      recent: [['hades.exe', 'game']],
      days: [['2026-10-28', [{ category: 'game', label: 'Hades', secs: 7200 }]]],
    };
    const api = {
      activityOverview: async () => ov,
      saveActivity: async (a: ActivitySettings) => {
        saved.push(a);
        return a;
      },
      deleteActivity: async (d: string) => {
        deleted.push(d);
        return 1;
      },
    };
    render(<ActivityPage api={api} view={testView()} />);
    await flush();
    expect(screen.getByText('现在：写代码（VS Code）')).toBeTruthy();
    expect(screen.getByText('玩游戏（Hades） 2 小时')).toBeTruthy();
    const game = screen.getByLabelText('玩游戏时') as HTMLSelectElement;
    expect(game.value).toBe('comment');
    fireEvent.change(game, { target: { value: 'quiet' } });
    await flush();
    expect(saved.at(-1)?.rules).toEqual([{ target: 'game', action: 'quiet' }]);
    fireEvent.click(screen.getByRole('button', { name: '设为回避' }));
    await flush();
    expect(saved.at(-1)?.rules).toContainEqual({ target: 'hades.exe', action: 'avoid' });
    fireEvent.click(screen.getByRole('button', { name: '删除这天' }));
    await flush();
    expect(deleted).toEqual(['2026-10-28']);
  });
});
