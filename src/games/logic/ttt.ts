// 井字棋：判胜负 + 电脑对手（极小极大算法；按难度掺入随机，不会总赢也不会总输）
export type Cell = 'X' | 'O' | null;
export type Board = Cell[];
export type Difficulty = 'easy' | 'normal' | 'hard';

const LINES = [
  [0, 1, 2],
  [3, 4, 5],
  [6, 7, 8],
  [0, 3, 6],
  [1, 4, 7],
  [2, 5, 8],
  [0, 4, 8],
  [2, 4, 6],
];

export function winner(b: Board): 'X' | 'O' | 'draw' | null {
  for (const [a, c, d] of LINES) {
    if (b[a] && b[a] === b[c] && b[a] === b[d]) return b[a];
  }
  return b.every((x) => x) ? 'draw' : null;
}

export function winningLine(b: Board): number[] | null {
  return LINES.find(([a, c, d]) => b[a] && b[a] === b[c] && b[a] === b[d]) ?? null;
}

const empty = (b: Board) => b.map((x, i) => (x ? -1 : i)).filter((i) => i >= 0);

/** 从 `me` 的角度打分：赢 +10，输 -10，越快越好 */
function minimax(b: Board, turn: 'X' | 'O', me: 'X' | 'O', depth: number): number {
  const w = winner(b);
  if (w === me) return 10 - depth;
  if (w && w !== 'draw') return depth - 10;
  if (w === 'draw') return 0;
  const scores = empty(b).map((i) => {
    const n = b.slice();
    n[i] = turn;
    return minimax(n, turn === 'X' ? 'O' : 'X', me, depth + 1);
  });
  return turn === me ? Math.max(...scores) : Math.min(...scores);
}

export function bestMove(b: Board, me: 'X' | 'O'): number {
  const other = me === 'X' ? 'O' : 'X';
  let best = -Infinity;
  let pick = empty(b)[0];
  for (const i of empty(b)) {
    const n = b.slice();
    n[i] = me;
    const s = minimax(n, other, me, 0);
    if (s > best) {
      best = s;
      pick = i;
    }
  }
  return pick;
}

/** 失误率：简单 70%、普通 25%、困难 0%（困难下她不会输，你最多打平） */
export const MISTAKE: Record<Difficulty, number> = { easy: 0.7, normal: 0.25, hard: 0 };

export function aiMove(b: Board, me: 'X' | 'O', d: Difficulty, random = Math.random): number {
  const free = empty(b);
  if (random() < MISTAKE[d]) return free[Math.floor(random() * free.length)];
  return bestMove(b, me);
}
