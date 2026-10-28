// 头顶气泡菜单：右键桌宠时她头顶浮出一圈气泡。
// - 按住右键：后端把鼠标位置发过来（menu-pointer），这里自己判断指着哪个气泡；松开（menu-release）在气泡上就选中。
// - 右键单击（很快松开、没松在气泡上）：变成普通的点选模式，窗口拿到焦点；点别处、按 Esc、10 秒不动就收起。
// 只有出现时有一次性的弹出动画，没有常驻动画。
import { useEffect, useRef, useState } from 'preact/hooks';
import type { MenuApi } from '../lib/ipc';
import type { MenuItem, MenuShow } from '../lib/types';
import { applyTheme } from '../lib/theme';

/** 窗口大小（和后端 desktop::MENU_LOGICAL 一致） */
const W = 340;
const H = 210;

/** 气泡排成一段圆弧：头顶上方（或脚下）。返回每个气泡的圆心和直径 */
export function layout(n: number, below: boolean): { x: number; y: number; size: number }[] {
  const r = n > 8 ? 148 : 138;
  const spread = n > 8 ? (170 * Math.PI) / 180 : (160 * Math.PI) / 180;
  const size = Math.max(38, Math.min(50, Math.floor((spread * r) / n) - 6));
  const cx = W / 2;
  const cy = below ? 22 : H - 30;
  const start = (below ? 0 : Math.PI) + (Math.PI - spread) / 2;
  return Array.from({ length: n }, (_, i) => {
    // 头顶：从左到右；脚下：也从左到右（角度反过来走）
    const t = n === 1 ? 0.5 : i / (n - 1);
    const a = below ? Math.PI - (start + t * spread) : start + t * spread;
    return { x: cx + r * Math.cos(a), y: cy + r * Math.sin(a), size };
  });
}

export function Menu({ api }: { api: MenuApi }) {
  const [show, setShow] = useState<MenuShow | null>(null);
  const [hover, setHover] = useState<string | null>(null);
  const [seq, setSeq] = useState(0);
  const mode = useRef<'hold' | 'click' | 'closed'>('closed');
  const focused = useRef(false);
  const idle = useRef<ReturnType<typeof setTimeout> | null>(null);

  const hit = (x: number, y: number): string | null => {
    const el = document.elementFromPoint(x, y)?.closest('[data-id]') as HTMLElement | null;
    return el?.dataset.id ?? null;
  };
  const stopIdle = () => {
    if (idle.current) clearTimeout(idle.current);
    idle.current = null;
  };
  const armIdle = () => {
    stopIdle();
    idle.current = setTimeout(() => close(0), 10_000);
  };
  const close = (heldMs: number) => {
    if (mode.current === 'closed') return;
    mode.current = 'closed';
    stopIdle();
    setHover(null);
    api.close(heldMs).catch(() => {});
  };
  const pick = (id: string) => {
    if (mode.current === 'closed') return;
    mode.current = 'closed';
    stopIdle();
    api.pick(id).catch(() => {});
  };

  useEffect(() => {
    const offs = [
      api.onShow((s) => {
        applyTheme(s.theme);
        setShow(s);
        setHover(null);
        setSeq((n) => n + 1);
        focused.current = false;
        mode.current = s.hold ? 'hold' : 'click';
        if (!s.hold) {
          api.focus().catch(() => {});
          armIdle();
        }
      }),
      api.onPointer(({ x, y }) => {
        if (mode.current === 'hold') setHover(hit(x, y));
      }),
      api.onRelease(({ x, y, held_ms }) => {
        if (mode.current !== 'hold') return;
        const id = hit(x, y);
        if (id) pick(id);
        else if (held_ms < 450) {
          // 右键单击：留着慢慢选
          mode.current = 'click';
          setHover(null);
          api.focus().catch(() => {});
          armIdle();
        } else close(held_ms);
      }),
    ];
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') close(0);
    };
    const onFocus = () => {
      focused.current = true;
    };
    const onBlur = () => {
      if (mode.current === 'click' && focused.current) close(0);
    };
    const noMenu = (e: Event) => e.preventDefault();
    window.addEventListener('keydown', onKey);
    window.addEventListener('focus', onFocus);
    window.addEventListener('blur', onBlur);
    window.addEventListener('contextmenu', noMenu);
    return () => {
      offs.forEach((p) => p.then((f) => f()).catch(() => {}));
      window.removeEventListener('keydown', onKey);
      window.removeEventListener('focus', onFocus);
      window.removeEventListener('blur', onBlur);
      window.removeEventListener('contextmenu', noMenu);
      stopIdle();
    };
  }, [api]);

  if (!show) return null;
  const pos = layout(show.items.length, show.below);
  return (
    <div
      class={`radial ${show.below ? 'below' : 'above'}`}
      key={seq}
      onPointerDown={(e) => {
        // 点在气泡之间的空白处：收起
        if (!(e.target as HTMLElement).closest('[data-id]')) close(0);
      }}
      onPointerMove={() => mode.current === 'click' && armIdle()}
    >
      {show.items.map((it: MenuItem, i) => {
        const p = pos[i];
        return (
          <button
            type="button"
            key={it.id}
            data-id={it.id}
            data-testid={`menu-${it.id}`}
            class={`bubble ${hover === it.id ? 'hover' : ''} ${it.on ? 'on' : ''} ${it.id === 'secret' ? 'secret' : ''}`}
            style={{
              left: `${p.x - p.size / 2}px`,
              top: `${p.y - p.size / 2}px`,
              width: `${p.size}px`,
              height: `${p.size}px`,
              animationDelay: `${i * 22}ms`,
            }}
            onPointerEnter={() => mode.current === 'click' && setHover(it.id)}
            onPointerLeave={() => mode.current === 'click' && setHover(null)}
            onClick={() => pick(it.id)}
            aria-label={it.label}
          >
            <span class="bubble-icon">{it.icon}</span>
            <span class="bubble-label">{it.label}</span>
          </button>
        );
      })}
    </div>
  );
}
