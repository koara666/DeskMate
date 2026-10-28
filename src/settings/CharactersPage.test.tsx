import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/preact';
import { CharactersPage } from './CharactersPage';
import type { SettingsApi } from '../lib/ipc';
import type { CharactersView } from '../lib/types';

afterEach(cleanup);

const view: CharactersView = {
  current: 'nailong',
  list: [
    {
      id: 'nailong',
      name: '奶龙',
      description: '黄色小恐龙',
      source: '',
      color: '#f5b700',
      origin: 'builtin',
      preview: '',
      user_images: ['idle'],
      quotes_fixed: 12,
      quotes_keyed: 30,
      prefs: { banned_words: [], custom_rules: [] },
    },
    {
      id: 'naiwa',
      name: '奶蛙',
      description: '小青蛙',
      source: '',
      color: '',
      origin: 'builtin',
      preview: '',
      user_images: [],
      quotes_fixed: 0,
      quotes_keyed: 0,
      prefs: { banned_words: [], custom_rules: [] },
    },
  ],
  global: { banned_words: ['旧词'], custom_rules: [] },
  states: ['idle', 'talking'],
  user_root: 'C:\\data\\characters',
};

function fakeApi() {
  return {
    characters: vi.fn(async () => view),
    savePrefs: vi.fn(async () => {}),
    selectCharacter: vi.fn(async () => {}),
  } as unknown as SettingsApi & { savePrefs: ReturnType<typeof vi.fn>; selectCharacter: ReturnType<typeof vi.fn> };
}

describe('CharactersPage', () => {
  it('保存时按行拆分、去掉空行，分别保存全局和当前角色的设置', async () => {
    const api = fakeApi();
    render(<CharactersPage api={api} />);
    await act(async () => {});
    fireEvent.input(screen.getByLabelText('奶龙 的禁词'), { target: { value: '笨蛋\n\n  傻瓜  \n' } });
    fireEvent.input(screen.getByLabelText('全局自定义要求（所有角色）'), { target: { value: '回复不超过三句话' } });
    await act(async () => {
      fireEvent.click(screen.getByText('保存'));
    });
    expect(api.savePrefs).toHaveBeenCalledWith(
      { banned_words: ['旧词'], custom_rules: ['回复不超过三句话'] },
      'nailong',
      { banned_words: ['笨蛋', '傻瓜'], custom_rules: [] },
    );
  });

  it('显示哪些状态已经放了图片，并能切换角色', async () => {
    const api = fakeApi();
    render(<CharactersPage api={api} />);
    await act(async () => {});
    expect(screen.getByText('使用中')).toBeTruthy();
    await act(async () => {
      fireEvent.click(screen.getByText('切换'));
    });
    expect(api.selectCharacter).toHaveBeenCalledWith('naiwa');
  });

  it('加载了原作台词库时显示段数，没有时不显示', async () => {
    const api = fakeApi();
    render(<CharactersPage api={api} />);
    await act(async () => {});
    expect(screen.getByText(/常驻 12 段，聊到相关话题时再用 30 段/)).toBeTruthy();
    await act(async () => {
      fireEvent.click(screen.getByText('奶蛙'));
    });
    expect(screen.queryByText(/原作台词库/)).toBeNull();
  });
});
