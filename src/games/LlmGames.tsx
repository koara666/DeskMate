// 需要模型的小游戏：成语接龙、海龟汤、二十问。她当主持 / 对手；规则检查和谜底都在后端。
import { useEffect, useRef, useState } from 'preact/hooks';
import type { GameApi } from '../lib/ipc';
import type { LlmGameStart, LlmGameTurn } from '../lib/types';
import type { GameProps } from './registry';

interface Entry {
  who: 'me' | 'her';
  text: string;
}

type Kind = 'ask' | 'guess' | 'idiom';

interface Shell {
  game: 'idiom' | 'turtle' | 'twenty';
  api: GameApi;
  /** 输入框旁边的按钮：（种类，文字） */
  actions: [Kind, string][];
  placeholder: string;
}

function LlmGame({ game, api, actions, placeholder }: Shell) {
  const [start, setStart] = useState<LlmGameStart | null>(null);
  const [error, setError] = useState('');
  const [log, setLog] = useState<Entry[]>([]);
  const [input, setInput] = useState('');
  const [busy, setBusy] = useState(false);
  const [end, setEnd] = useState<LlmGameTurn | null>(null);
  const [asked, setAsked] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);

  const begin = () => {
    setStart(null);
    setError('');
    setLog([]);
    setEnd(null);
    setAsked(0);
    api
      .llmStart(game)
      .then(setStart)
      .catch((e) => setError(String(e)));
  };
  useEffect(begin, [game]);
  useEffect(() => {
    const el = listRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [log, end]);

  const push = (e: Entry) => setLog((l) => [...l, e]);
  const send = async (kind: Kind) => {
    const text = input.trim();
    if (!start || !text || busy) return;
    setBusy(true);
    setError('');
    try {
      const t = await api.llmTurn(start.session, kind, text);
      push({ who: 'me', text: kind === 'guess' ? `我猜：${text}` : text });
      setInput('');
      const parts = [t.answer, t.idiom, t.line].filter(Boolean);
      if (parts.length) push({ who: 'her', text: parts.join(' — ') });
      setAsked(t.asked);
      if (t.ended) setEnd(t);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const giveUp = async () => {
    if (!start || busy) return;
    try {
      const t = await api.llmGiveUp(start.session);
      if (t.line) push({ who: 'her', text: t.line });
      setEnd(t);
    } catch (e) {
      setError(String(e));
    }
  };

  if (error && !start) {
    return (
      <div class="game llm">
        <p class="muted">{error}</p>
      </div>
    );
  }
  if (!start) return <div class="muted">她在想题目……</div>;
  return (
    <div class="game llm">
      <p class="llm-intro">{start.intro}</p>
      {start.puzzle && <p class="llm-puzzle">{start.puzzle}</p>}
      <div class="llm-log" ref={listRef}>
        {log.map((e, i) => (
          <div key={i} class={`llm-entry ${e.who}`}>
            {e.text}
          </div>
        ))}
        {busy && <div class="llm-entry her muted">……</div>}
      </div>
      {start.limit > 0 && !end && (
        <div class="muted small">
          已经问了 {asked} / {start.limit}
        </div>
      )}
      {error && <div class="llm-error">{error}</div>}
      {end ? (
        <div class="llm-end">
          <b>{end.ended === 'win' ? '你赢了！' : end.ended === 'lose' ? '这局她赢了' : '结束'}</b>
          {end.reveal && <p class="llm-reveal">{end.reveal}</p>}
          <button type="button" class="btn btn-small" onClick={begin}>
            再来一局
          </button>
        </div>
      ) : (
        <>
          <input
            class="llm-input"
            value={input}
            maxLength={100}
            placeholder={placeholder}
            aria-label="你的话"
            onInput={(e) => setInput((e.currentTarget as HTMLInputElement).value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && !e.isComposing && e.keyCode !== 229) send(actions[0][0]);
            }}
          />
          <div class="llm-actions">
            {actions.map(([k, label]) => (
              <button type="button" key={k} class="btn btn-small" disabled={busy || !input.trim()} onClick={() => send(k)}>
                {label}
              </button>
            ))}
            <button type="button" class="btn btn-small btn-plain" disabled={busy} onClick={giveUp}>
              {game === 'idiom' ? '认输' : '揭晓'}
            </button>
          </div>
        </>
      )}
    </div>
  );
}

export const Idiom = ({ api }: GameProps) => (
  <LlmGame game="idiom" api={api} actions={[['idiom', '接龙']]} placeholder="说一个四字成语" />
);

export const Turtle = ({ api }: GameProps) => (
  <LlmGame
    game="turtle"
    api={api}
    actions={[
      ['ask', '提问'],
      ['guess', '说出真相'],
    ]}
    placeholder="问一个能用「是 / 不是」回答的问题"
  />
);

export const Twenty = ({ api }: GameProps) => (
  <LlmGame
    game="twenty"
    api={api}
    actions={[
      ['ask', '提问'],
      ['guess', '猜'],
    ]}
    placeholder="比如：它是活的吗？"
  />
);
