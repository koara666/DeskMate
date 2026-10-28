import { afterEach, describe, expect, it } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/preact';
import { Rps, judge } from './Rps';
import { Ttt } from './Ttt';
import { GameHost } from './GameHost';
import type { GameApi } from '../lib/ipc';

afterEach(cleanup);

function fakeApi() {
  const said: string[] = [];
  const finished: string[] = [];
  const api: GameApi = {
    say: async (_g, e) => {
      said.push(e);
      return `台词:${e}`;
    },
    finish: async (_g, o) => {
      finished.push(o);
    },
    stats: async () => [{ game: 'rps', win: 3, lose: 1, draw: 0, played: 4 }],
    fortuneToday: async () => null,
    fortuneDraw: async () => ({ day: 'd', level: 'great', name: '大吉', color: '樱花粉', item: '热茶', direction: '东', comment: '好日子' }),
    llmStart: async (g) => ({ session: 's1', intro: `开始${g}`, puzzle: g === 'turtle' ? '汤面内容' : null, limit: g === 'twenty' ? 20 : 30 }),
    llmTurn: async (_s, kind, input) =>
      kind === 'idiom'
        ? { line: '接上啦', answer: null, idiom: `${input.slice(-1)}马当先`, ended: null, reveal: null, asked: 2, limit: 0 }
        : { line: '嗯', answer: '是', idiom: null, ended: null, reveal: null, asked: 1, limit: 30 },
    llmGiveUp: async () => ({ line: '揭晓', answer: null, idiom: null, ended: 'lose', reveal: '汤底内容', asked: 1, limit: 30 }),
  };
  return { api, said, finished };
}

const flush = () =>
  act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });

describe('小游戏', () => {
  it('猜拳判定', () => {
    expect(judge('rock', 'scissors')).toBe('win');
    expect(judge('rock', 'paper')).toBe('lose');
    expect(judge('paper', 'paper')).toBe('draw');
  });

  it('猜拳：三局两胜，结束时上报结果', () => {
    const f = fakeApi();
    // random() = 0.5：她总是出剪刀，你出石头连赢两局
    render(<Rps api={f.api} say={(e) => f.said.push(e)} finish={(o) => f.finished.push(o)} random={() => 0.5} />);
    fireEvent.click(screen.getByLabelText('石头'));
    expect(screen.getByText('你 1 : 0 她')).toBeTruthy();
    fireEvent.click(screen.getByLabelText('石头'));
    expect(f.finished).toEqual(['win']);
    expect(f.said).toEqual(['start', 'round_win', 'win']);
    expect(screen.getByRole('button', { name: '再来一局' })).toBeTruthy();
  });

  it('井字棋：困难难度下她会堵你', () => {
    const f = fakeApi();
    render(<Ttt api={f.api} say={(e) => f.said.push(e)} finish={(o) => f.finished.push(o)} random={() => 0.99} />);
    fireEvent.click(screen.getByRole('radio', { name: '困难' }));
    fireEvent.click(screen.getByLabelText('第 1 格'));
    fireEvent.click(screen.getByLabelText('第 2 格'));
    // 第 3 格被她占了（堵住了这一行）
    expect(screen.getByLabelText('第 3 格，她')).toBeTruthy();
  });

  it('游戏列表显示战绩；抽签', async () => {
    const f = fakeApi();
    render(<GameHost api={f.api} name="千秋" onClose={() => {}} />);
    await flush();
    expect(screen.getByText('你赢 3 · 她赢 1 · 平 0')).toBeTruthy();
    fireEvent.click(screen.getByText('今日运势'));
    // 游戏组件是按需加载的：等它出现
    fireEvent.click(await screen.findByRole('button', { name: '抽签' }, { timeout: 3000 }));
    await flush();
    expect(screen.getByText('大吉')).toBeTruthy();
    expect(screen.getByText('好日子')).toBeTruthy();
  });

  it('海龟汤：显示汤面，提问、揭晓汤底；成语接龙显示她接的成语', async () => {
    const { Turtle, Idiom } = await import('./LlmGames');
    const f = fakeApi();
    render(<Turtle api={f.api} say={() => {}} finish={() => {}} />);
    expect(await screen.findByText('汤面内容')).toBeTruthy();
    fireEvent.input(screen.getByLabelText('你的话'), { target: { value: '她是一个人吗' } });
    fireEvent.click(screen.getByRole('button', { name: '提问' }));
    await flush();
    expect(screen.getByText('是 — 嗯')).toBeTruthy();
    expect(screen.getByText('已经问了 1 / 30')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '揭晓' }));
    await flush();
    expect(screen.getByText('汤底内容')).toBeTruthy();
    cleanup();
    render(<Idiom api={f.api} say={() => {}} finish={() => {}} />);
    await screen.findByText('开始idiom');
    fireEvent.input(screen.getByLabelText('你的话'), { target: { value: '一心一意' } });
    fireEvent.click(screen.getByRole('button', { name: '接龙' }));
    await flush();
    expect(screen.getByText('意马当先 — 接上啦')).toBeTruthy();
  });
});
