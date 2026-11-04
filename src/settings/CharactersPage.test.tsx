import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen, within } from '@testing-library/preact';
import { CharactersPage } from './CharactersPage';
import type { SettingsApi } from '../lib/ipc';
import type { CharactersView, PackProgress, PackView } from '../lib/types';

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

describe('CharactersPage 下载角色', () => {
  const pack = (over: Partial<PackView> = {}): PackView => ({
    id: 'nanami-chiaki',
    name: '七海千秋',
    description: '爱打游戏、总是有点困',
    color: '#e88fb0',
    size: 5_242_880,
    preview: '',
    installed: false,
    installed_version: 0,
    latest_version: 1,
    update: false,
    running: false,
    done: 0,
    total: 5_242_880,
    error: '',
    ...over,
  });
  const empty: CharactersView = { ...view, current: '', list: [] };

  function packApi(chars: CharactersView, packs: PackView[]) {
    let onProgress: ((p: PackProgress) => void) | null = null;
    const api = {
      characters: vi.fn(async () => chars),
      charpackList: vi.fn(async () => packs),
      charpackDownload: vi.fn(async () => {}),
      charpackCancel: vi.fn(async () => {}),
      charpackRemove: vi.fn(async () => {}),
      selectCharacter: vi.fn(async () => {}),
      onCharpackProgress: vi.fn(async (cb: (p: PackProgress) => void) => {
        onProgress = cb;
        return () => {};
      }),
      onCharacterChanged: vi.fn(async () => () => {}),
    };
    return { api: api as unknown as SettingsApi, mocks: api, progress: (p: PackProgress) => onProgress?.(p) };
  }

  it('一个角色都没有（全新安装）：写明要先下载一位，每位角色都有下载按钮和大小', async () => {
    const { api, mocks } = packApi(empty, [pack(), pack({ id: 'makise-kurisu', name: '牧濑红莉栖' })]);
    render(<CharactersPage api={api} />);
    await act(async () => {});
    expect(screen.getByTestId('no-character')).toBeTruthy();
    expect(screen.getAllByText('下载（5.0 MB）')).toHaveLength(2);
    await act(async () => {
      fireEvent.click(within(screen.getByTestId('pack-makise-kurisu')).getByText('下载（5.0 MB）'));
    });
    expect(mocks.charpackDownload).toHaveBeenCalledWith('makise-kurisu');
  });

  it('下载中显示进度和取消；进度事件更新那一张卡，结束后重新读一遍', async () => {
    const { api, mocks, progress } = packApi(empty, [pack({ running: true, done: 1_048_576 })]);
    render(<CharactersPage api={api} />);
    await act(async () => {});
    const card = screen.getByTestId('pack-nanami-chiaki');
    expect(within(card).getByText('1.0 MB / 5.0 MB')).toBeTruthy();
    act(() => progress({ id: 'nanami-chiaki', running: true, done: 3_145_728, total: 5_242_880, error: '' }));
    expect(within(card).getByText('3.0 MB / 5.0 MB')).toBeTruthy();
    await act(async () => {
      fireEvent.click(within(card).getByText('取消'));
    });
    expect(mocks.charpackCancel).toHaveBeenCalledWith('nanami-chiaki');
    const calls = mocks.charpackList.mock.calls.length;
    await act(async () => progress({ id: 'nanami-chiaki', running: false, done: 0, total: 5_242_880, error: '' }));
    expect(mocks.charpackList.mock.calls.length).toBeGreaterThan(calls);
  });

  it('下载失败：显示原因，按钮变成重试', async () => {
    const { api, mocks } = packApi(empty, [pack({ error: '下载失败（网络不通）' })]);
    render(<CharactersPage api={api} />);
    await act(async () => {});
    const card = screen.getByTestId('pack-nanami-chiaki');
    expect(within(card).getByRole('alert').textContent).toContain('网络不通');
    await act(async () => {
      fireEvent.click(within(card).getByText('重试'));
    });
    expect(mocks.charpackDownload).toHaveBeenCalledWith('nanami-chiaki');
  });

  it('已下载的：正在用的不能删，别的可以删；有新版本时能更新', async () => {
    const { api, mocks } = packApi(view, [
      pack({ id: 'nailong', name: '奶龙', installed: true, installed_version: 1 }),
      pack({ id: 'naiwa', name: '奶蛙', installed: true, installed_version: 1, update: true, latest_version: 2 }),
    ]);
    render(<CharactersPage api={api} />);
    await act(async () => {});
    const cur = screen.getByTestId('pack-nailong');
    expect(within(cur).getByText('使用中，不能删除')).toBeTruthy();
    expect(within(cur).queryByText('删除')).toBeNull();
    const other = screen.getByTestId('pack-naiwa');
    expect(within(other).getByText('有新版本')).toBeTruthy();
    await act(async () => {
      fireEvent.click(within(other).getByText('更新'));
    });
    expect(mocks.charpackDownload).toHaveBeenCalledWith('naiwa');
    await act(async () => {
      fireEvent.click(within(other).getByText('删除'));
    });
    expect(mocks.charpackRemove).toHaveBeenCalledWith('naiwa');
  });
});
