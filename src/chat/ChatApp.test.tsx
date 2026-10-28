import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/preact';
import { ChatApp } from './ChatApp';
import type { ChatApi } from '../lib/ipc';
import type { ChatEvent, HistoryMessage, ProactiveMessage, Reminder } from '../lib/types';

function fakeApi(mode: 'compact' | 'expanded' = 'expanded', history: HistoryMessage[] = []) {
  const calls = {
    send: [] as { text: string; rid: string }[],
    stop: [] as string[],
    regenerate: [] as string[],
    fit: [] as number[],
    hide: 0,
    open: [] as string[],
    read: [] as string[],
    feedback: [] as boolean[],
    reminders: [] as Reminder[],
  };
  let handler: ((e: ChatEvent) => void) | null = null;
  let proactive: ((m: ProactiveMessage) => void) | null = null;
  const off = async () => () => {};
  const api: ChatApi = {
    init: async () => ({
      character: { id: 't', name: '测试酱', color: '' },
      mode,
      placement: { side: 'left', top: false },
      history,
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
    fit: async (h) => {
      calls.fit.push(h);
    },
    hide: async () => {
      calls.hide++;
    },
    openSettings: async () => {},
    onPlacement: off,
    onFocusInput: off,
    onCharacterChanged: off,
    onOpenGames: off,
    onProactive: async (cb) => {
      proactive = cb;
      return () => {};
    },
    open: async (target) => {
      calls.open.push(target);
      return '已打开';
    },
    read: async (path, rid, onEvent) => {
      calls.read.push(path);
      calls.send.push({ text: '(read)', rid });
      handler = onEvent;
    },
    feedback: async (engaged) => {
      calls.feedback.push(engaged);
    },
    games: {
      say: async () => null,
      finish: async () => {},
      stats: async () => [],
      fortuneToday: async () => null,
      fortuneDraw: async () => ({ day: '', level: '', name: '', color: '', item: '', direction: '', comment: '' }),
    },
    choose: async (_id, index) => (index === 0 ? '那我陪着你。' : '休息一下也好。'),
    saveReminder: async (r) => {
      calls.reminders.push(r);
      return { reminder: { ...r, id: 1, when: '30 分钟后' }, line: '记下了，到时候叫你。' };
    },
  };
  const emit = (e: ChatEvent) => act(() => handler!(e));
  const say = (m: ProactiveMessage) => act(() => proactive!(m));
  return { api, calls, emit, say };
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

describe('ChatApp 气泡高度', () => {
  it('气泡模式把内容需要的高度告诉后端，高度没变时不重复通知；面板模式不通知', async () => {
    const f = fakeApi('compact');
    render(<ChatApp api={f.api} typewriterTickMs={10} />);
    await act(async () => {});
    expect(f.calls.fit).toHaveLength(1);
    expect(f.calls.fit[0]).toBeGreaterThan(0);
    await sendText('你好');
    expect(f.calls.fit).toHaveLength(1); // 测试环境没有真实排版，高度不变
    cleanup();

    const g = fakeApi('expanded');
    render(<ChatApp api={g.api} typewriterTickMs={10} />);
    await act(async () => {});
    expect(g.calls.fit).toHaveLength(0);
  });
});

describe('ChatApp 时间', () => {
  const nowS = Math.floor(Date.now() / 1000);
  const msg = (role: 'user' | 'assistant', content: string, ago: number): HistoryMessage => ({
    role,
    content,
    created_at: nowS - ago,
  });

  it('气泡：上次回复是很久以前的，显示开场白；刚刚的回复照常显示', async () => {
    const old = fakeApi('compact', [msg('user', '晚安', 7300), msg('assistant', '晚安，明天见', 7200)]);
    render(<ChatApp api={old.api} typewriterTickMs={10} />);
    await act(async () => {});
    expect(screen.queryByText('晚安，明天见')).toBeNull();
    expect(screen.getByText('你好呀～')).toBeTruthy();
    cleanup();

    const recent = fakeApi('compact', [msg('user', '在吗', 70), msg('assistant', '在呢', 60)]);
    render(<ChatApp api={recent.api} typewriterTickMs={10} />);
    await act(async () => {});
    expect(screen.getByText('在呢')).toBeTruthy();
  });

  it('面板：每段聊天前显示时间，间隔不到 30 分钟的不重复显示', async () => {
    const f = fakeApi('expanded', [
      msg('user', 'A', 7300),
      msg('assistant', 'B', 7200),
      msg('user', 'C', 100),
      msg('assistant', 'D', 90),
    ]);
    const { container } = render(<ChatApp api={f.api} typewriterTickMs={10} />);
    await act(async () => {});
    expect(container.querySelectorAll('.time-sep')).toHaveLength(2);
  });
});

describe('ChatApp 键盘', () => {
  it('输入法组字时按 Esc 只取消候选词，不关窗口；平时按 Esc 关窗口', async () => {
    const f = await mount();
    fireEvent.keyDown(window, { key: 'Escape', isComposing: true });
    fireEvent.keyDown(window, { key: 'Escape', keyCode: 229 });
    expect(f.calls.hide).toBe(0);
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(f.calls.hide).toBe(1);
  });
});

describe('ChatApp 动作按钮（内置能力层）', () => {
  it('角色提议打开：用户点了按钮才打开；不能打开的只显示原因', async () => {
    const f = await mount();
    await sendText('帮我打开B站');
    f.emit({
      type: 'done',
      text: '好～',
      finish: 'stop',
      actions: [
        { kind: 'open', target: 'https://www.bilibili.com', label: '打开网页 https://www.bilibili.com', error: null },
        { kind: 'open', target: 'C:/x.exe', label: '打开 C:/x.exe', error: '程序不能直接打开' },
      ],
    });
    act(() => {
      vi.advanceTimersByTime(500);
    });
    expect(f.calls.open).toHaveLength(0);
    expect(screen.getByText('程序不能直接打开')).toBeTruthy();
    const buttons = screen.getAllByRole('button', { name: '打开' });
    expect(buttons).toHaveLength(1);
    await act(async () => {
      fireEvent.click(buttons[0]);
    });
    await act(async () => {});
    expect(f.calls.open).toEqual(['https://www.bilibili.com']);
    expect(screen.getByText('已打开')).toBeTruthy();
  });

  it('点「读取」后由角色说说文件内容（新的一轮流式回复）', async () => {
    const f = await mount();
    await sendText('看看这个文件');
    f.emit({
      type: 'done',
      text: '好',
      finish: 'stop',
      actions: [{ kind: 'read', target: 'D:/笔记/a.txt', label: '读取文件 D:/笔记/a.txt', error: null }],
    });
    act(() => {
      vi.advanceTimersByTime(500);
    });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '读取' }));
    });
    expect(f.calls.read).toEqual(['D:/笔记/a.txt']);
    expect(screen.getByLabelText('思考中')).toBeTruthy();
  });
});

describe('ChatApp 主动发言', () => {
  it('无人理会一段时间后自动收起，并告诉调度器被忽略了', async () => {
    const f = await mount();
    f.say({ id: 1, kind: 'random', text: '在忙吗？', popped: true });
    expect(assistantText()).toContain('在忙吗？');
    act(() => {
      vi.advanceTimersByTime(30_000);
    });
    expect(f.calls.hide).toBe(0);
    act(() => {
      vi.advanceTimersByTime(30_000);
    });
    expect(f.calls.hide).toBe(1);
    expect(f.calls.feedback).toEqual([false]);
  });

  it('鼠标停在气泡上时不收起；点一下算理会了', async () => {
    const f = await mount();
    f.say({ id: 2, kind: 'care', text: '休息一下吧', popped: true });
    const panel = document.querySelector('.panel')!;
    fireEvent.pointerEnter(panel);
    act(() => {
      vi.advanceTimersByTime(200_000);
    });
    expect(f.calls.hide).toBe(0);
    fireEvent.pointerDown(panel);
    expect(f.calls.feedback).toEqual([true]);
    fireEvent.pointerLeave(panel);
    act(() => {
      vi.advanceTimersByTime(200_000);
    });
    expect(f.calls.hide).toBe(0);
  });

  it('全屏时改发系统通知的（没弹气泡）不计时', async () => {
    const f = await mount();
    f.say({ id: 3, kind: 'reminder', text: '收衣服', popped: false });
    act(() => {
      vi.advanceTimersByTime(200_000);
    });
    expect(f.calls.hide).toBe(0);
    expect(f.calls.feedback).toEqual([]);
  });
});

describe('ChatApp 提醒', () => {
  const draft = {
    when: '30 分钟后（15:42）',
    reminder: {
      id: 0,
      content: '收衣服',
      trigger: 'at' as const,
      at: 1_800_000_000,
      repeat: { type: 'none' as const },
      activity: '',
      line: '收衣服啦',
      important: false,
      status: 'active' as const,
      created_at: 0,
      last_fired_at: null,
    },
  };

  it('复述后用户点「好」才保存，角色接一句「记下了」', async () => {
    const f = await mount();
    await sendText('半小时后提醒我收衣服');
    f.emit({ type: 'done', text: '好，30 分钟后提醒你收衣服，可以吗？', finish: 'stop', reminder: draft });
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    expect(screen.getByText(/30 分钟后（15:42） · 收衣服/)).toBeTruthy();
    expect(f.calls.reminders).toHaveLength(0);
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '好' }));
    });
    await act(async () => {});
    expect(f.calls.reminders[0].content).toBe('收衣服');
    expect(assistantText()).toContain('记下了');
    expect(screen.getByText(/已设好/)).toBeTruthy();
  });

  it('「不用了」不保存；没听懂时可以手动设置', async () => {
    const f = await mount();
    await sendText('半小时后提醒我收衣服');
    f.emit({ type: 'done', text: '……', finish: 'stop', reminder: draft });
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    fireEvent.click(screen.getByRole('button', { name: '不用了' }));
    expect(screen.getByText('（没有设置提醒）')).toBeTruthy();
    expect(f.calls.reminders).toHaveLength(0);

    await sendText('提醒我那个事');
    f.emit({ type: 'done', text: '没听懂', finish: 'stop', reminder_form: true });
    act(() => {
      vi.advanceTimersByTime(1000);
    });
    fireEvent.input(screen.getByLabelText('要提醒的事'), { target: { value: '吃药' } });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '保存' }));
    });
    await act(async () => {});
    expect(f.calls.reminders.at(-1)?.content).toBe('吃药');
    // 手动填的台词由后端生成
    expect(f.calls.reminders.at(-1)?.line).toBe('');
  });
});

describe('ChatApp 小事件', () => {
  it('给出选项，点了以后显示我的选择和她的回应', async () => {
    const f = await mount();
    f.say({ id: 7, kind: 'random', text: '待会儿想做什么？', popped: true, choices: ['继续努力', '休息一下'] });
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: '休息一下' }));
    });
    await act(async () => {});
    expect(screen.queryByRole('button', { name: '继续努力' })).toBeNull();
    expect(assistantText()).toContain('休息一下也好');
    expect(f.calls.feedback).toEqual([true]);
  });
});
