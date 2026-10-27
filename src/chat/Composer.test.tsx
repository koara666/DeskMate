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
