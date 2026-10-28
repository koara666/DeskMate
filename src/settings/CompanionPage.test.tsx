import { afterEach, describe, expect, it } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/preact';
import { CompanionPage } from './CompanionPage';
import { AdvancedPage } from './AdvancedPage';
import type { SettingsApi } from '../lib/ipc';
import type { AdvancedSettings, CompanionSettings } from '../lib/types';
import { testView } from './fixtures';

afterEach(cleanup);

function fakeApi() {
  const saved = { companion: [] as CompanionSettings[], advanced: [] as AdvancedSettings[] };
  const api = {
    saveCompanion: async (c: CompanionSettings) => {
      saved.companion.push(c);
    },
    saveDaily: async () => {},
    saveGeneral: async () => {},
    saveAdvanced: async (a: AdvancedSettings) => {
      saved.advanced.push(a);
      // 后端会把路径规范化
      return { ...a, read_dirs: a.read_dirs.map((d) => d.toUpperCase()) };
    },
  } as unknown as SettingsApi;
  return { api, saved };
}

describe('陪伴设置', () => {
  it('改频率、打开安静时段都会保存', async () => {
    const f = fakeApi();
    render(<CompanionPage api={f.api} view={testView()} />);
    await act(async () => {
      fireEvent.click(screen.getByRole('radio', { name: '很少' }));
    });
    expect(f.saved.companion.at(-1)?.frequency).toBe('rare');
    expect(screen.queryByLabelText('安静时段开始')).toBeNull();
    await act(async () => {
      fireEvent.click(screen.getByText('安静时段'));
    });
    expect(f.saved.companion.at(-1)?.quiet_enabled).toBe(true);
    expect(screen.getByLabelText('安静时段开始')).toBeTruthy();
  });
});

describe('高级设置', () => {
  it('委托外部 agent 默认关闭；添加允许读取的文件夹用后端规范化后的结果', async () => {
    const f = fakeApi();
    render(<AdvancedPage api={f.api} view={testView()} onChanged={() => {}} />);
    const box = screen.getByText('允许她把事情交给 Claude Code / Codex 去做').closest('label')!.querySelector('input')!;
    expect(box.checked).toBe(false);
    fireEvent.input(screen.getByLabelText('添加允许读取的文件夹'), { target: { value: 'd:/notes' } });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '添加' }));
    });
    await act(async () => {
      await new Promise((r) => setTimeout(r, 0));
    });
    expect(f.saved.advanced.at(-1)?.read_dirs).toEqual(['d:/notes']);
    expect(screen.getByText('D:/NOTES')).toBeTruthy();
    expect((screen.getByLabelText('添加允许读取的文件夹') as HTMLInputElement).value).toBe('');
  });
});
