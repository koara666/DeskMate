import { describe, expect, it } from 'vitest';
import {
  COLS,
  GARBAGE,
  KINDS,
  ROWS,
  addGarbage,
  cells,
  collides,
  dropY,
  emptyBoard,
  garbageFor,
  lock,
  makeBag,
  AI,
  plan,
  rotate,
  secondsPerPiece,
  spawn,
  type Board,
} from './blocks';

function seq(...xs: number[]) {
  let i = 0;
  return () => xs[i++ % xs.length];
}

describe('方块对战逻辑', () => {
  it('每种方块转四次回到原样，格子数不变', () => {
    for (const k of KINDS) {
      const c0 = JSON.stringify(cells(k, 0).sort());
      expect(JSON.stringify(cells(k, 4).sort())).toBe(c0);
      for (let r = 0; r < 4; r++) expect(cells(k, r)).toHaveLength(cells(k, 0).length);
    }
    expect(cells('i3', 1).map((p) => p[0])).toEqual([0, 0, 0]);
  });

  it('出生在顶上中间、能落到底；靠墙旋转会自动挪开', () => {
    const b = emptyBoard();
    const p = spawn('i4');
    expect(collides(b, p)).toBe(false);
    expect(dropY(b, p)).toBe(ROWS - 1);
    const wall = { ...rotate(b, p), x: COLS - 1 };
    const r = rotate(b, wall);
    expect(collides(b, r)).toBe(false);
  });

  it('填满一行会消掉，上面的往下掉', () => {
    let b: Board = emptyBoard();
    b[ROWS - 1] = Array(COLS).fill(1);
    b[ROWS - 1][0] = 0;
    b[ROWS - 2][3] = 2;
    const p = { kind: 'i3' as const, rot: 1, x: 0, y: 0 };
    const r = lock(b, { ...p, y: dropY(b, p) });
    expect(r.cleared).toBe(1);
    expect(r.board[ROWS - 1][3]).toBe(2);
    expect(r.board).toHaveLength(ROWS);
    b = r.board;
    expect(b.every((row) => row.length === COLS)).toBe(true);
  });

  it('消 2 行以上给对方塞垃圾行；连消再多一行；顶上有东西被挤出去 = 输', () => {
    expect(garbageFor(1, 0)).toBe(0);
    expect(garbageFor(2, 0)).toBe(1);
    expect(garbageFor(4, 0)).toBe(3);
    expect(garbageFor(2, 2)).toBe(2);
    const g = addGarbage(emptyBoard(), 2, seq(0.5));
    expect(g.topOut).toBe(false);
    expect(g.board[ROWS - 1].filter((c) => c === GARBAGE)).toHaveLength(COLS - 1);
    const full = emptyBoard();
    full[0][0] = 1;
    expect(addGarbage(full, 1, seq(0)).topOut).toBe(true);
  });

  it('公平：她放一块的速度在人手能做到的范围（困难也不快过 1 秒），三档逐级变快', () => {
    // 玩家同样有「直接落下」。她想好以后一步步挪，所以一块的时间 = 想的时间 + 挪转的步数 × 间隔
    for (const d of ['easy', 'normal', 'hard'] as const) {
      const cfg = AI[d];
      expect(cfg.think + cfg.step * 3, `${d} 最快的一块`).toBeGreaterThanOrEqual(750);
      expect(cfg.step, `${d} 每一步`).toBeGreaterThanOrEqual(100);
    }
    expect(secondsPerPiece('easy')).toBeGreaterThan(secondsPerPiece('normal'));
    expect(secondsPerPiece('normal')).toBeGreaterThan(secondsPerPiece('hard'));
    expect(secondsPerPiece('hard')).toBeGreaterThanOrEqual(1);
  });

  it('一袋里每种方块各一个', () => {
    expect(makeBag(Math.random).slice().sort()).toEqual(KINDS.slice().sort());
  });

  it('她会挑能消行的位置', () => {
    const b = emptyBoard();
    // 最底下一行只差右边两格
    b[ROWS - 1] = [1, 1, 1, 1, 1, 1, 0, 0];
    const p = plan(b, 'o4', 'hard', seq(0.99));
    expect(p).not.toBeNull();
    expect(p!.x).toBe(6);
  });

  it('困难的她能连着放 150 块不顶到上面，还会消不少行', () => {
    let seed = 7;
    const rand = () => {
      seed = (seed * 1103515245 + 12345) % 2147483648;
      return seed / 2147483648;
    };
    let b = emptyBoard();
    let lines = 0;
    let bag = makeBag(rand);
    for (let i = 0; i < 150; i++) {
      if (bag.length === 0) bag = makeBag(rand);
      const kind = bag.shift()!;
      const p = plan(b, kind, 'hard', rand);
      expect(p, `第 ${i} 块没地方放`).not.toBeNull();
      const piece = { kind, rot: p!.rot, x: p!.x, y: 0 };
      const r = lock(b, { ...piece, y: dropY(b, piece) });
      expect(r.topOut).toBe(false);
      b = r.board;
      lines += r.cleared;
    }
    expect(lines).toBeGreaterThan(30);
  });
});
