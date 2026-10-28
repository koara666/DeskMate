import { afterEach, describe, expect, it } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/preact';
import { UpdateCard } from './UpdateCard';
import type { UpdateApi } from '../lib/ipc';
import type { UpdateProgress } from '../lib/types';

afterEach(cleanup);

const view = { version: '0.3.0', current: '0.2.0', notes: '- 她会说话了', page: 'https://github.com/x', can_install: true };

function fakeApi() {
  const calls: string[] = [];
  let emit: (p: UpdateProgress) => void = () => {};
  const api: UpdateApi = {
    install: async () => {
      calls.push('install');
    },
    skip: async (v) => {
      calls.push(`skip:${v}`);
    },
    onProgress: async (cb) => {
      emit = cb;
      return () => {};
    },
  };
  return { api, calls, emit: (p: UpdateProgress) => emit(p) };
}

describe('更新卡片', () => {
  it('看说明、下载并安装（显示进度）', async () => {
    const f = fakeApi();
    render(<UpdateCard api={f.api} view={view} />);
    fireEvent.click(screen.getByText('看看更新了什么'));
    expect(screen.getByText('- 她会说话了')).toBeTruthy();
    fireEvent.click(screen.getByText('下载并安装'));
    expect(f.calls).toEqual(['install']);
    await act(async () => f.emit({ phase: 'downloading', percent: 40 }));
    expect(screen.getByRole('status').textContent).toContain('40%');
    expect(screen.queryByText('下载并安装'), '下载时不能再点').toBeNull();
    await act(async () => f.emit({ phase: 'failed', percent: 0, message: '校验没通过' }));
    expect(screen.getByRole('status').textContent).toContain('校验没通过');
    expect(screen.getByText('下载并安装'), '失败后可以重试').toBeTruthy();
  });

  it('跳过这个版本', () => {
    const f = fakeApi();
    render(<UpdateCard api={f.api} view={view} />);
    fireEvent.click(screen.getByText('跳过这个版本'));
    expect(f.calls).toEqual(['skip:0.3.0']);
    expect(screen.getByText(/不再提示 0.3.0/)).toBeTruthy();
  });
});
