import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/preact';
import { AsrCard, type AsrApi } from './AsrCard';
import type { AsrDownload, AsrSettings, AsrStatus } from '../lib/types';
import { testView } from './fixtures';

afterEach(cleanup);
const flush = () =>
  act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });

const idle: AsrDownload = { running: false, done: 0, total: 0, error: '' };
const status = (over: Partial<AsrStatus> = {}): AsrStatus => ({
  model_ready: false,
  download: idle,
  listening: false,
  devices: [{ id: 'mic-1', name: 'USB 麦克风' }],
  ...over,
});

function setup(st: AsrStatus, initial?: Partial<AsrSettings>) {
  const saved: AsrSettings[] = [];
  const api: AsrApi = {
    asrSave: async (a) => {
      saved.push(a);
      return a;
    },
    asrStatus: vi.fn(async () => st),
    asrDownload: vi.fn(async () => {}),
  };
  render(<AsrCard api={api} initial={{ ...testView().asr, ...initial }} />);
  return { api, saved };
}

describe('语音输入设置', () => {
  it('默认关闭，选项藏着；打开后保存并显示选项', async () => {
    const { saved } = setup(status());
    await flush();
    expect(screen.queryByLabelText('连续对话')).toBeNull();
    fireEvent.click(screen.getByLabelText('打开语音输入'));
    await flush();
    expect(saved.at(-1)?.enabled).toBe(true);
    expect(screen.getByLabelText('连续对话')).toBeTruthy();
    // 默认：识别完自动发送、不连续
    expect((screen.getByLabelText('识别完自动发送') as HTMLInputElement).checked).toBe(true);
    expect((screen.getByLabelText('连续对话') as HTMLInputElement).checked).toBe(false);
  });

  it('模型没下好时显示进度；下载出错能重试', async () => {
    const running = status({ download: { running: true, done: 80 * 1048576, total: 160 * 1048576, error: '' } });
    setup(running, { enabled: true });
    await flush();
    expect(screen.getByText(/80 MB \/ 160 MB/)).toBeTruthy();
    cleanup();
    const failed = status({ download: { ...idle, error: '下载失败（超时）' } });
    const { api } = setup(failed, { enabled: true });
    await flush();
    fireEvent.click(screen.getByText('重试'));
    await flush();
    expect(api.asrDownload).toHaveBeenCalled();
  });

  it('模型就绪后可以选麦克风、打开快捷键', async () => {
    const { saved } = setup(status({ model_ready: true }), { enabled: true });
    await flush();
    expect(screen.getByText(/识别模型已就绪/)).toBeTruthy();
    fireEvent.change(screen.getByLabelText('麦克风'), { target: { value: 'mic-1' } });
    await flush();
    expect(saved.at(-1)?.input_device).toBe('mic-1');
    fireEvent.click(screen.getByLabelText('用快捷键开始说话'));
    await flush();
    expect(saved.at(-1)?.hotkey_enabled).toBe(true);
    expect((screen.getByLabelText('语音输入快捷键') as HTMLInputElement).value).toBe('Alt+Shift+V');
  });
});
