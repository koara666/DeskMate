// 输入框：Enter 发送，Shift+Enter 换行。
// 中文输入法组字过程中按 Enter 是「确认候选词」，绝不能触发发送：
// - Chromium（WebView2）里这次 keydown 的 isComposing 为 true、keyCode 为 229；
// - 个别浏览器会先触发 compositionend 再触发 keydown，所以组字结束后的同一轮事件里仍然视为组字中。
import { useEffect, useRef, useState } from 'preact/hooks';

interface Props {
  generating: boolean;
  disabled?: boolean;
  placeholder?: string;
  onSend: (text: string) => void;
  onStop: () => void;
  /** 每次变化时自增，用来请求聚焦输入框 */
  focusSignal?: number;
}

export function isImeEnter(e: KeyboardEvent, composing: boolean): boolean {
  return e.isComposing || composing || e.keyCode === 229;
}

export function Composer({ generating, disabled, placeholder, onSend, onStop, focusSignal }: Props) {
  const [text, setText] = useState('');
  const composing = useRef(false);
  const ta = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    ta.current?.focus();
  }, [focusSignal]);

  // 根据内容自动调整高度（最多约 5 行）
  useEffect(() => {
    const el = ta.current;
    if (!el) return;
    el.style.height = 'auto';
    el.style.height = `${Math.min(el.scrollHeight, 110)}px`;
    el.style.overflowY = el.scrollHeight > 110 ? 'auto' : 'hidden';
  }, [text]);

  const submit = () => {
    const t = text.trim();
    if (!t || generating || disabled) return;
    onSend(t);
    setText('');
  };

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key !== 'Enter') return;
    if (isImeEnter(e, composing.current)) return; // 输入法组字中：交给输入法
    if (e.shiftKey) return; // 换行
    e.preventDefault();
    submit();
  };

  return (
    <div class="composer">
      <textarea
        ref={ta}
        rows={1}
        value={text}
        placeholder={placeholder ?? '说点什么…'}
        title="Enter 发送，Shift+Enter 换行"
        aria-label="输入消息"
        maxLength={8000}
        disabled={disabled}
        onInput={(e) => setText((e.currentTarget as HTMLTextAreaElement).value)}
        onKeyDown={onKeyDown}
        onCompositionStart={() => {
          composing.current = true;
        }}
        onCompositionEnd={() => {
          // 延后到下一轮事件再解除，兼容「compositionend 先于 keydown」的顺序
          setTimeout(() => {
            composing.current = false;
          }, 0);
        }}
      />
      {generating ? (
        <button type="button" class="btn btn-stop" onClick={onStop} title="停止生成" aria-label="停止生成">
          停止
        </button>
      ) : (
        <button
          type="button"
          class="btn btn-send"
          onClick={submit}
          disabled={!text.trim() || disabled}
          title="发送（Enter）"
          aria-label="发送"
        >
          发送
        </button>
      )}
    </div>
  );
}
