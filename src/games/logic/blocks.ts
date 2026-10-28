// 方块对战的纯逻辑（v2.1 P15，DECISIONS #53）：棋盘、方块、旋转、消行、垃圾行、她的电脑对手。
// 这是同类玩法的原创小游戏，不是哪个具体游戏的复刻：8 列 × 16 行的小棋盘，方块是 3~5 格的一组原创组合，
// 对战靠「一次消掉 2 行以上就给对方塞垃圾行」。
export const COLS = 8;
export const ROWS = 16;

/** 0 = 空，1~8 = 方块颜色，9 = 垃圾行 */
export type Cell = number;
export type Board = Cell[][];
export type Kind = 'i3' | 'v3' | 'o4' | 't4' | 's4' | 'l4' | 'i4' | 'p5';
export const KINDS: Kind[] = ['i3', 'v3', 'o4', 't4', 's4', 'l4', 'i4', 'p5'];
export const GARBAGE = 9;

/** 每种方块的格子（第 0 个朝向），坐标 [x, y] */
const SHAPES: Record<Kind, [number, number][]> = {
  i3: [
    [0, 1],
    [1, 1],
    [2, 1],
  ],
  v3: [
    [0, 0],
    [0, 1],
    [1, 1],
  ],
  o4: [
    [0, 0],
    [1, 0],
    [0, 1],
    [1, 1],
  ],
  t4: [
    [0, 1],
    [1, 1],
    [2, 1],
    [1, 0],
  ],
  s4: [
    [1, 0],
    [2, 0],
    [0, 1],
    [1, 1],
  ],
  l4: [
    [0, 0],
    [0, 1],
    [1, 1],
    [2, 1],
  ],
  i4: [
    [0, 1],
    [1, 1],
    [2, 1],
    [3, 1],
  ],
  p5: [
    [0, 0],
    [1, 0],
    [0, 1],
    [1, 1],
    [0, 2],
  ],
};

export interface Piece {
  kind: Kind;
  rot: number;
  x: number;
  y: number;
}

export function emptyBoard(): Board {
  return Array.from({ length: ROWS }, () => Array<Cell>(COLS).fill(0));
}

/** 朝向 rot 时的格子（绕着方块自己的小方框顺时针转） */
export function cells(kind: Kind, rot: number): [number, number][] {
  let pts = SHAPES[kind].map(([x, y]) => [x, y] as [number, number]);
  const r = ((rot % 4) + 4) % 4;
  for (let i = 0; i < r; i++) {
    const size = Math.max(...pts.map(([x, y]) => Math.max(x, y))) + 1;
    pts = pts.map(([x, y]) => [size - 1 - y, x] as [number, number]);
  }
  // 归一化到左上角
  const mx = Math.min(...pts.map((p) => p[0]));
  const my = Math.min(...pts.map((p) => p[1]));
  return pts.map(([x, y]) => [x - mx, y - my] as [number, number]);
}

export function width(kind: Kind, rot: number): number {
  return Math.max(...cells(kind, rot).map((p) => p[0])) + 1;
}

export function collides(b: Board, p: Piece): boolean {
  return cells(p.kind, p.rot).some(([cx, cy]) => {
    const x = p.x + cx;
    const y = p.y + cy;
    return x < 0 || x >= COLS || y >= ROWS || (y >= 0 && b[y][x] !== 0);
  });
}

export function spawn(kind: Kind): Piece {
  return { kind, rot: 0, x: Math.floor((COLS - width(kind, 0)) / 2), y: 0 };
}

/** 旋转（撞墙时试着左右挪一挪） */
export function rotate(b: Board, p: Piece, dir = 1): Piece {
  const r = { ...p, rot: (p.rot + dir + 4) % 4 };
  for (const dx of [0, -1, 1, -2, 2]) {
    const t = { ...r, x: r.x + dx };
    if (!collides(b, t)) return t;
  }
  return p;
}

export function move(b: Board, p: Piece, dx: number, dy: number): Piece | null {
  const t = { ...p, x: p.x + dx, y: p.y + dy };
  return collides(b, t) ? null : t;
}

export function dropY(b: Board, p: Piece): number {
  let y = p.y;
  while (!collides(b, { ...p, y: y + 1 })) y++;
  return y;
}

export const COLOR: Record<Kind, number> = { i3: 1, v3: 2, o4: 3, t4: 4, s4: 5, l4: 6, i4: 7, p5: 8 };

/** 放下来：返回新棋盘、消了几行、是不是顶到上面了 */
export function lock(b: Board, p: Piece): { board: Board; cleared: number; topOut: boolean } {
  const nb = b.map((r) => r.slice());
  let topOut = false;
  for (const [cx, cy] of cells(p.kind, p.rot)) {
    const y = p.y + cy;
    if (y < 0) topOut = true;
    else nb[y][p.x + cx] = COLOR[p.kind];
  }
  const kept = nb.filter((r) => r.some((c) => c === 0));
  const cleared = ROWS - kept.length;
  while (kept.length < ROWS) kept.unshift(Array<Cell>(COLS).fill(0));
  return { board: kept, cleared, topOut };
}

/** 一次消几行给对方塞几行垃圾 */
export function garbageFor(cleared: number, combo: number): number {
  const base = cleared >= 4 ? 3 : cleared === 3 ? 2 : cleared === 2 ? 1 : 0;
  return base + (cleared > 0 && combo >= 2 ? 1 : 0);
}

/** 从底下塞进 n 行垃圾（每行一个缺口）；顶上被挤出去的有方块 = 顶到上面了 */
export function addGarbage(b: Board, n: number, rand: () => number): { board: Board; topOut: boolean } {
  if (n <= 0) return { board: b, topOut: false };
  const gap = Math.floor(rand() * COLS);
  const top = b.slice(0, n);
  const topOut = top.some((r) => r.some((c) => c !== 0));
  const rest = b.slice(n).map((r) => r.slice());
  for (let i = 0; i < n; i++) rest.push(Array.from({ length: COLS }, (_, x) => (x === gap ? 0 : GARBAGE)));
  return { board: rest, topOut };
}

/** 一袋 8 个方块打乱了按顺序发（不会连着很久不来某一种） */
export function makeBag(rand: () => number): Kind[] {
  const bag = KINDS.slice();
  for (let i = bag.length - 1; i > 0; i--) {
    const j = Math.floor(rand() * (i + 1));
    [bag[i], bag[j]] = [bag[j], bag[i]];
  }
  return bag;
}

export function scoreFor(cleared: number): number {
  return [0, 100, 300, 600, 1000, 1500][Math.min(cleared, 5)];
}

// ---------------- 她的电脑对手 ----------------

export type Difficulty = 'easy' | 'normal' | 'hard';

/** 难度：每块想多久（毫秒）、每一步操作的间隔、多大概率不选最好的那个 */
export const AI: Record<Difficulty, { think: number; step: number; mistake: number }> = {
  easy: { think: 900, step: 160, mistake: 0.3 },
  normal: { think: 520, step: 100, mistake: 0.12 },
  hard: { think: 260, step: 55, mistake: 0.03 },
};

/** 给一个棋盘打分（越大越好）：越平、越矮、洞越少越好 */
export function evaluate(b: Board, cleared: number): number {
  const heights: number[] = [];
  let holes = 0;
  for (let x = 0; x < COLS; x++) {
    let h = 0;
    let seen = false;
    for (let y = 0; y < ROWS; y++) {
      if (b[y][x] !== 0) {
        if (!seen) h = ROWS - y;
        seen = true;
      } else if (seen) holes++;
    }
    heights.push(h);
  }
  const agg = heights.reduce((a, c) => a + c, 0);
  let bump = 0;
  for (let x = 1; x < COLS; x++) bump += Math.abs(heights[x] - heights[x - 1]);
  const maxH = Math.max(...heights);
  return cleared * 7.6 - agg * 0.51 - holes * 3.6 - bump * 0.18 - (maxH > ROWS - 4 ? 20 : 0);
}

export interface Plan {
  rot: number;
  x: number;
  score: number;
}

/** 所有落点里挑一个（按难度有一定概率挑第二、第三好的） */
export function plan(b: Board, kind: Kind, diff: Difficulty, rand: () => number): Plan | null {
  const plans: Plan[] = [];
  for (let rot = 0; rot < 4; rot++) {
    const w = width(kind, rot);
    for (let x = 0; x <= COLS - w; x++) {
      const p = { kind, rot, x, y: -1 };
      if (collides(b, p)) continue;
      const y = dropY(b, p);
      const r = lock(b, { ...p, y });
      if (r.topOut) continue;
      plans.push({ rot, x, score: evaluate(r.board, r.cleared) });
    }
  }
  if (plans.length === 0) return null;
  plans.sort((a, c) => c.score - a.score);
  if (plans.length > 1 && rand() < AI[diff].mistake) {
    return plans[Math.min(plans.length - 1, 1 + Math.floor(rand() * 3))];
  }
  return plans[0];
}
