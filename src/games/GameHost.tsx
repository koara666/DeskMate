// 聊天窗口里的小游戏区：选游戏 → 玩；上面一个气泡显示角色在关键时刻说的话。
import { useEffect, useRef, useState } from 'preact/hooks';
import type { ComponentType } from 'preact';
import type { GameApi } from '../lib/ipc';
import type { GameStats } from '../lib/types';
import { GAMES, type GameInfo, type GameProps } from './registry';
import '../styles/games.css';

interface Props {
  api: GameApi;
  name: string;
  onClose: () => void;
  /** 直接打开这个游戏（聊天里说「来一局井字棋」） */
  initial?: string;
}

export function GameHost({ api, name, onClose, initial }: Props) {
  const [game, setGame] = useState<GameInfo | null>(null);
  const [Comp, setComp] = useState<ComponentType<GameProps> | null>(null);
  const [line, setLine] = useState('');
  const [stats, setStats] = useState<GameStats[]>([]);
  const seq = useRef(0);

  const loadStats = () => api.stats().then(setStats).catch(() => {});
  useEffect(() => {
    loadStats();
  }, []);
  useEffect(() => {
    const g = initial ? GAMES.find((x) => x.id === initial) : undefined;
    if (g) open(g);
  }, [initial]);

  const open = (g: GameInfo) => {
    setGame(g);
    setComp(null);
    setLine('');
    g.load()
      .then((c) => setComp(() => c))
      .catch(() => setLine('这个游戏没加载出来……'));
  };

  const props: GameProps | null = game
    ? {
        api,
        name,
        say: (event, vars) => {
          const n = ++seq.current;
          api
            .say(game.id, event, game.name, vars)
            .then((t) => {
              if (t && n === seq.current) setLine(t);
            })
            .catch(() => {});
        },
        finish: (outcome) => {
          api
            .finish(game.id, outcome)
            .then(loadStats)
            .catch(() => {});
        },
      }
    : null;

  const st = (id: string) => stats.find((s) => s.game === id);

  return (
    <div class="game-host">
      <div class="game-top">
        {game ? (
          <button type="button" class="link-btn" onClick={() => setGame(null)}>
            ‹ 换个游戏
          </button>
        ) : (
          <span class="game-title">和{name}玩点什么？</span>
        )}
        <button type="button" class="link-btn" onClick={onClose}>
          回到聊天
        </button>
      </div>
      {game && line && (
        <div class="game-line" role="status">
          {line}
        </div>
      )}
      {!game && (
        <ul class="game-list">
          {GAMES.map((g) => {
            const s = st(g.id);
            return (
              <li key={g.id}>
                <button type="button" class="game-pick" onClick={() => open(g)}>
                  <b>{g.name}</b>
                  <small>
                    {g.desc}
                    {g.needsModel ? ' · 需要模型' : ''}
                  </small>
                  {s && s.played > 0 && (
                    <small class="game-stat">
                      {g.id === 'fortune' ? `抽过 ${s.played} 次` : `你赢 ${s.win} · 她赢 ${s.lose} · 平 ${s.draw}`}
                    </small>
                  )}
                </button>
              </li>
            );
          })}
        </ul>
      )}
      {game && props && (Comp ? <Comp key={game.id} {...props} /> : <div class="muted">加载中…</div>)}
    </div>
  );
}
