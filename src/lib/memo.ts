// 极简版 memo：属性没变（浅比较）就不重新渲染。
// 不用 preact/compat 的 memo：compat 会全局改写事件处理（比如把部分 onChange 换成 onInput），还多 5 KB。
import { Component, h, type FunctionComponent } from 'preact';

export function memo<P extends object>(fn: FunctionComponent<P>) {
  return class Memo extends Component<P> {
    override shouldComponentUpdate(next: P) {
      const a = this.props as Record<string, unknown>;
      const b = next as Record<string, unknown>;
      const keys = Object.keys(a);
      return keys.length !== Object.keys(b).length || keys.some((k) => a[k] !== b[k]);
    }
    override render(props: P) {
      return h(fn, props);
    }
  };
}
