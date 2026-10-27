// 桌宠：显示角色形象和表情状态；左键点击开关聊天、拖动移动、右键菜单。
// 待机动画用「低频定时器 + 一次性 CSS 动画」：每隔 5~11 秒播一次约 1 秒的呼吸/眨眼，
// 其余时间没有任何动画在跑，空闲 CPU 不上涨（见 DECISIONS #7）。
import { useCallback, useEffect, useRef, useState } from 'preact/hooks';
import type { PetApi } from '../lib/ipc';
import type { PetInit, PetState } from '../lib/types';

const DRAG_THRESHOLD = 4;
const REVERT_MS: Partial<Record<PetState, number>> = { happy: 2500, sad: 4000 };

interface Props {
  api: PetApi;
  random?: () => number;
}

export function Pet({ api, random = Math.random }: Props) {
  const [init, setInit] = useState<PetInit | null>(null);
  const [state, setState] = useState<PetState>('idle');
  const [fx, setFx] = useState<'' | 'breathe' | 'blink'>('');
  const revert = useRef<ReturnType<typeof setTimeout> | null>(null);
  const press = useRef<{ x: number; y: number; dragging: boolean } | null>(null);

  const load = useCallback(() => {
    api.init().then(setInit).catch(() => {});
  }, [api]);

  useEffect(() => {
    load();
    const offState = api.onState((s) => {
      if (revert.current) clearTimeout(revert.current);
      setState(s);
      const ms = REVERT_MS[s];
      if (ms) revert.current = setTimeout(() => setState('idle'), ms);
    });
    const offChanged = api.onChanged(load);
    return () => {
      offState.then((f) => f()).catch(() => {});
      offChanged.then((fs) => fs.forEach((f) => f())).catch(() => {});
      if (revert.current) clearTimeout(revert.current);
    };
  }, [api, load]);

  // 低频待机动画
  const animate = init?.idle_animation ?? false;
  useEffect(() => {
    if (!animate || state !== 'idle') return;
    let t1: ReturnType<typeof setTimeout> | undefined;
    let t2: ReturnType<typeof setTimeout> | undefined;
    const schedule = () => {
      t1 = setTimeout(() => {
        if (!document.hidden) {
          setFx(random() < 0.5 ? 'breathe' : 'blink');
          t2 = setTimeout(() => setFx(''), 1200);
        }
        schedule();
      }, 5000 + random() * 6000);
    };
    schedule();
    return () => {
      clearTimeout(t1);
      clearTimeout(t2);
      setFx('');
    };
  }, [animate, state, random]);

  const onPointerDown = (e: PointerEvent) => {
    if (e.button !== 0) return;
    press.current = { x: e.screenX, y: e.screenY, dragging: false };
  };
  const onPointerMove = (e: PointerEvent) => {
    const p = press.current;
    if (!p || p.dragging || (e.buttons & 1) === 0) return;
    if (Math.hypot(e.screenX - p.x, e.screenY - p.y) > DRAG_THRESHOLD) {
      p.dragging = true;
      api.startDrag().catch(() => {});
    }
  };
  const onPointerUp = (e: PointerEvent) => {
    const p = press.current;
    press.current = null;
    if (e.button === 0 && p && !p.dragging) api.click().catch(() => {});
  };
  const onContextMenu = (e: MouseEvent) => {
    e.preventDefault();
    api.menu().catch(() => {});
  };

  const images = init?.appearance?.images ?? {};
  const src = images[state] ?? images.idle;
  const placeholder = init?.appearance?.placeholder ?? true;
  const name = init?.character.name ?? '';

  return (
    <div
      class={`pet state-${state} ${placeholder ? 'placeholder' : ''} ${fx ? `fx-${fx}` : ''}`}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onContextMenu={onContextMenu}
      title={name}
      data-testid="pet"
    >
      {state === 'thinking' && (
        <div class="pet-thinking" aria-label="思考中">
          <span />
          <span />
          <span />
        </div>
      )}
      {state === 'sleep' && <div class="pet-zzz">z z</div>}
      {init?.dnd && <div class="pet-dnd" title="勿扰模式" />}
      {src && <img class="pet-img" src={src} alt={name} draggable={false} />}
    </div>
  );
}
