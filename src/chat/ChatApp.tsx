// 聊天窗口：气泡模式（只显示最新回复）和面板模式（完整历史）。
// 模型输出一律作为纯文本渲染（Preact 自动转义），绝不使用 dangerouslySetInnerHTML。
import { Fragment } from 'preact';
import { memo } from '../lib/memo';
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'preact/hooks';
import type { ComponentType } from 'preact';
import type { ChatApi, TaskApi } from '../lib/ipc';
import { newRequestId } from '../lib/ipc';
import { Typewriter } from '../lib/typewriter';
import { copyText } from '../lib/clipboard';
import { GAP_SECS, formatTime, nowSecs } from '../lib/time';
import type {
  ActionView,
  CharacterView,
  ChatEvent,
  ChatMode,
  ErrorInfo,
  Placement,
  ProactiveMessage,
  Reminder,
  ReminderDraft,
  Role,
} from '../lib/types';
import { Composer } from './Composer';
import { ReminderForm, emptyReminder } from '../reminders/ReminderForm';
// 任务卡片（委托外部 agent，高级选项）按需加载：没开委托时根本不加载这部分代码
type TaskCardType = typeof import('./TaskCard').TaskCard;
// 小游戏也按需加载：点了 🎮 才加载
type GameHostType = typeof import('../games/GameHost').GameHost;

export type MsgStatus = 'streaming' | 'done' | 'stopped' | 'error';

export interface UiMessage {
  id: number;
  role: Role;
  text: string;
  status: MsgStatus;
  error?: ErrorInfo;
  /** 时间（Unix 秒） */
  at: number;
  /** 角色提议的打开 / 读取（用户点了才做） */
  actions?: ActionView[];
  /** 用户吩咐的提醒，等确认 */
  reminder?: ReminderDraft;
  /** 没听懂的提醒：给「手动设置」 */
  reminderForm?: boolean;
  /** 小事件的选项（id 是主动消息的编号） */
  choices?: { id: number; options: string[] };
  /** 下面一行小字（比如看屏幕时「看了「Hades」· 遮住 2 处」） */
  note?: string;
}

let seq = 0;
const nextId = () => ++seq;

const NETWORK_KINDS = new Set(['network', 'timeout']);
/** 主动弹出的气泡无人理会多久后自动收起（毫秒）：至少 45 秒，字多就多给点时间 */
export const proactiveHideMs = (text: string) => Math.min(120_000, 45_000 + text.length * 300);
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
  const [TaskCard, setTaskCard] = useState<ComponentType<Parameters<TaskCardType>[0]> | null>(null);
  const [GameHost, setGameHost] = useState<ComponentType<Parameters<GameHostType>[0]> | null>(null);
  const [playing, setPlaying] = useState(false);
  /** 主动弹出的气泡：用户理会前，无人理会一段时间就自动收起（悬停在气泡上时不收） */
  const proactive = useRef<{ timer: ReturnType<typeof setTimeout> | null; hovering: boolean } | null>(null);
  /** 用户上次在聊天窗口里打字、发消息的时间（正在聊天时不自动收起） */
  const lastActivity = useRef(0);

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
      api.onOpenGames(() => openGamesRef.current()),
      api.onProactive((m) => {
        stickToBottom.current = true;
        setMessages((ms) => [
          ...ms,
          {
            id: nextId(),
            role: 'assistant',
            text: m.text,
            status: 'done',
            at: nowSecs(),
            choices: m.choices?.length ? { id: m.id, options: m.choices } : undefined,
            note: m.note || undefined,
          },
        ]);
        armProactive(m);
      }),
    ];
    const onVisible = () => {
      if (!document.hidden) setNow(nowSecs());
    };
    const onOnline = () => setOnline(true);
    const onOffline = () => setOnline(false);
    const onKey = (e: KeyboardEvent) => {
      // 输入法组字时按 Esc 是取消候选词，不关窗口
      if (e.key === 'Escape' && !e.isComposing && e.keyCode !== 229) api.hide().catch(() => {});
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
      if (proactive.current?.timer) clearTimeout(proactive.current.timer);
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

  // ---------- 主动发言：无人理会自动收起，并告诉调度器（连续被忽略会降低频率） ----------
  function armProactive(m: ProactiveMessage) {
    if (!m.popped || Date.now() - lastActivity.current < 60_000) return;
    if (proactive.current?.timer) clearTimeout(proactive.current.timer);
    const p = { timer: null as ReturnType<typeof setTimeout> | null, hovering: false };
    proactive.current = p;
    const check = () => {
      if (proactive.current !== p) return;
      if (p.hovering) {
        p.timer = setTimeout(check, 10_000);
        return;
      }
      proactive.current = null;
      api.hide().catch(() => {});
      api.feedback(false).catch(() => {});
    };
    p.timer = setTimeout(check, proactiveHideMs(m.text));
  }
  /** 用户理会了（点了、打字了、发消息了） */
  const engage = () => {
    lastActivity.current = Date.now();
    const p = proactive.current;
    if (!p) return;
    if (p.timer) clearTimeout(p.timer);
    proactive.current = null;
    api.feedback(true).catch(() => {});
  };

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
        const actions = e.actions?.length ? e.actions : undefined;
        w.end(() =>
          finishWith({
            text: e.text || w.text(),
            status: e.finish === 'cancelled' ? 'stopped' : 'done',
            actions,
            reminder: e.reminder,
            reminderForm: e.reminder_form || undefined,
          }),
        );
        // 只有允许委托外部 agent 时后端才会带上任务（DECISIONS #33）
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
    engage();
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
    if (!TaskCard) {
      import('./TaskCard')
        .then((m) => setTaskCard(() => m.TaskCard))
        .catch(() => {});
    }
    setTaskGoal(goal);
    setMode('expanded');
  };

  // 气泡里的「打开 / 读取」按钮（处理函数放在 ref 里，消息组件的属性保持不变，不用跟着重新渲染）
  const actionRef = useRef<(a: ActionView) => Promise<string>>(async () => '');
  actionRef.current = async (a: ActionView) => {
    engage();
    if (a.kind === 'read') {
      startStream((rid, onEvent) => api.read(a.target, rid, onEvent));
      return '';
    }
    return api.open(a.target);
  };
  const onAction = useCallback((a: ActionView) => actionRef.current(a), []);

  // 确认 / 手动填写提醒：保存后角色说一句「记下了」
  const reminderRef = useRef<(r: Reminder) => Promise<void>>(async () => {});
  reminderRef.current = async (r: Reminder) => {
    engage();
    const saved = await api.saveReminder(r);
    stickToBottom.current = true;
    setMessages((ms) => [...ms, { id: nextId(), role: 'assistant', text: saved.line, status: 'done', at: nowSecs() }]);
  };
  const onReminder = useCallback((r: Reminder) => reminderRef.current(r), []);

  // 小事件：选了一项，角色回应
  const chooseRef = useRef<(msgId: number, id: number, index: number, option: string) => Promise<void>>(async () => {});
  chooseRef.current = async (msgId, id, index, option) => {
    engage();
    setMessages((ms) => ms.map((m) => (m.id === msgId ? { ...m, choices: undefined } : m)));
    const add = (role: Role, text: string) =>
      setMessages((ms) => [...ms, { id: nextId(), role, text, status: 'done', at: nowSecs() }]);
    add('user', option);
    stickToBottom.current = true;
    try {
      add('assistant', await api.choose(id, index));
    } catch (e) {
      add('assistant', String(e));
    }
  };
  const onChoose = useCallback(
    (msgId: number, id: number, index: number, option: string) => chooseRef.current(msgId, id, index, option),
    [],
  );

  const openGamesRef = useRef(() => {});
  const openGames = () => {
    engage();
    if (!GameHost) {
      import('../games/GameHost')
        .then((m) => setGameHost(() => m.GameHost))
        .catch(() => {});
    }
    setPlaying(true);
    setMode('expanded');
  };
  openGamesRef.current = openGames;

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
      onPointerEnter={() => {
        if (proactive.current) proactive.current.hovering = true;
      }}
      onPointerLeave={() => {
        if (proactive.current) proactive.current.hovering = false;
      }}
      onPointerDown={engage}
      onKeyDown={engage}
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
          <button type="button" class="icon-btn" title="一起玩小游戏" aria-label="小游戏" onClick={openGames}>
            🎮
          </button>
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

      {playing ? (
        GameHost ? (
          <GameHost api={api.games} name={name} onClose={() => setPlaying(false)} />
        ) : (
          <div class="messages muted">加载中…</div>
        )
      ) : mode === 'expanded' ? (
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
                onRegenerate={m === lastMsg ? regenerate : undefined}
                onAction={onAction}
                onReminder={onReminder}
                onChoose={onChoose}
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
              onAction={onAction}
              onReminder={onReminder}
              onChoose={onChoose}
            />
          ) : (
            <div class="msg msg-assistant">
              <div class="text">{greeting || `有什么想和${name}聊的吗？`}</div>
            </div>
          )}
        </div>
      )}

      {taskApi && TaskCard && taskGoal !== null && (
        <TaskCard
          api={taskApi}
          initialGoal={taskGoal}
          onClose={() => setTaskGoal(null)}
          onFinished={(id) => startStream((rid, onEvent) => taskApi.report(id, rid, onEvent))}
        />
      )}
      {!playing && <Composer generating={generating} onSend={send} onStop={stop} focusSignal={focusSignal} />}
    </div>
  );
}

interface MessageProps {
  msg: UiMessage;
  isLast: boolean;
  generating: boolean;
  /** 只有最后一条需要（其余消息的属性不变，打字机每次刷新时就不用重新渲染它们） */
  onRegenerate?: () => void;
  /** 点了动作按钮（打开返回说明文字；读取返回空串） */
  onAction?: (a: ActionView) => Promise<string>;
  /** 保存提醒 */
  onReminder?: (r: Reminder) => Promise<void>;
  /** 小事件选了一项 */
  onChoose?: (msgId: number, id: number, index: number, option: string) => Promise<void>;
}

const Message = memo(function Message({
  msg,
  isLast,
  generating,
  onRegenerate,
  onAction,
  onReminder,
  onChoose,
}: MessageProps) {
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
      {msg.note && <div class="note msg-note">{msg.note}</div>}
      {msg.choices && onChoose && (
        <div class="choices">
          {msg.choices.options.map((o, i) => (
            <button type="button" key={o} class="btn btn-small btn-plain" onClick={() => onChoose(msg.id, msg.choices!.id, i, o)}>
              {o}
            </button>
          ))}
        </div>
      )}
      {(msg.reminder || msg.reminderForm) && onReminder && (
        <ReminderCard draft={msg.reminder} onSave={onReminder} />
      )}
      {msg.actions && onAction && (
        <div class="actions">
          {msg.actions.map((a, i) => (
            <ActionChip key={i} action={a} onAction={onAction} />
          ))}
        </div>
      )}
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
          {isLast && !generating && onRegenerate && (
            <button type="button" class="link-btn" onClick={onRegenerate}>
              {msg.status === 'error' ? '重试' : '重新生成'}
            </button>
          )}
        </div>
      )}
    </div>
  );
});

/** 一个动作按钮：说明要打开 / 读取什么，用户点了才做 */
function ActionChip({ action, onAction }: { action: ActionView; onAction: (a: ActionView) => Promise<string> }) {
  const [state, setState] = useState<{ done: boolean; text: string } | null>(null);
  const icon = action.kind === 'read' ? '📄' : '📂';
  if (action.error) {
    return (
      <div class="action action-blocked" role="note">
        {icon} {action.label}
        <div class="action-note">{action.error}</div>
      </div>
    );
  }
  const run = async () => {
    try {
      await onAction(action);
      setState({ done: true, text: action.kind === 'read' ? '正在读…' : '已打开' });
    } catch (e) {
      setState({ done: false, text: String(e) });
    }
  };
  return (
    <div class="action">
      <span class="action-label">
        {icon} {action.label}
      </span>
      {state?.done ? (
        <span class="action-note">{state.text}</span>
      ) : (
        <button type="button" class="btn btn-small" onClick={run}>
          {action.kind === 'read' ? '读取' : '打开'}
        </button>
      )}
      {state && !state.done && <div class="action-note action-err">{state.text}</div>}
    </div>
  );
}

/** 提醒确认卡片：「好」直接保存，「改一下」打开表单，「不用了」就不设 */
function ReminderCard({ draft, onSave }: { draft?: ReminderDraft; onSave: (r: Reminder) => Promise<void> }) {
  const [phase, setPhase] = useState<'ask' | 'edit' | 'saved' | 'cancelled'>(draft ? 'ask' : 'edit');
  const [error, setError] = useState('');
  const [shown, setShown] = useState(draft ? `${draft.when} · ${draft.reminder.content}` : '');
  const save = async (r: Reminder) => {
    await onSave(r);
    setShown(r.content);
    setPhase('saved');
  };
  if (phase === 'saved') return <div class="reminder-card done">⏰ 已设好{shown ? `：${shown}` : ''}</div>;
  if (phase === 'cancelled') return <div class="reminder-card done muted">（没有设置提醒）</div>;
  if (phase === 'edit')
    return (
      <div class="reminder-card">
        <ReminderForm
          initial={draft?.reminder ?? emptyReminder()}
          onSave={save}
          onCancel={() => setPhase(draft ? 'ask' : 'cancelled')}
        />
      </div>
    );
  return (
    <div class="reminder-card">
      <div class="reminder-what">⏰ {shown}</div>
      {error && <div class="action-note action-err">{error}</div>}
      <div class="reminder-buttons">
        <button
          type="button"
          class="btn btn-small"
          onClick={() => draft && save(draft.reminder).catch((e) => setError(String(e)))}
        >
          好
        </button>
        <button type="button" class="btn btn-small btn-plain" onClick={() => setPhase('edit')}>
          改一下
        </button>
        <button type="button" class="btn btn-small btn-plain" onClick={() => setPhase('cancelled')}>
          不用了
        </button>
      </div>
    </div>
  );
}
