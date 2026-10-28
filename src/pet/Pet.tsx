// 桌宠：显示角色形象和表情状态；左键点击开关聊天、拖动移动、右键菜单。
// 待机动画用「低频定时器 + 离散姿势」：每隔 8~20 秒切换 2~4 次姿势（呼吸/眨眼），
// 每次切换只渲染一帧，其余时间没有任何动画在跑，空闲 CPU 不上涨（见 DECISIONS #7、#12）。
import { useCallback, useEffect, useRef, useState } from 'preact/hooks';
import type { PetApi } from '../lib/ipc';
import type { PetInit, PetState } from '../lib/types';

const DRAG_THRESHOLD = 4;
/** 点在形象上面这一截算摸头 */
const HEAD_RATIO = 0.3;
const REVERT_MS: Partial<Record<PetState, number>> = { happy: 2500, sad: 4000 };

type Pose = '' | 'breathe1' | 'breathe2' | 'blink';

interface Props {
  api: PetApi;
  random?: () => number;
}

export function Pet({ api, random = Math.random }: Props) {
  const [init, setInit] = useState<PetInit | null>(null);
  const [state, setState] = useState<PetState>('idle');
  const [pose, setPose] = useState<Pose>('');
  /** 说话 / 思考时的离散帧（不用无限循环的 CSS 动画） */
  const [frame, setFrame] = useState(0);
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

  // 低频待机动画：几次离散的姿势切换（每次只渲染 1 帧），不用连续的 CSS 动画
  const animate = init?.idle_animation ?? false;
  useEffect(() => {
    if (!animate || state !== 'idle') return;
    const timers: ReturnType<typeof setTimeout>[] = [];
    const play = (poses: Pose[], stepMs: number) => {
      poses.forEach((p, i) => timers.push(setTimeout(() => setPose(p), i * stepMs)));
    };
    const schedule = () => {
      timers.push(
        setTimeout(() => {
          if (!document.hidden) {
            if (random() < 0.5) play(['breathe1', 'breathe2', 'breathe1', ''], 260);
            else play(['blink', ''], 140);
          }
          schedule();
        }, 8000 + random() * 12000),
      );
    };
    schedule();
    return () => {
      timers.forEach(clearTimeout);
      setPose('');
    };
  }, [animate, state, random]);

  // 说话：两帧来回切；思考：三个点轮流亮。窗口不可见时跳过，不渲染
  useEffect(() => {
    if (state !== 'talking' && state !== 'thinking') return;
    const n = state === 'talking' ? 2 : 3;
    let i = 0;
    const t = setInterval(
      () => {
        if (document.hidden) return;
        i = (i + 1) % n;
        setFrame(i);
      },
      state === 'talking' ? 220 : 320,
    );
    return () => {
      clearInterval(t);
      setFrame(0);
    };
  }, [state]);

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
      api.touch('drag').catch(() => {});
    }
  };
  const onPointerUp = (e: PointerEvent) => {
    const p = press.current;
    press.current = null;
    if (e.button !== 0 || !p || p.dragging) return;
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const onHead = r.height > 0 && (e.clientY - r.top) / r.height < HEAD_RATIO;
    if (onHead) api.touch('head').catch(() => {});
    else api.click().catch(() => {});
  };
  const onContextMenu = (e: MouseEvent) => {
    e.preventDefault();
    api.menu().catch(() => {});
  };

  const images = init?.appearance?.images ?? {};
  // 待机时：心情 / 天气 / 夜里 / 季节的图片（用户放了才有），没有就用 idle
  const idleVariant = (init?.variants ?? []).find((v) => images[v]);
  const src = state === 'idle' && idleVariant ? images[idleVariant] : (images[state] ?? images.idle);
  const mood = init?.mood ?? 0;
  const placeholder = init?.appearance?.placeholder ?? true;
  const name = init?.character.name ?? '';

  return (
    <div
      class={`pet state-${state} frame-${frame} ${placeholder ? 'placeholder' : ''} ${pose ? `pose-${pose}` : ''} ${mood >= 30 ? 'mood-good' : mood <= -30 ? 'mood-low' : ''}`}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onContextMenu={onContextMenu}
      title={name ? `${name}\n点我聊天 · 摸摸头 · 右键更多选项` : undefined}
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
