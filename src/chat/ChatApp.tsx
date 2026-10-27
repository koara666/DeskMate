// 聊天窗口：气泡模式（只显示最新回复）和面板模式（完整历史）。
// 模型输出一律作为纯文本渲染（Preact 自动转义），绝不使用 dangerouslySetInnerHTML。
import { useCallback, useEffect, useRef, useState } from 'preact/hooks';
import type { ChatApi } from '../lib/ipc';
import { newRequestId } from '../lib/ipc';
import { Typewriter } from '../lib/typewriter';
import { copyText } from '../lib/clipboard';
import type { CharacterView, ChatEvent, ChatMode, ErrorInfo, Role, Side } from '../lib/types';
import { Composer } from './Composer';

export type MsgStatus = 'streaming' | 'done' | 'stopped' | 'error';

export interface UiMessage {
  id: number;
  role: Role;
  text: string;
  status: MsgStatus;
  error?: ErrorInfo;
}

let seq = 0;
const nextId = () => ++seq;

const NETWORK_KINDS = new Set(['network', 'timeout']);

interface Props {
  api: ChatApi;
  /** 测试时可以把打字机间隔调小 */
  typewriterTickMs?: number;
}

export function ChatApp({ api, typewriterTickMs = 28 }: Props) {
  const [messages, setMessages] = useState<UiMessage[]>([]);
  const [mode, setModeState] = useState<ChatMode>('compact');
  const [side, setSide] = useState<Side>('left');
  const [character, setCharacter] = useState<CharacterView>({ id: '', name: '', color: '' });
  const [greeting, setGreeting] = useState('');
  const [notice, setNotice] = useState<string | null>(null);
  const [requestId, setRequestId] = useState<string | null>(null);
  const [online, setOnline] = useState(typeof navigator === 'undefined' ? true : navigator.onLine);
  const [focusSignal, setFocusSignal] = useState(0);
  const writer = useRef<Typewriter | null>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const stickToBottom = useRef(true);

  const load = useCallback(async () => {
    const init = await api.init();
    setCharacter(init.character);
    setGreeting(init.greeting);
    if (init.notice) setNotice(init.notice);
    setModeState(init.mode);
    setSide(init.side);
    setMessages(
      init.history.map((m) => ({ id: nextId(), role: m.role, text: m.content, status: 'done' as const })),
    );
  }, [api]);

  useEffect(() => {
    load().catch(() => {});
    const offs = [
      api.onPlacement(setSide),
      api.onFocusInput(() => setFocusSignal((n) => n + 1)),
      api.onCharacterChanged(() => load().catch(() => {})),
    ];
    const onOnline = () => setOnline(true);
    const onOffline = () => setOnline(false);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') api.hide().catch(() => {});
    };
    window.addEventListener('online', onOnline);
    window.addEventListener('offline', onOffline);
    window.addEventListener('keydown', onKey);
    return () => {
      offs.forEach((p) => p.then((off) => off()).catch(() => {}));
      window.removeEventListener('online', onOnline);
      window.removeEventListener('offline', onOffline);
      window.removeEventListener('keydown', onKey);
      writer.current?.dispose();
    };
  }, [api, load]);

  // 有新内容时滚到底部（用户往上翻看历史时不打扰）
  useEffect(() => {
    const el = listRef.current;
    if (el && stickToBottom.current) el.scrollTop = el.scrollHeight;
  }, [messages, mode]);

  const update = (id: number, patch: Partial<UiMessage>) =>
    setMessages((ms) => ms.map((m) => (m.id === id ? { ...m, ...patch } : m)));

  const startStream = (start: (rid: string, onEvent: (e: ChatEvent) => void) => Promise<void>) => {
    const rid = newRequestId();
    const msgId = nextId();
    setMessages((ms) => [...ms, { id: msgId, role: 'assistant', text: '', status: 'streaming' }]);
    setRequestId(rid);
    stickToBottom.current = true;
    writer.current?.dispose();
    const w = new Typewriter((t) => update(msgId, { text: t }), { tickMs: typewriterTickMs });
    writer.current = w;
    const finishWith = (patch: Partial<UiMessage>) => {
      update(msgId, patch);
      setRequestId((cur) => (cur === rid ? null : cur));
    };
    const onEvent = (e: ChatEvent) => {
      if (e.type === 'delta') {
        w.push(e.text);
      } else if (e.type === 'done') {
        if (e.finish === 'cancelled') w.flush();
        w.end(() => finishWith({ text: e.text || w.text(), status: e.finish === 'cancelled' ? 'stopped' : 'done' }));
      } else {
        w.flush();
        const { type: _t, ...info } = e;
        finishWith({ status: 'error', error: info });
      }
    };
    start(rid, onEvent).catch((err: unknown) => {
      w.flush();
      finishWith({
        status: 'error',
        error: { kind: 'internal', message: '哎呀，消息没能发出去……', detail: String(err), hint: '' },
      });
    });
  };

  const send = (text: string) => {
    setMessages((ms) => [...ms, { id: nextId(), role: 'user', text, status: 'done' }]);
    startStream((rid, onEvent) => api.send(text, rid, onEvent));
  };

  const regenerate = () => {
    setMessages((ms) => {
      const last = ms[ms.length - 1];
      return last && last.role === 'assistant' ? ms.slice(0, -1) : ms;
    });
    startStream((rid, onEvent) => api.regenerate(rid, onEvent));
  };

  const stop = () => {
    if (!requestId) return;
    writer.current?.flush();
    api.stop(requestId).catch(() => {});
  };

  const setMode = (m: ChatMode) => {
    setModeState(m);
    api.setMode(m).catch(() => {});
  };

  const generating = requestId !== null;
  const lastAssistant = [...messages].reverse().find((m) => m.role === 'assistant');
  const lastMsg = messages[messages.length - 1];
  const showReconnect =
    online && lastMsg?.status === 'error' && lastMsg.error && NETWORK_KINDS.has(lastMsg.error.kind);
  const accent = character.color || undefined;
  const name = character.name || '小伙伴';

  return (
    <div
      class={`panel mode-${mode} tail-${side === 'left' ? 'right' : 'left'}`}
      style={accent ? ({ '--accent': accent } as Record<string, string>) : undefined}
    >
      <header class="panel-header">
        <span class="name">{name}</span>
        <div class="header-actions">
          {mode === 'compact' ? (
            <button type="button" class="icon-btn" title="展开聊天记录" aria-label="展开" onClick={() => setMode('expanded')}>
              ⤢
            </button>
          ) : (
            <button type="button" class="icon-btn" title="收起为气泡" aria-label="收起" onClick={() => setMode('compact')}>
              ⤡
            </button>
          )}
          <button type="button" class="icon-btn" title="设置" aria-label="设置" onClick={() => api.openSettings().catch(() => {})}>
            ⚙
          </button>
          <button type="button" class="icon-btn" title="关闭（Esc）" aria-label="关闭" onClick={() => api.hide().catch(() => {})}>
            ✕
          </button>
        </div>
      </header>

      {notice && (
        <div class="banner banner-notice" role="status">
          {notice}
          <button type="button" class="link-btn" onClick={() => setNotice(null)}>
            知道了
          </button>
        </div>
      )}
      {!online && <div class="banner banner-offline">网络断开了，恢复后可以重试</div>}
      {showReconnect && (
        <div class="banner banner-online">
          网络好像恢复了
          <button type="button" class="link-btn" onClick={regenerate}>
            重试
          </button>
        </div>
      )}

      {mode === 'expanded' ? (
        <div
          class="messages"
          ref={listRef}
          onScroll={(e) => {
            const el = e.currentTarget as HTMLDivElement;
            stickToBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
          }}
        >
          {messages.length === 0 && <div class="empty">还没有聊天记录，和{name}打个招呼吧～</div>}
          {messages.map((m) => (
            <Message
              key={m.id}
              msg={m}
              isLast={m === lastAssistant && m === lastMsg}
              generating={generating}
              onRegenerate={regenerate}
            />
          ))}
        </div>
      ) : (
        <div class="bubble-area" ref={listRef}>
          {lastAssistant ? (
            <Message
              msg={lastAssistant}
              isLast={lastAssistant === lastMsg}
              generating={generating}
              onRegenerate={regenerate}
            />
          ) : (
            <div class="msg msg-assistant">
              <div class="text">{greeting || `有什么想和${name}聊的吗？`}</div>
            </div>
          )}
        </div>
      )}

      <Composer generating={generating} onSend={send} onStop={stop} focusSignal={focusSignal} />
    </div>
  );
}

interface MessageProps {
  msg: UiMessage;
  isLast: boolean;
  generating: boolean;
  onRegenerate: () => void;
}

function Message({ msg, isLast, generating, onRegenerate }: MessageProps) {
  const [copied, setCopied] = useState(false);
  const [showDetail, setShowDetail] = useState(false);
  const thinking = msg.status === 'streaming' && msg.text === '';

  const copy = async () => {
    if (await copyText(msg.text)) {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    }
  };

  return (
    <div class={`msg msg-${msg.role} status-${msg.status}`} data-testid={`msg-${msg.role}`}>
      {thinking ? (
        <div class="thinking" aria-label="思考中">
          <span />
          <span />
          <span />
        </div>
      ) : (
        msg.text && <div class="text">{msg.text}</div>
      )}
      {msg.status === 'stopped' && <div class="note">（已停止）</div>}
      {msg.status === 'error' && msg.error && (
        <div class="error-box" role="alert">
          <div class="error-message">{msg.error.message}</div>
          <button type="button" class="link-btn" onClick={() => setShowDetail((v) => !v)}>
            {showDetail ? '收起详情' : '查看详情'}
          </button>
          {showDetail && (
            <div class="error-detail">
              <div>{msg.error.detail}</div>
              {msg.error.hint && <div class="hint">怎么办：{msg.error.hint}</div>}
            </div>
          )}
        </div>
      )}
      {msg.role === 'assistant' && msg.status !== 'streaming' && (
        <div class="msg-actions">
          {msg.text && (
            <button type="button" class="link-btn" onClick={copy}>
              {copied ? '已复制' : '复制'}
            </button>
          )}
          {isLast && !generating && (
            <button type="button" class="link-btn" onClick={onRegenerate}>
              {msg.status === 'error' ? '重试' : '重新生成'}
            </button>
          )}
        </div>
      )}
    </div>
  );
}
