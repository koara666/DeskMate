// 输入框：Enter 发送，Shift+Enter 换行。
// 中文输入法组字过程中按 Enter 是「确认候选词」，绝不能触发发送：
// - Chromium（WebView2）里这次 keydown 的 isComposing 为 true、keyCode 为 229；
// - 个别浏览器会先触发 compositionend 再触发 keydown，所以组字结束后的同一轮事件里仍然视为组字中。
import { useEffect, useRef, useState } from 'preact/hooks';
import type { AsrPhase } from '../lib/types';

interface Props {
  generating: boolean;
  disabled?: boolean;
  placeholder?: string;
  onSend: (text: string) => void;
  onStop: () => void;
  /** 每次变化时自增，用来请求聚焦输入框 */
  focusSignal?: number;
  /** 语音输入：不传 = 没有麦克风按钮 */
  mic?: MicProps;
  /** 语音识别出来的文字（不自动发送时）：`n` 每变一次就把 `text` 接到输入框里 */
  insert?: { text: string; n: number };
}

export interface MicProps {
  phase: AsrPhase;
  /** 0~1，说话的音量 */
  level: number;
  onToggle: () => void;
}

const MIC_TITLE: Record<AsrPhase, string> = {
  idle: '语音输入：点一下开始说话',
  loading: '正在准备麦克风…',
  listening: '正在听你说…说完会自动识别；再点一下结束',
  hearing: '听到了…说完停一下就会识别；再点一下结束',
  thinking: '正在识别…',
};

export function isImeEnter(e: KeyboardEvent, composing: boolean): boolean {
  return e.isComposing || composing || e.keyCode === 229;
}

export function Composer({ generating, disabled, placeholder, onSend, onStop, focusSignal, mic, insert }: Props) {
  const [text, setText] = useState('');
  const composing = useRef(false);
  const ta = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    ta.current?.focus();
  }, [focusSignal]);

  // 语音识别的文字接在已有内容后面，并把光标留在输入框里方便改
  useEffect(() => {
    if (!insert || !insert.text) return;
    setText((t) => (t ? `${t} ${insert.text}` : insert.text));
    ta.current?.focus();
  }, [insert?.n]);

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
      {mic && (
        <button
          type="button"
          class="btn btn-mic"
          data-phase={mic.phase}
          style={{ '--mic-level': String(Math.round(mic.level * 100) / 100) }}
          onClick={mic.onToggle}
          disabled={disabled || mic.phase === 'loading' || mic.phase === 'thinking'}
          aria-pressed={mic.phase === 'listening' || mic.phase === 'hearing'}
          title={MIC_TITLE[mic.phase]}
          aria-label="语音输入"
        >
          <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
            <path
              fill="currentColor"
              d="M12 15a3 3 0 0 0 3-3V6a3 3 0 0 0-6 0v6a3 3 0 0 0 3 3Zm5-3a5 5 0 0 1-10 0H5a7 7 0 0 0 6 6.92V22h2v-3.08A7 7 0 0 0 19 12h-2Z"
            />
          </svg>
        </button>
      )}
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
