import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/preact';
import { ChatApp } from './ChatApp';
import type { ChatApi } from '../lib/ipc';
import type { ChatEvent } from '../lib/types';

function fakeApi() {
  const calls = { send: [] as { text: string; rid: string }[], stop: [] as string[], regenerate: [] as string[] };
  let handler: ((e: ChatEvent) => void) | null = null;
  const off = async () => () => {};
  const api: ChatApi = {
    init: async () => ({
      character: { id: 't', name: '测试酱', color: '' },
      mode: 'expanded',
      side: 'left',
      history: [],
      greeting: '你好呀～',
      notice: null,
    }),
    send: async (text, rid, onEvent) => {
      calls.send.push({ text, rid });
      handler = onEvent;
    },
    regenerate: async (rid, onEvent) => {
      calls.regenerate.push(rid);
      handler = onEvent;
    },
    stop: async (rid) => {
      calls.stop.push(rid);
      return true;
    },
    setMode: async () => {},
    hide: async () => {},
    openSettings: async () => {},
    onPlacement: off,
    onFocusInput: off,
    onCharacterChanged: off,
    onProactive: off,
  };
  const emit = (e: ChatEvent) => act(() => handler!(e));
  return { api, calls, emit };
}

async function mount() {
  const f = fakeApi();
  render(<ChatApp api={f.api} typewriterTickMs={10} />);
  await act(async () => {}); // 等 init 完成
  return f;
}

async function sendText(text: string) {
  const ta = screen.getByLabelText('输入消息');
  fireEvent.input(ta, { target: { value: text } });
  await act(async () => {
    fireEvent.keyDown(ta, { key: 'Enter' });
  });
}

const assistantText = () => screen.getAllByTestId('msg-assistant').at(-1)!.textContent ?? '';

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe('ChatApp 流式显示', () => {
  it('逐字显示增量，结束后显示完整内容和操作按钮', async () => {
    const f = await mount();
    await sendText('你好');
    expect(f.calls.send).toHaveLength(1);
    expect(f.calls.send[0].text).toBe('你好');
    expect(screen.getByLabelText('思考中')).toBeTruthy();

    f.emit({ type: 'delta', text: 'ABCDEFGHIJKL' });
    act(() => {
      vi.advanceTimersByTime(10);
    });
    const partial = assistantText();
    expect(partial.length).toBeGreaterThan(0);
    expect(partial).not.toContain('ABCDEFGHIJKL'); // 还没显示完：确实是逐步出现

    f.emit({ type: 'delta', text: 'MN' });
    f.emit({ type: 'done', text: 'ABCDEFGHIJKLMN', finish: 'stop' });
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(assistantText()).toContain('ABCDEFGHIJKLMN');
    expect(screen.getByText('复制')).toBeTruthy();
    expect(screen.getByText('重新生成')).toBeTruthy();
    expect(screen.getByLabelText('发送')).toBeTruthy();
  });

  it('模型输出里的 HTML 按纯文本显示，不会被执行', async () => {
    const f = await mount();
    await sendText('hi');
    f.emit({ type: 'delta', text: '<img src=x onerror="alert(1)"><b>粗</b>' });
    f.emit({ type: 'done', text: '<img src=x onerror="alert(1)"><b>粗</b>', finish: 'stop' });
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(document.querySelector('.messages img')).toBeNull();
    expect(document.querySelector('.messages b')).toBeNull();
    expect(assistantText()).toContain('<img src=x');
  });
});

describe('ChatApp 停止生成', () => {
  it('点停止会用同一个请求编号通知后端，并立即显示已收到的内容', async () => {
    const f = await mount();
    await sendText('讲个长故事');
    f.emit({ type: 'delta', text: '从前有座山，山里有座庙' });
    const stopBtn = screen.getByLabelText('停止生成');
    await act(async () => {
      fireEvent.click(stopBtn);
    });
    expect(f.calls.stop).toEqual([f.calls.send[0].rid]);
    expect(assistantText()).toContain('从前有座山，山里有座庙'); // flush 后立即全部显示

    f.emit({ type: 'done', text: '从前有座山，山里有座庙', finish: 'cancelled' });
    act(() => {
      vi.advanceTimersByTime(100);
    });
    expect(screen.getByText('（已停止）')).toBeTruthy();
    expect(screen.queryByLabelText('停止生成')).toBeNull();
    expect(screen.getByLabelText('发送')).toBeTruthy();
  });

  it('重新生成会去掉上一条回复并重新请求', async () => {
    const f = await mount();
    await sendText('你好');
    f.emit({ type: 'done', text: '第一版', finish: 'stop' });
    act(() => {
      vi.advanceTimersByTime(100);
    });
    await act(async () => {
      fireEvent.click(screen.getByText('重新生成'));
    });
    expect(f.calls.regenerate).toHaveLength(1);
    expect(screen.queryByText('第一版')).toBeNull();
  });
});

describe('ChatApp 错误提示', () => {
  it('显示角色口吻的错误，点「查看详情」展开真实错误和建议', async () => {
    const f = await mount();
    await sendText('你好');
    f.emit({
      type: 'error',
      kind: 'invalid_key',
      message: '呜，钥匙好像不对……',
      detail: 'API Key 无效或没有权限：401',
      hint: '检查设置里的 Key',
    });
    expect(screen.getByText('呜，钥匙好像不对……')).toBeTruthy();
    expect(screen.queryByText(/401/)).toBeNull();
    fireEvent.click(screen.getByText('查看详情'));
    expect(screen.getByText(/401/)).toBeTruthy();
    expect(screen.getByText(/检查设置里的 Key/)).toBeTruthy();
    expect(screen.getByText('重试')).toBeTruthy();
  });
});
