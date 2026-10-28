import { describe, expect, it } from 'vitest';
import { aiMove, bestMove, winner, type Board } from './ttt';
import { dealerPlay, isBlackjack, newDeck, score, settle, type Card } from './blackjack';

const c = (rank: string): Card => ({ rank, suit: '♠' });

describe('井字棋', () => {
  it('判胜负', () => {
    expect(winner(['X', 'X', 'X', null, 'O', 'O', null, null, null])).toBe('X');
    expect(winner(['X', 'O', 'X', 'X', 'O', 'O', 'O', 'X', 'X'])).toBe('draw');
    expect(winner(Array(9).fill(null))).toBeNull();
  });
  it('困难：能赢就赢，该堵就堵', () => {
    const win: Board = ['O', 'O', null, 'X', 'X', null, null, null, null];
    expect(bestMove(win, 'O')).toBe(2);
    const block: Board = ['X', 'X', null, null, 'O', null, null, null, null];
    expect(bestMove(block, 'O')).toBe(2);
  });
  it('困难难度自己对自己永远是平局；简单难度会失误', () => {
    let b: Board = Array(9).fill(null);
    let turn: 'X' | 'O' = 'X';
    while (!winner(b)) {
      b = b.slice();
      b[aiMove(b, turn, 'hard')] = turn;
      turn = turn === 'X' ? 'O' : 'X';
    }
    expect(winner(b)).toBe('draw');
    // 随机数总是 0：简单难度走第一个空格（失误），困难不受影响
    const board: Board = ['X', 'X', null, null, 'O', null, null, null, null];
    const zero = () => 0;
    expect(aiMove(['O', 'O', null, 'X', 'X', null, null, null, null], 'O', 'easy', zero)).toBe(2);
    expect(aiMove(board, 'O', 'hard', zero)).toBe(2);
  });
});

describe('21 点', () => {
  it('计分：A 算 1 或 11', () => {
    expect(score([c('A'), c('K')])).toBe(21);
    expect(score([c('A'), c('A'), c('9')])).toBe(21);
    expect(score([c('K'), c('Q'), c('5')])).toBe(25);
    expect(isBlackjack([c('A'), c('Q')])).toBe(true);
    expect(isBlackjack([c('7'), c('7'), c('7')])).toBe(false);
  });
  it('一副牌 52 张不重复', () => {
    const d = newDeck();
    expect(d).toHaveLength(52);
    expect(new Set(d.map((x) => x.rank + x.suit)).size).toBe(52);
  });
  it('庄家到 17 点停，结算', () => {
    const deck = [c('5'), c('9')];
    const h = dealerPlay([c('10'), c('2')], deck);
    expect(score(h)).toBeGreaterThanOrEqual(17);
    expect(settle([c('K'), c('Q'), c('5')], [c('10'), c('7')])).toBe('lose');
    expect(settle([c('K'), c('9')], [c('10'), c('6'), c('K')])).toBe('win');
    expect(settle([c('K'), c('8')], [c('10'), c('8')])).toBe('draw');
    expect(settle([c('A'), c('K')], [c('10'), c('5'), c('6')])).toBe('win');
  });
});
