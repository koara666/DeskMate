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
    companionStatus: async () => ({
      character: '七海千秋',
      level: 2,
      points: 120,
      next: 250,
      title: '游戏搭档',
      mood: 30,
      mood_word: '不错',
      nickname: '小林',
      call: '小林',
    }),
    saveNickname: async (n: string) => n,
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
    // 你和她：关系和称呼
    expect(await screen.findByText('游戏搭档（亲密度 2/5）')).toBeTruthy();
    expect((screen.getByLabelText('称呼') as HTMLInputElement).value).toBe('小林');
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

describe('每日播报设置', () => {
  it('查找城市后选中保存；添加播报时间和纪念日', async () => {
    const saved: unknown[] = [];
    const api = {
      companionStatus: async () => ({
        character: '七海千秋',
        level: 2,
        points: 120,
        next: 250,
        title: '游戏搭档',
        mood: 30,
        mood_word: '不错',
        nickname: '小林',
        call: '小林',
      }),
      saveNickname: async (n: string) => n,
      saveCompanion: async () => {},
      saveDaily: async () => {},
      saveBriefing: async (b: unknown) => {
        saved.push(b);
        return b;
      },
      citySearch: async (q: string) => (q === '大阪' ? [{ name: '大阪', country: 'JP', lat: 34.69, lon: 135.5, tz: 'Asia/Tokyo' }] : []),
      briefingNow: async () => {},
    } as unknown as SettingsApi;
    render(<CompanionPage api={api} view={testView()} />);
    expect(screen.getByText('还没有选城市')).toBeTruthy();
    fireEvent.input(screen.getByLabelText('城市'), { target: { value: '大阪' } });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '查找' }));
    });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '选这个' }));
    });
    await act(async () => {
      await new Promise((r) => setTimeout(r, 0));
    });
    expect((saved.at(-1) as { city: { name: string } }).city.name).toBe('大阪');
    expect(screen.getByText('现在：大阪（JP）')).toBeTruthy();

    fireEvent.input(screen.getByLabelText('新的播报时间'), { target: { value: '23:00' } });
    await act(async () => {
      fireEvent.click(screen.getAllByRole('button', { name: '添加' })[0]);
    });
    expect((saved.at(-1) as { times: string[] }).times).toEqual(['23:00']);
  });
});
