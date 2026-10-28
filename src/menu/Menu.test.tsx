import { afterEach, describe, expect, it } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/preact';
import { Menu, layout } from './Menu';
import type { MenuApi } from '../lib/ipc';
import type { MenuShow } from '../lib/types';

afterEach(cleanup);

function fakeApi() {
  const calls: string[] = [];
  const h: {
    show?: (s: MenuShow) => void;
    pointer?: (p: { x: number; y: number }) => void;
    release?: (r: { x: number; y: number; held_ms: number }) => void;
  } = {};
  const api: MenuApi = {
    onShow: async (cb) => ((h.show = cb), () => {}),
    onPointer: async (cb) => ((h.pointer = cb), () => {}),
    onRelease: async (cb) => ((h.release = cb), () => {}),
    pick: async (id) => {
      calls.push(`pick:${id}`);
    },
    close: async (ms) => {
      calls.push(`close:${ms}`);
    },
    focus: async () => {
      calls.push('focus');
    },
  };
  return { api, calls, h };
}

const items = [
  { id: 'chat', icon: '💬', label: '聊天', on: false },
  { id: 'games', icon: '🎮', label: '一起玩', on: false },
  { id: 'dnd', icon: '🌙', label: '勿扰中', on: true },
];

describe('头顶气泡菜单', () => {
  it('气泡排成一段在窗口里的圆弧', () => {
    for (const n of [3, 8, 11]) {
      for (const below of [false, true]) {
        const ps = layout(n, below);
        expect(ps).toHaveLength(n);
        for (const p of ps) {
          expect(p.x - p.size / 2).toBeGreaterThanOrEqual(0);
          expect(p.x + p.size / 2).toBeLessThanOrEqual(340);
          expect(p.y - p.size / 2).toBeGreaterThanOrEqual(0);
          expect(p.y + p.size / 2 + 18).toBeLessThanOrEqual(210);
        }
        // 从左到右
        expect(ps[0].x).toBeLessThan(ps[n - 1].x);
      }
    }
  });

  it('右键单击：变成点选模式，点气泡选中', async () => {
    const f = fakeApi();
    render(<Menu api={f.api} />);
    await act(async () => f.h.show!({ items, below: false, hold: true }));
    expect(screen.getByTestId('menu-dnd').className).toContain('on');
    // 很快松开、没松在气泡上
    await act(async () => f.h.release!({ x: -10, y: -10, held_ms: 120 }));
    expect(f.calls).toEqual(['focus']);
    fireEvent.click(screen.getByTestId('menu-games'));
    expect(f.calls).toEqual(['focus', 'pick:games']);
  });

  it('按住很久没松在气泡上：收起（告诉后端按了多久）', async () => {
    const f = fakeApi();
    render(<Menu api={f.api} />);
    await act(async () => f.h.show!({ items, below: false, hold: true }));
    await act(async () => f.h.release!({ x: -10, y: -10, held_ms: 5000 }));
    expect(f.calls).toEqual(['close:5000']);
  });
});
