// 角色自己的界面（v2.1 P13）：皮肤（程序自带的样式，见 src/styles/skins/）+ 角色包里写的颜色。
// 颜色作为 --t-<名字> 变量放在根元素上，皮肤的样式表再决定怎么用（这样深色模式的媒体查询照样生效）。
// 颜色值后端已经检查过（只允许 #十六进制），这里只是转交。
import type { Theme } from './types';

export function applyTheme(t: Theme | null | undefined, root: HTMLElement = document.documentElement) {
  root.dataset.skin = t?.skin || 'default';
  for (const k of Array.from(root.style)) {
    if (k.startsWith('--t-')) root.style.removeProperty(k);
  }
  for (const [k, v] of Object.entries(t?.colors ?? {})) {
    if (/^[a-z0-9-]+$/.test(k) && /^#[0-9a-f]{3,8}$/i.test(v)) root.style.setProperty(`--t-${k}`, v);
  }
}
