import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/preact';
import { MemoryPage } from './MemoryPage';
import type { SettingsApi } from '../lib/ipc';
import type { Memory } from '../lib/types';

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const mem: Memory = {
  id: 7,
  kind: 'fact',
  content: '用户叫小林',
  scope: 'global',
  category: 'profile',
  source_kind: 'web',
  source_ref: '',
  trusted: false,
  importance: 5,
  created_at: 1_800_000_000,
  updated_at: 1_800_000_000,
  last_used_at: null,
  use_count: 0,
  status: 'active',
  superseded_by: null,
  day: '',
};

function fakeApi() {
  return {
    memoryOverview: vi.fn(async () => ({
      stats: { facts: 1, summaries: 0, history: 0, conversations: 2 },
      paused: false,
      last_backup: 'deskmate-20261027.db',
      scopes: [{ id: 'global', name: '关于你（所有角色共享）' }],
    })),
    memoryList: vi.fn(async () => ({ items: [mem], total: 1 })),
    memoryUpdate: vi.fn(async () => {}),
    memoryClearAll: vi.fn(async () => {}),
    memorySetPaused: vi.fn(async () => {}),
  } as unknown as SettingsApi & Record<string, ReturnType<typeof vi.fn>>;
}

async function mount(api: SettingsApi) {
  render(<MemoryPage api={api} />);
  await act(async () => {});
}

describe('MemoryPage', () => {
  it('列出记忆，并标出不可信来源', async () => {
    const api = fakeApi();
    await mount(api);
    expect(screen.getByText(/用户叫小林/)).toBeTruthy();
    expect(screen.getByText('不可信来源')).toBeTruthy();
    expect(screen.getByText('来源：网页')).toBeTruthy();
  });

  it('编辑后保存新内容和重要度', async () => {
    const api = fakeApi();
    await mount(api);
    fireEvent.click(screen.getByText('编辑'));
    fireEvent.input(screen.getByLabelText('记忆内容'), { target: { value: '用户叫小林，大家叫他林林' } });
    fireEvent.change(screen.getByLabelText('重要度'), { target: { value: '4' } });
    await act(async () => {
      fireEvent.click(screen.getByText('保存'));
    });
    expect(api.memoryUpdate).toHaveBeenCalledWith(7, '用户叫小林，大家叫他林林', 4);
  });

  it('清空需要输入「清空」并再次确认', async () => {
    const api = fakeApi();
    const confirmMock = vi.fn(() => true);
    vi.stubGlobal('confirm', confirmMock);
    await mount(api);
    const btn = screen.getByText('清空全部') as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
    fireEvent.input(screen.getByLabelText('清空确认'), { target: { value: '清空' } });
    expect(btn.disabled).toBe(false);
    await act(async () => {
      fireEvent.click(btn);
    });
    expect(confirmMock).toHaveBeenCalled();
    expect(api.memoryClearAll).toHaveBeenCalledWith('清空');
  });
});
