import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/preact';
import { Composer } from './Composer';

afterEach(cleanup);

function setup(generating = false) {
  const onSend = vi.fn();
  const onStop = vi.fn();
  render(<Composer generating={generating} onSend={onSend} onStop={onStop} />);
  const ta = screen.getByLabelText('输入消息') as HTMLTextAreaElement;
  fireEvent.input(ta, { target: { value: '你好' } });
  return { ta, onSend, onStop };
}

describe('Composer 输入框', () => {
  it('Enter 发送并清空', () => {
    const { ta, onSend } = setup();
    fireEvent.keyDown(ta, { key: 'Enter' });
    expect(onSend).toHaveBeenCalledWith('你好');
    expect(ta.value).toBe('');
  });

  it('Shift+Enter 不发送（换行）', () => {
    const { ta, onSend } = setup();
    fireEvent.keyDown(ta, { key: 'Enter', shiftKey: true });
    expect(onSend).not.toHaveBeenCalled();
  });

  it('输入法组字中按 Enter（isComposing）不发送', () => {
    const { ta, onSend } = setup();
    fireEvent.keyDown(ta, { key: 'Enter', isComposing: true });
    expect(onSend).not.toHaveBeenCalled();
  });

  it('输入法组字中按 Enter（keyCode 229）不发送', () => {
    const { ta, onSend } = setup();
    fireEvent.keyDown(ta, { key: 'Enter', keyCode: 229 });
    expect(onSend).not.toHaveBeenCalled();
  });

  it('compositionstart 之后、compositionend 之前按 Enter 不发送；组字结束后可以发送', async () => {
    vi.useFakeTimers();
    try {
      const { ta, onSend } = setup();
      fireEvent.compositionStart(ta);
      fireEvent.keyDown(ta, { key: 'Enter' });
      expect(onSend).not.toHaveBeenCalled();
      // 有的浏览器先 compositionend 再 keydown：同一轮事件里仍然不能发送
      fireEvent.compositionEnd(ta);
      fireEvent.keyDown(ta, { key: 'Enter' });
      expect(onSend).not.toHaveBeenCalled();
      vi.runAllTimers();
      fireEvent.keyDown(ta, { key: 'Enter' });
      expect(onSend).toHaveBeenCalledWith('你好');
    } finally {
      vi.useRealTimers();
    }
  });

  it('空白内容不发送', () => {
    const { ta, onSend } = setup();
    fireEvent.input(ta, { target: { value: '   ' } });
    fireEvent.keyDown(ta, { key: 'Enter' });
    expect(onSend).not.toHaveBeenCalled();
  });

  it('生成中显示停止按钮，不会重复发送', () => {
    const { ta, onSend, onStop } = setup(true);
    fireEvent.keyDown(ta, { key: 'Enter' });
    expect(onSend).not.toHaveBeenCalled();
    fireEvent.click(screen.getByLabelText('停止生成'));
    expect(onStop).toHaveBeenCalled();
  });
});

describe('Composer 语音输入', () => {
  it('没有 mic 属性时没有麦克风按钮', () => {
    render(<Composer generating={false} onSend={vi.fn()} onStop={vi.fn()} />);
    expect(screen.queryByLabelText('语音输入')).toBeNull();
  });

  it('点麦克风按钮触发 onToggle，状态写在 data-phase 上', () => {
    const onToggle = vi.fn();
    const { rerender } = render(
      <Composer generating={false} onSend={vi.fn()} onStop={vi.fn()} mic={{ phase: 'idle', level: 0, onToggle }} />,
    );
    const btn = screen.getByLabelText('语音输入') as HTMLButtonElement;
    expect(btn.dataset.phase).toBe('idle');
    fireEvent.click(btn);
    expect(onToggle).toHaveBeenCalledTimes(1);
    rerender(<Composer generating={false} onSend={vi.fn()} onStop={vi.fn()} mic={{ phase: 'hearing', level: 0.5, onToggle }} />);
    expect(btn.dataset.phase).toBe('hearing');
    expect(btn.getAttribute('aria-pressed')).toBe('true');
    // 正在识别时不能再点
    rerender(<Composer generating={false} onSend={vi.fn()} onStop={vi.fn()} mic={{ phase: 'thinking', level: 0, onToggle }} />);
    expect(btn.disabled).toBe(true);
  });

  it('识别出来的文字接在输入框已有内容后面', () => {
    const { rerender } = render(<Composer generating={false} onSend={vi.fn()} onStop={vi.fn()} insert={{ text: '', n: 0 }} />);
    const ta = screen.getByLabelText('输入消息') as HTMLTextAreaElement;
    fireEvent.input(ta, { target: { value: '帮我' } });
    rerender(<Composer generating={false} onSend={vi.fn()} onStop={vi.fn()} insert={{ text: '查一下天气', n: 1 }} />);
    expect(ta.value).toBe('帮我 查一下天气');
    rerender(<Composer generating={false} onSend={vi.fn()} onStop={vi.fn()} insert={{ text: '谢谢', n: 2 }} />);
    expect(ta.value).toBe('帮我 查一下天气 谢谢');
  });
});
