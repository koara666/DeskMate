// 方块对战（v2.1 P15，DECISIONS #53）：你和她各一块棋盘同时玩。一次消 2 行以上会给对方塞垃圾行；
// 限时 3 分钟比分数，或者生存模式比谁先顶到上面。她是电脑对手（难度决定思考速度和失误率）。
// 原创的同类玩法：8×16 的小棋盘、3~5 格的原创方块组合（见 logic/blocks.ts）。
// 画面用 canvas，只在对局进行时每帧绘制；窗口不可见时浏览器自己会停掉 requestAnimationFrame。
import { useEffect, useRef, useState } from 'preact/hooks';
import type { GameProps } from './registry';
import { ResultBanner } from './Hud';
import {
  AI,
  COLOR,
  COLS,
  ROWS,
  addGarbage,
  cells,
  collides,
  dropY,
  emptyBoard,
  garbageFor,
  lock,
  makeBag,
  move,
  plan,
  rotate,
  scoreFor,
  spawn,
  type Board,
  type Difficulty,
  type Kind,
  type Piece,
  type Plan,
} from './logic/blocks';

type Mode = 'time' | 'survival';
type Phase = 'menu' | 'countdown' | 'play' | 'over';

const MATCH_MS = 3 * 60 * 1000;
const LOCK_DELAY = 350;
const PALETTE = ['', '#7cc6f2', '#ffb36b', '#ffd95a', '#c2a3ff', '#86e0a2', '#ff8fab', '#5fd0c9', '#f59fd0', '#8d99a6'];

interface Side {
  board: Board;
  piece: Piece | null;
  bag: Kind[];
  score: number;
  lines: number;
  combo: number;
  /** 对方塞过来、下一块出来前要加上的垃圾行 */
  pending: number;
  dead: boolean;
  fall: number;
  lockAt: number | null;
  /** 她的计划：什么时候开始动、目标、下一步时间 */
  plan?: Plan | null;
  planAt?: number;
  stepAt?: number;
}

function newSide(rand: () => number): Side {
  const bag = [...makeBag(rand), ...makeBag(rand)];
  return { board: emptyBoard(), piece: null, bag, score: 0, lines: 0, combo: 0, pending: 0, dead: false, fall: 0, lockAt: null };
}

function nextKind(s: Side, rand: () => number): Kind {
  if (s.bag.length < 4) s.bag.push(...makeBag(rand));
  return s.bag.shift()!;
}

function gravityMs(elapsed: number): number {
  return Math.max(180, 800 - Math.floor(elapsed / 30_000) * 70);
}

function drawBoard(cv: HTMLCanvasElement | null, s: Side, ghost: boolean) {
  if (!cv) return;
  const ctx = cv.getContext('2d');
  if (!ctx) return;
  const dpr = window.devicePixelRatio || 1;
  const w = cv.clientWidth;
  const h = cv.clientHeight;
  if (cv.width !== Math.round(w * dpr)) {
    cv.width = Math.round(w * dpr);
    cv.height = Math.round(h * dpr);
  }
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  const c = w / COLS;
  ctx.fillStyle = '#1b2b2d';
  ctx.fillRect(0, 0, w, h);
  ctx.strokeStyle = 'rgba(255,255,255,0.05)';
  ctx.lineWidth = 1;
  for (let x = 1; x < COLS; x++) {
    ctx.beginPath();
    ctx.moveTo(x * c + 0.5, 0);
    ctx.lineTo(x * c + 0.5, h);
    ctx.stroke();
  }
  const cell = (x: number, y: number, v: number, alpha = 1) => {
    if (y < 0) return;
    ctx.globalAlpha = alpha;
    ctx.fillStyle = PALETTE[v];
    ctx.fillRect(x * c + 1, y * c + 1, c - 2, c - 2);
    // 像素风的高光和阴影
    ctx.fillStyle = 'rgba(255,255,255,0.35)';
    ctx.fillRect(x * c + 1, y * c + 1, c - 2, 2);
    ctx.fillStyle = 'rgba(0,0,0,0.25)';
    ctx.fillRect(x * c + 1, y * c + c - 3, c - 2, 2);
    ctx.globalAlpha = 1;
  };
  for (let y = 0; y < ROWS; y++) for (let x = 0; x < COLS; x++) if (s.board[y][x]) cell(x, y, s.board[y][x]);
  const p = s.piece;
  if (p) {
    const pts = cells(p.kind, p.rot);
    if (ghost) {
      const gy = dropY(s.board, p);
      ctx.strokeStyle = 'rgba(255,255,255,0.35)';
      for (const [cx, cy] of pts) ctx.strokeRect((p.x + cx) * c + 2.5, (gy + cy) * c + 2.5, c - 5, c - 5);
    }
    for (const [cx, cy] of pts) cell(p.x + cx, p.y + cy, COLOR[p.kind]);
  }
  // 塞过来还没加上的垃圾：右边一条红色提示
  if (s.pending > 0) {
    ctx.fillStyle = '#ff6b81';
    ctx.fillRect(w - 3, h - s.pending * c, 3, s.pending * c);
  }
  if (s.dead) {
    ctx.fillStyle = 'rgba(0,0,0,0.5)';
    ctx.fillRect(0, 0, w, h);
  }
}

function drawNext(cv: HTMLCanvasElement | null, kind: Kind | undefined) {
  if (!cv) return;
  const ctx = cv.getContext('2d');
  if (!ctx) return;
  const dpr = window.devicePixelRatio || 1;
  const w = cv.clientWidth;
  const h = cv.clientHeight;
  if (cv.width !== Math.round(w * dpr)) {
    cv.width = Math.round(w * dpr);
    cv.height = Math.round(h * dpr);
  }
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, w, h);
  if (!kind) return;
  const pts = cells(kind, 0);
  const c = 10;
  const pw = (Math.max(...pts.map((p) => p[0])) + 1) * c;
  const ph = (Math.max(...pts.map((p) => p[1])) + 1) * c;
  const ox = (w - pw) / 2;
  const oy = (h - ph) / 2;
  ctx.fillStyle = PALETTE[COLOR[kind]];
  for (const [x, y] of pts) ctx.fillRect(ox + x * c + 1, oy + y * c + 1, c - 2, c - 2);
}

export function Blocks({ say, finish, name = '她', random = Math.random }: GameProps & { random?: () => number }) {
  const [phase, setPhase] = useState<Phase>('menu');
  const [mode, setMode] = useState<Mode>('time');
  const [diff, setDiff] = useState<Difficulty>('normal');
  const [hud, setHud] = useState({ me: 0, her: 0, meLines: 0, herLines: 0, left: MATCH_MS, count: 3 });
  const [result, setResult] = useState<'win' | 'lose' | 'draw' | null>(null);
  const meCv = useRef<HTMLCanvasElement>(null);
  const herCv = useRef<HTMLCanvasElement>(null);
  const nextCv = useRef<HTMLCanvasElement>(null);
  const herNextCv = useRef<HTMLCanvasElement>(null);
  const g = useRef<{ me: Side; her: Side; start: number; last: number; said: Record<string, number>; halfSaid: boolean } | null>(null);
  const raf = useRef(0);
  const box = useRef<HTMLDivElement>(null);

  useEffect(() => say('start'), []);
  useEffect(() => () => cancelAnimationFrame(raf.current), []);

  /** 她说话不要太频繁（同一种事件 8 秒一次） */
  const talk = (event: Parameters<GameProps['say']>[0], now: number) => {
    const st = g.current;
    if (!st || now - (st.said[event] ?? -1e9) < 8000) return;
    st.said[event] = now;
    say(event);
  };

  const spawnFor = (s: Side, now: number): boolean => {
    if (s.pending > 0) {
      const r = addGarbage(s.board, s.pending, random);
      s.board = r.board;
      s.pending = 0;
      if (r.topOut) return false;
    }
    const p = spawn(nextKind(s, random));
    if (collides(s.board, p)) return false;
    s.piece = p;
    s.fall = now;
    s.lockAt = null;
    s.plan = undefined;
    s.planAt = now;
    return true;
  };

  /** 放下当前方块：消行、算分、给对方塞垃圾、出下一块 */
  const settle = (s: Side, other: Side, now: number, isHer: boolean) => {
    if (!s.piece) return;
    const r = lock(s.board, s.piece);
    s.board = r.board;
    s.piece = null;
    if (r.topOut) {
      s.dead = true;
      return;
    }
    if (r.cleared > 0) {
      s.combo += 1;
      s.lines += r.cleared;
      s.score += scoreFor(r.cleared) * (s.combo >= 2 ? 2 : 1);
      let send = garbageFor(r.cleared, s.combo);
      // 先抵消自己身上等着的垃圾
      const cancel = Math.min(send, s.pending);
      s.pending -= cancel;
      send -= cancel;
      other.pending = Math.min(ROWS - 2, other.pending + send);
      if (isHer && send >= 2) talk('special', now);
    } else s.combo = 0;
    if (!spawnFor(s, now)) s.dead = true;
  };

  const end = (res: 'win' | 'lose' | 'draw') => {
    cancelAnimationFrame(raf.current);
    setPhase('over');
    setResult(res);
    say(res);
    finish(res);
  };

  const loop = (t: number) => {
    const st = g.current;
    if (!st) return;
    // 窗口被藏起来一段时间再回来：不算这段时间
    const dt = Math.min(250, t - st.last);
    if (t - st.last > 250) st.start += t - st.last - dt;
    st.last = t;
    const elapsed = t - st.start;
    if (elapsed < 3000) {
      setHud((h) => (h.count === 3 - Math.floor(elapsed / 1000) ? h : { ...h, count: 3 - Math.floor(elapsed / 1000) }));
      drawBoard(meCv.current, st.me, false);
      drawBoard(herCv.current, st.her, false);
      raf.current = requestAnimationFrame(loop);
      return;
    }
    const playMs = elapsed - 3000;
    if (!st.me.piece && !st.me.dead) {
      setPhase('play');
      if (!spawnFor(st.me, t)) st.me.dead = true;
      if (!spawnFor(st.her, t)) st.her.dead = true;
    }
    const grav = gravityMs(playMs);
    for (const [s, other, isHer] of [
      [st.me, st.her, false],
      [st.her, st.me, true],
    ] as const) {
      if (s.dead || !s.piece) continue;
      // 她：想好落点后一步一步挪过去
      if (isHer) {
        const cfg = AI[diff];
        if (s.plan === undefined && t - (s.planAt ?? t) >= cfg.think) {
          s.plan = plan(s.board, s.piece.kind, diff, random);
          s.stepAt = t;
        }
        if (s.plan !== undefined && t >= (s.stepAt ?? 0)) {
          s.stepAt = t + cfg.step;
          const p = s.piece;
          if (!s.plan) {
            s.piece = { ...p, y: dropY(s.board, p) };
            settle(s, other, t, true);
            continue;
          }
          if (p.rot !== s.plan.rot) s.piece = rotate(s.board, p);
          else if (p.x < s.plan.x) s.piece = move(s.board, p, 1, 0) ?? p;
          else if (p.x > s.plan.x) s.piece = move(s.board, p, -1, 0) ?? p;
          else {
            s.piece = { ...p, y: dropY(s.board, p) };
            settle(s, other, t, true);
            continue;
          }
          // 转不过去 / 挪不过去（被挡住）：就地放下
          if (s.piece === p) {
            s.piece = { ...p, y: dropY(s.board, p) };
            settle(s, other, t, true);
            continue;
          }
        }
      }
      if (t - s.fall >= grav) {
        s.fall = t;
        const down = move(s.board, s.piece, 0, 1);
        if (down) {
          s.piece = down;
          s.lockAt = null;
        } else if (s.lockAt === null) s.lockAt = t;
      }
      if (s.lockAt !== null && t - s.lockAt >= LOCK_DELAY && !move(s.board, s.piece, 0, 1)) settle(s, other, t, isHer);
    }
    drawBoard(meCv.current, st.me, true);
    drawBoard(herCv.current, st.her, false);
    drawNext(nextCv.current, st.me.bag[0]);
    drawNext(herNextCv.current, st.her.bag[0]);
    const left = Math.max(0, MATCH_MS - playMs);
    setHud((h) => {
      const n = {
        ...h,
        me: st.me.score,
        her: st.her.score,
        meLines: st.me.lines,
        herLines: st.her.lines,
        left: mode === 'time' ? Math.ceil(left / 1000) * 1000 : playMs,
      };
      return n.me === h.me && n.her === h.her && n.meLines === h.meLines && n.herLines === h.herLines && Math.floor(n.left / 1000) === Math.floor(h.left / 1000) ? h : n;
    });
    // 过半时说一句领先 / 落后
    if (!st.halfSaid && mode === 'time' && playMs > MATCH_MS / 2) {
      st.halfSaid = true;
      // 事件是从你的角度说的：lead = 你领先
      if (st.me.score !== st.her.score) say(st.me.score > st.her.score ? 'lead' : 'behind');
    }
    if (st.me.dead || st.her.dead) {
      end(st.me.dead && st.her.dead ? 'draw' : st.me.dead ? 'lose' : 'win');
      return;
    }
    if (mode === 'time' && left <= 0) {
      end(st.me.score > st.her.score ? 'win' : st.me.score < st.her.score ? 'lose' : 'draw');
      return;
    }
    raf.current = requestAnimationFrame(loop);
  };

  const start = () => {
    cancelAnimationFrame(raf.current);
    const now = performance.now();
    g.current = { me: newSide(random), her: newSide(random), start: now, last: now, said: {}, halfSaid: false };
    setResult(null);
    setHud({ me: 0, her: 0, meLines: 0, herLines: 0, left: MATCH_MS, count: 3 });
    setPhase('countdown');
    raf.current = requestAnimationFrame(loop);
    setTimeout(() => box.current?.focus(), 0);
  };

  // —— 你的操作 ——
  const act = (a: 'left' | 'right' | 'rotate' | 'ccw' | 'down' | 'drop') => {
    const st = g.current;
    if (phase !== 'play' || !st || !st.me.piece || st.me.dead) return;
    const s = st.me;
    const p = s.piece!;
    const now = performance.now();
    if (a === 'left') s.piece = move(s.board, p, -1, 0) ?? p;
    else if (a === 'right') s.piece = move(s.board, p, 1, 0) ?? p;
    else if (a === 'rotate') s.piece = rotate(s.board, p, 1);
    else if (a === 'ccw') s.piece = rotate(s.board, p, -1);
    else if (a === 'down') {
      const d = move(s.board, p, 0, 1);
      if (d) {
        s.piece = d;
        s.fall = now;
        s.score += 1;
      }
    } else {
      const y = dropY(s.board, p);
      s.score += (y - p.y) * 2;
      s.piece = { ...p, y };
      settle(s, st.her, now, false);
    }
    // 挪动后如果脚下空了，取消正在等的锁定
    if (s.piece && move(s.board, s.piece, 0, 1)) s.lockAt = null;
  };
  const onKey = (e: KeyboardEvent) => {
    const map: Record<string, Parameters<typeof act>[0]> = {
      ArrowLeft: 'left',
      ArrowRight: 'right',
      ArrowUp: 'rotate',
      x: 'rotate',
      X: 'rotate',
      z: 'ccw',
      Z: 'ccw',
      ArrowDown: 'down',
      ' ': 'drop',
    };
    const a = map[e.key];
    if (!a) return;
    e.preventDefault();
    act(a);
  };

  const mmss = (ms: number) => {
    const s = Math.floor(ms / 1000);
    return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
  };

  if (phase === 'menu') {
    return (
      <div class="game blocks blocks-menu">
        <div class="blocks-title">
          <b>方块对战</b>
          <small>你和{name}同时玩：一次消 2 行以上，会给对方塞垃圾行</small>
        </div>
        <div class="seg ttt-diff" role="radiogroup" aria-label="模式">
          {(
            [
              ['time', '限时 3 分钟比分数'],
              ['survival', '生存：先顶到上面的输'],
            ] as const
          ).map(([id, label]) => (
            <button type="button" key={id} role="radio" aria-checked={mode === id} class={mode === id ? 'on' : ''} onClick={() => setMode(id)}>
              {label}
            </button>
          ))}
        </div>
        <div class="seg ttt-diff" role="radiogroup" aria-label="她的水平">
          {(
            [
              ['easy', '简单'],
              ['normal', '普通'],
              ['hard', '困难'],
            ] as const
          ).map(([id, label]) => (
            <button type="button" key={id} role="radio" aria-checked={diff === id} class={diff === id ? 'on' : ''} onClick={() => setDiff(id)}>
              {label}
            </button>
          ))}
        </div>
        <p class="muted small blocks-keys">← → 移动 · ↑ / X 旋转 · Z 反转 · ↓ 加速 · 空格 直接落下</p>
        <button type="button" class="btn btn-small blocks-start" onClick={start}>
          PRESS START
        </button>
      </div>
    );
  }

  return (
    <div class="game blocks" ref={box} tabIndex={0} onKeyDown={onKey} aria-label="方块对战，用方向键和空格操作">
      <div class="blocks-arena">
        <div class="blocks-side">
          <div class="blocks-name me">YOU</div>
          <canvas ref={meCv} class="blocks-board" width={112} height={224} aria-label="你的棋盘" />
          <div class="blocks-pad">
            <button type="button" aria-label="左移" onClick={() => act('left')}>
              ◀
            </button>
            <button type="button" aria-label="旋转" onClick={() => act('rotate')}>
              ⟳
            </button>
            <button type="button" aria-label="右移" onClick={() => act('right')}>
              ▶
            </button>
            <button type="button" aria-label="落下" onClick={() => act('drop')}>
              ⤓
            </button>
          </div>
        </div>
        <div class="blocks-mid">
          <div class="blocks-time" title={mode === 'time' ? '剩下的时间' : '已经玩了'}>
            {mmss(hud.left)}
          </div>
          <div class="blocks-stat">
            <small>NEXT</small>
            <canvas ref={nextCv} class="blocks-next" width={48} height={36} />
          </div>
          <div class="blocks-stat">
            <small>SCORE</small>
            <b>{hud.me}</b>
            <small>{hud.meLines} 行</small>
          </div>
          <div class="blocks-vs">VS</div>
          <div class="blocks-stat">
            <small>SCORE</small>
            <b>{hud.her}</b>
            <small>{hud.herLines} 行</small>
          </div>
          <div class="blocks-stat">
            <small>NEXT</small>
            <canvas ref={herNextCv} class="blocks-next" width={48} height={36} />
          </div>
        </div>
        <div class="blocks-side">
          <div class="blocks-name her">{name}</div>
          <canvas ref={herCv} class="blocks-board" width={112} height={224} aria-label={`${name}的棋盘`} />
        </div>
        {phase === 'countdown' && <div class="blocks-count">{Math.max(1, hud.count)}</div>}
      </div>
      {phase === 'over' && result && (
        <div class="blocks-over">
          <ResultBanner result={result} />
          <button type="button" class="btn btn-small" onClick={start}>
            再来一局
          </button>
          <button type="button" class="btn btn-small btn-plain" onClick={() => setPhase('menu')}>
            换模式
          </button>
        </div>
      )}
    </div>
  );
}

