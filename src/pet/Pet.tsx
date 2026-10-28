// 桌宠：显示角色形象和表情状态。
// 鼠标（v2.1 P11）：点身体 = 聊天；点头顶或在头上来回蹭 = 摸头（不弹聊天框）；按住拖动 = 移动（不弹聊天框）；
// 按住不动一会儿、连着戳好几下都有反应；右键按下 = 头顶浮出气泡菜单（按住右键移到气泡上松开也能选）。
// 待机动画用「低频定时器 + 离散姿势」：每隔 8~20 秒切换 2~4 次姿势（呼吸/眨眼），
// 每次切换只渲染一帧，其余时间没有任何动画在跑，空闲 CPU 不上涨（见 DECISIONS #7、#12）。
import { useCallback, useEffect, useRef, useState } from 'preact/hooks';
import type { PetApi } from '../lib/ipc';
import type { PetInit, PetState } from '../lib/types';

const DRAG_THRESHOLD = 4;
/** 点在形象上面这一截算摸头 */
const HEAD_RATIO = 0.3;
/** 按住不动多久算「按住不放」 */
const HOLD_MS = 700;
/** 这么短的时间里点身体第 4 下算「连戳」 */
const POKE_WINDOW_MS = 2500;
/** 在头上来回蹭：这么短的时间里换方向几次 */
const RUB_WINDOW_MS = 1500;
const RUB_TURNS = 4;
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
  const press = useRef<{ x: number; y: number; dragging: boolean; held: boolean } | null>(null);
  const holdTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pokes = useRef<number[]>([]);
  const rub = useRef<{ lastX: number; dir: number; acc: number; turns: number[]; cooldownUntil: number }>({
    lastX: NaN,
    dir: 0,
    acc: 0,
    turns: [],
    cooldownUntil: 0,
  });
  /** 摸头时本地马上缩一下（不用等后端） */
  const [patted, setPatted] = useState(0);

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

  const clearHold = () => {
    if (holdTimer.current) clearTimeout(holdTimer.current);
    holdTimer.current = null;
  };
  useEffect(() => clearHold, []);

  const pat = () => {
    setPatted((n) => n + 1);
  };
  const onHead = (e: PointerEvent) => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    return r.height > 0 && (e.clientY - r.top) / r.height < HEAD_RATIO;
  };

  const onPointerDown = (e: PointerEvent) => {
    if (e.button === 2) {
      // 右键按下就出气泡（按住移到气泡上松开 = 选中；很快松开 = 慢慢点选）
      e.preventDefault();
      api.menuOpen(true).catch(() => {});
      return;
    }
    if (e.button !== 0) return;
    const p = { x: e.screenX, y: e.screenY, dragging: false, held: false };
    press.current = p;
    clearHold();
    holdTimer.current = setTimeout(() => {
      if (press.current === p && !p.dragging) {
        p.held = true;
        api.touch('hold').catch(() => {});
      }
    }, HOLD_MS);
  };
  const onPointerMove = (e: PointerEvent) => {
    const p = press.current;
    if (p && (e.buttons & 1) !== 0) {
      if (!p.dragging && Math.hypot(e.screenX - p.x, e.screenY - p.y) > DRAG_THRESHOLD) {
        p.dragging = true;
        clearHold();
        api.startDrag().catch(() => {});
        api.touch('drag').catch(() => {});
      }
      return;
    }
    if (e.buttons !== 0) return;
    // 没按键、在头上来回蹭 = 摸头
    const r = rub.current;
    if (!onHead(e)) {
      r.lastX = NaN;
      r.dir = 0;
      r.turns = [];
      return;
    }
    const now = e.timeStamp || Date.now();
    if (!Number.isNaN(r.lastX)) {
      const d = e.screenX - r.lastX;
      if (d !== 0) {
        const dir = Math.sign(d);
        if (dir === r.dir) r.acc += Math.abs(d);
        else {
          // 每段至少挪 6 像素才算一次换方向（手抖不算）
          if (r.dir !== 0 && r.acc >= 6) r.turns.push(now);
          r.dir = dir;
          r.acc = Math.abs(d);
        }
        r.turns = r.turns.filter((t) => now - t <= RUB_WINDOW_MS);
        if (r.turns.length >= RUB_TURNS && now >= r.cooldownUntil) {
          r.turns = [];
          r.cooldownUntil = now + 1500;
          pat();
          api.touch('rub').catch(() => {});
        }
      }
    }
    r.lastX = e.screenX;
  };
  const onPointerUp = (e: PointerEvent) => {
    const p = press.current;
    press.current = null;
    clearHold();
    if (e.button !== 0 || !p || p.dragging || p.held) return;
    if (onHead(e)) {
      pat();
      api.touch('head').catch(() => {});
      return;
    }
    // 连着戳身体：第 4 下换成「别戳啦」，这一下不开关聊天
    const now = Date.now();
    pokes.current = [...pokes.current.filter((t) => now - t <= POKE_WINDOW_MS), now];
    if (pokes.current.length >= 4) {
      pokes.current = [];
      api.touch('poke').catch(() => {});
      return;
    }
    api.click().catch(() => {});
  };
  const onPointerLeave = () => {
    rub.current.lastX = NaN;
    rub.current.turns = [];
  };
  // 右键菜单在按下时已经处理了，这里只是不让浏览器弹自己的菜单
  const onContextMenu = (e: MouseEvent) => e.preventDefault();

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
      onPointerLeave={onPointerLeave}
      onContextMenu={onContextMenu}
      title={name ? `${name}\n点身体聊天 · 点头或在头上蹭蹭是摸头 · 按住拖动 · 右键（可以按住）出选项` : undefined}
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
      {src && <img key={`pat-${patted}`} class={`pet-img ${patted ? 'patted' : ''}`} src={src} alt={name} draggable={false} />}
    </div>
  );
}
