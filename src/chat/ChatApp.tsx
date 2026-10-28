// 聊天窗口：气泡模式（只显示最新回复）和面板模式（完整历史）。
// 模型输出一律作为纯文本渲染（Preact 自动转义），绝不使用 dangerouslySetInnerHTML。
import { Fragment } from 'preact';
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'preact/hooks';
import type { ChatApi, TaskApi } from '../lib/ipc';
import { newRequestId } from '../lib/ipc';
import { Typewriter } from '../lib/typewriter';
import { copyText } from '../lib/clipboard';
import { GAP_SECS, formatTime, nowSecs } from '../lib/time';
import type { CharacterView, ChatEvent, ChatMode, ErrorInfo, Placement, Role } from '../lib/types';
import { Composer } from './Composer';
import { TaskCard } from './TaskCard';

export type MsgStatus = 'streaming' | 'done' | 'stopped' | 'error';

export interface UiMessage {
  id: number;
  role: Role;
  text: string;
  status: MsgStatus;
  error?: ErrorInfo;
  /** 时间（Unix 秒） */
  at: number;
}

let seq = 0;
const nextId = () => ++seq;

const NETWORK_KINDS = new Set(['network', 'timeout']);
/** 气泡模式的最大高度（和后端 desktop::CHAT_COMPACT 一致） */
const COMPACT_MAX_H = 250;

interface Props {
  api: ChatApi;
  /** 任务执行（不传时不显示任务相关界面） */
  taskApi?: TaskApi;
  /** 测试时可以把打字机间隔调小 */
  typewriterTickMs?: number;
}

export function ChatApp({ api, taskApi, typewriterTickMs = 28 }: Props) {
  const [messages, setMessages] = useState<UiMessage[]>([]);
  const [mode, setModeState] = useState<ChatMode>('compact');
  const [place, setPlace] = useState<Placement>({ side: 'left', top: false });
  const [character, setCharacter] = useState<CharacterView>({ id: '', name: '', color: '' });
  const [greeting, setGreeting] = useState('');
  const [notice, setNotice] = useState<string | null>(null);
  /** 任务卡片：null = 不显示；字符串 = 预填的任务描述 */
  const [taskGoal, setTaskGoal] = useState<string | null>(null);
  const [requestId, setRequestId] = useState<string | null>(null);
  const [online, setOnline] = useState(typeof navigator === 'undefined' ? true : navigator.onLine);
  const [focusSignal, setFocusSignal] = useState(0);
  /** 打开窗口时刷新：判断最近一条回复是不是「很久以前」的 */
  const [now, setNow] = useState(nowSecs());
  const writer = useRef<Typewriter | null>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const fitted = useRef(0);
  const [ready, setReady] = useState(false);
  const stickToBottom = useRef(true);

  const load = useCallback(async () => {
    const init = await api.init();
    setCharacter(init.character);
    setGreeting(init.greeting);
    if (init.notice) setNotice(init.notice);
    setModeState(init.mode);
    setPlace(init.placement);
    setReady(true);
    setMessages(
      init.history.map((m) => ({ id: nextId(), role: m.role, text: m.content, status: 'done' as const, at: m.created_at })),
    );
  }, [api]);

  useEffect(() => {
    load().catch(() => {});
    const offs = [
      api.onPlacement(setPlace),
      api.onFocusInput(() => {
        setFocusSignal((n) => n + 1);
        setNow(nowSecs());
      }),
      api.onCharacterChanged(() => load().catch(() => {})),
      api.onProactive((text) => {
        stickToBottom.current = true;
        setMessages((ms) => [...ms, { id: nextId(), role: 'assistant', text, status: 'done', at: nowSecs() }]);
      }),
    ];
    const onVisible = () => {
      if (!document.hidden) setNow(nowSecs());
    };
    const onOnline = () => setOnline(true);
    const onOffline = () => setOnline(false);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') api.hide().catch(() => {});
    };
    window.addEventListener('online', onOnline);
    window.addEventListener('offline', onOffline);
    window.addEventListener('keydown', onKey);
    document.addEventListener('visibilitychange', onVisible);
    return () => {
      document.removeEventListener('visibilitychange', onVisible);
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

  const generating = requestId !== null;

  // 气泡模式：窗口高度跟着内容走（短回复不留一大块空白）。
  // 生成过程中只长不缩，避免文字一边出来气泡一边跳；说完再缩到合适的高度。
  useLayoutEffect(() => {
    const panel = panelRef.current;
    const area = listRef.current;
    if (!ready || mode !== 'compact' || !panel || !area) return;
    // 内容需要的高度 = 面板现在的高度 + 气泡区被裁掉的部分 + 面板上下各 6px 的留白
    const need = Math.ceil(panel.offsetHeight + area.scrollHeight - area.clientHeight + 12);
    // 窗口还在长高时不显示滚动条（否则每多一行都会闪一下），到最大高度还放不下才显示
    area.classList.toggle('growing', need <= COMPACT_MAX_H);
    if (Math.abs(need - fitted.current) < 2 || (generating && need < fitted.current)) return;
    fitted.current = need;
    api.fit(need).catch(() => {});
  });

  const update = (id: number, patch: Partial<UiMessage>) =>
    setMessages((ms) => ms.map((m) => (m.id === id ? { ...m, ...patch } : m)));

  const startStream = (start: (rid: string, onEvent: (e: ChatEvent) => void) => Promise<void>) => {
    const rid = newRequestId();
    const msgId = nextId();
    setMessages((ms) => [...ms, { id: msgId, role: 'assistant', text: '', status: 'streaming', at: nowSecs() }]);
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
        if (e.task && taskApi) openTask(e.task);
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
    setMessages((ms) => [...ms, { id: nextId(), role: 'user', text, status: 'done', at: nowSecs() }]);
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

  const openTask = (goal: string) => {
    setTaskGoal(goal);
    setMode('expanded');
  };

  const setMode = (m: ChatMode) => {
    setModeState(m);
    api.setMode(m).catch(() => {});
  };

  const lastAssistant = [...messages].reverse().find((m) => m.role === 'assistant');
  const lastMsg = messages[messages.length - 1];
  // 上次聊天已经过去很久（新的一次会话）：气泡里不再显示旧回复，换成开场白
  const bubbleMsg =
    lastAssistant && (generating || lastAssistant.status !== 'done' || now - lastAssistant.at < GAP_SECS)
      ? lastAssistant
      : null;
  const showReconnect =
    online && lastMsg?.status === 'error' && lastMsg.error && NETWORK_KINDS.has(lastMsg.error.kind);
  const accent = character.color || undefined;
  const name = character.name || '小伙伴';

  return (
    <div
      ref={panelRef}
      class={`panel mode-${mode} tail-${place.side === 'left' ? 'right' : 'left'} anchor-${place.top ? 'top' : 'bottom'}`}
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
          {taskApi && (
            <button type="button" class="icon-btn" title="交给执行工具做一件事" aria-label="新任务" onClick={() => openTask('')}>
              🛠
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
          {messages.map((m, i) => (
            <Fragment key={m.id}>
              {(i === 0 || m.at - messages[i - 1].at >= GAP_SECS) && (
                <div class="time-sep">{formatTime(m.at, now)}</div>
              )}
              <Message
                msg={m}
                isLast={m === lastAssistant && m === lastMsg}
                generating={generating}
                onRegenerate={regenerate}
              />
            </Fragment>
          ))}
        </div>
      ) : (
        <div class="bubble-area" ref={listRef}>
          {bubbleMsg ? (
            <Message
              msg={bubbleMsg}
              isLast={bubbleMsg === lastMsg}
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

      {taskApi && taskGoal !== null && (
        <TaskCard
          api={taskApi}
          initialGoal={taskGoal}
          onClose={() => setTaskGoal(null)}
          onFinished={(id) => startStream((rid, onEvent) => taskApi.report(id, rid, onEvent))}
        />
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
