import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/preact';
import { Pet } from './Pet';
import type { PetApi } from '../lib/ipc';
import type { PetState } from '../lib/types';

function fakeApi() {
  let stateCb: ((s: PetState) => void) | null = null;
  const api: PetApi & {
    click: ReturnType<typeof vi.fn>;
    touch: ReturnType<typeof vi.fn>;
    menu: ReturnType<typeof vi.fn>;
    menuOpen: ReturnType<typeof vi.fn>;
    startDrag: ReturnType<typeof vi.fn>;
  } = {
    init: async () => ({
      character: { id: 't', name: '测试酱', color: '' },
      appearance: { images: { idle: 'data:image/svg+xml;base64,AA==' }, placeholder: true },
      idle_animation: false,
      dnd: false,
    }),
    click: vi.fn(async () => {}),
    touch: vi.fn(async () => {}),
    menu: vi.fn(async () => {}),
    menuOpen: vi.fn(async () => {}),
    startDrag: vi.fn(async () => {}),
    onState: async (cb) => {
      stateCb = cb;
      return () => {};
    },
    onChanged: async () => [],
  };
  return { api, setState: (s: PetState) => act(() => stateCb!(s)) };
}

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

async function mount() {
  const f = fakeApi();
  render(<Pet api={f.api} />);
  await act(async () => {});
  return { ...f, el: screen.getByTestId('pet') };
}

describe('Pet 桌宠', () => {
  it('单击（没有移动）打开聊天', async () => {
    const { api, el } = await mount();
    fireEvent.pointerDown(el, { button: 0, screenX: 100, screenY: 100 });
    fireEvent.pointerUp(el, { button: 0, screenX: 101, screenY: 100 });
    expect(api.click).toHaveBeenCalledTimes(1);
    expect(api.startDrag).not.toHaveBeenCalled();
  });

  it('按住移动超过阈值开始拖动，松手不算点击', async () => {
    const { api, el } = await mount();
    fireEvent.pointerDown(el, { button: 0, screenX: 100, screenY: 100 });
    fireEvent.pointerMove(el, { buttons: 1, screenX: 112, screenY: 100 });
    fireEvent.pointerUp(el, { button: 0, screenX: 112, screenY: 100 });
    expect(api.startDrag).toHaveBeenCalledTimes(1);
    expect(api.click).not.toHaveBeenCalled();
  });

  it('右键按下就出头顶气泡菜单（浏览器自己的菜单不弹）', async () => {
    const { api, el } = await mount();
    fireEvent.pointerDown(el, { button: 2, screenX: 100, screenY: 100 });
    expect(api.menuOpen).toHaveBeenCalledWith(true);
    fireEvent.contextMenu(el);
    expect(api.menu).not.toHaveBeenCalled();
    expect(api.click).not.toHaveBeenCalled();
  });

  it('点头顶是摸头，不打开聊天', async () => {
    const { api, el } = await mount();
    el.getBoundingClientRect = () => ({ top: 0, left: 0, width: 160, height: 180, right: 160, bottom: 180, x: 0, y: 0, toJSON: () => ({}) });
    fireEvent.pointerDown(el, { button: 0, screenX: 80, screenY: 20, clientX: 80, clientY: 20 });
    fireEvent.pointerUp(el, { button: 0, screenX: 80, screenY: 20, clientX: 80, clientY: 20 });
    expect(api.touch).toHaveBeenCalledWith('head');
    expect(api.click).not.toHaveBeenCalled();
  });

  it('在头上来回蹭也是摸头', async () => {
    const { api, el } = await mount();
    el.getBoundingClientRect = () => ({ top: 0, left: 0, width: 160, height: 180, right: 160, bottom: 180, x: 0, y: 0, toJSON: () => ({}) });
    let x = 60;
    for (let i = 0; i < 12; i++) {
      x += i % 2 === 0 ? 20 : -20;
      fireEvent.pointerMove(el, { buttons: 0, screenX: x, screenY: 20, clientX: x, clientY: 20, timeStamp: 1000 + i * 80 });
    }
    expect(api.touch).toHaveBeenCalledWith('rub');
    expect(api.click).not.toHaveBeenCalled();
  });

  it('按住不动一会儿有反应，松手不打开聊天', async () => {
    const { api, el } = await mount();
    fireEvent.pointerDown(el, { button: 0, screenX: 100, screenY: 150 });
    act(() => {
      vi.advanceTimersByTime(800);
    });
    expect(api.touch).toHaveBeenCalledWith('hold');
    fireEvent.pointerUp(el, { button: 0, screenX: 100, screenY: 150 });
    expect(api.click).not.toHaveBeenCalled();
  });

  it('连着戳身体：第 4 下变成「别戳啦」', async () => {
    const { api, el } = await mount();
    for (let i = 0; i < 4; i++) {
      fireEvent.pointerDown(el, { button: 0, screenX: 100, screenY: 150 });
      fireEvent.pointerUp(el, { button: 0, screenX: 100, screenY: 150 });
    }
    expect(api.click).toHaveBeenCalledTimes(3);
    expect(api.touch).toHaveBeenCalledWith('poke');
  });

  it('拖动时不打开聊天', async () => {
    const { api, el } = await mount();
    fireEvent.pointerDown(el, { button: 0, screenX: 100, screenY: 150 });
    fireEvent.pointerMove(el, { buttons: 1, screenX: 130, screenY: 150 });
    fireEvent.pointerUp(el, { button: 0, screenX: 130, screenY: 150 });
    expect(api.touch).toHaveBeenCalledWith('drag');
    expect(api.click).not.toHaveBeenCalled();
  });

  it('开心状态过一会儿自动回到待机', async () => {
    const { setState, el } = await mount();
    setState('happy');
    expect(el.className).toContain('state-happy');
    act(() => {
      vi.advanceTimersByTime(3000);
    });
    expect(el.className).toContain('state-idle');
  });

  it('思考中显示省略号气泡', async () => {
    const { setState } = await mount();
    setState('thinking');
    expect(screen.getByLabelText('思考中')).toBeTruthy();
  });

  it('点上面一截是摸头，不打开聊天；拖动时说一句', async () => {
    const { api, el } = await mount();
    el.getBoundingClientRect = () => ({ top: 0, left: 0, width: 160, height: 180, right: 160, bottom: 180, x: 0, y: 0, toJSON: () => ({}) });
    fireEvent.pointerDown(el, { button: 0, screenX: 100, screenY: 100, clientY: 20 });
    fireEvent.pointerUp(el, { button: 0, screenX: 100, screenY: 100, clientY: 20 });
    expect(api.touch).toHaveBeenCalledWith('head');
    expect(api.click).not.toHaveBeenCalled();
    fireEvent.pointerDown(el, { button: 0, screenX: 100, screenY: 100, clientY: 120 });
    fireEvent.pointerUp(el, { button: 0, screenX: 100, screenY: 100, clientY: 120 });
    expect(api.click).toHaveBeenCalledTimes(1);
    fireEvent.pointerDown(el, { button: 0, screenX: 100, screenY: 100 });
    fireEvent.pointerMove(el, { buttons: 1, screenX: 130, screenY: 100 });
    expect(api.touch).toHaveBeenCalledWith('drag');
  });

  it('说话时用离散的帧切换（定时器），不是无限 CSS 动画；停下来就回到第 0 帧', async () => {
    const f = await mount();
    f.setState('talking');
    expect(f.el.className).toContain('frame-0');
    act(() => {
      vi.advanceTimersByTime(230);
    });
    expect(f.el.className).toContain('frame-1');
    f.setState('idle');
    expect(f.el.className).toContain('frame-0');
  });
});
