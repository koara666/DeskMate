// 打字机效果：把后端一段一段到来的文本，平滑地逐字显示出来。
// 只在有待显示内容时才运行定时器，显示完立即停止（空闲时没有任何定时器）。

export interface TypewriterOptions {
  /** 每次显示的间隔（毫秒） */
  tickMs: number;
}

export class Typewriter {
  private chars: string[] = [];
  private shown = 0;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private onEnd: (() => void) | null = null;

  constructor(
    private readonly onUpdate: (text: string) => void,
    private readonly opts: TypewriterOptions = { tickMs: 28 },
  ) {}

  /** 追加后端发来的增量 */
  push(text: string): void {
    // 按 Unicode 码点切分，避免把 emoji 劈成两半
    this.chars.push(...Array.from(text));
    this.schedule();
  }

  /** 立即显示全部已收到的内容（比如用户点了停止） */
  flush(): void {
    this.clear();
    this.shown = this.chars.length;
    this.onUpdate(this.text());
    this.finishIfDone();
  }

  /** 后端说完了：等剩下的字显示完后回调 */
  end(cb: () => void): void {
    this.onEnd = cb;
    if (this.shown >= this.chars.length) {
      this.finishIfDone();
    } else {
      this.schedule();
    }
  }

  dispose(): void {
    this.clear();
    this.onEnd = null;
  }

  text(): string {
    return this.chars.slice(0, this.shown).join('');
  }

  private schedule(): void {
    if (this.timer !== null || this.shown >= this.chars.length) return;
    this.timer = setTimeout(() => this.tick(), this.opts.tickMs);
  }

  private tick(): void {
    this.timer = null;
    const backlog = this.chars.length - this.shown;
    // 积压越多显示越快，保证不会落后太多
    const step = Math.max(1, Math.ceil(backlog / 6));
    this.shown = Math.min(this.chars.length, this.shown + step);
    this.onUpdate(this.text());
    if (this.shown < this.chars.length) {
      this.schedule();
    } else {
      this.finishIfDone();
    }
  }

  private finishIfDone(): void {
    if (this.onEnd && this.shown >= this.chars.length) {
      const cb = this.onEnd;
      this.onEnd = null;
      cb();
    }
  }

  private clear(): void {
    if (this.timer !== null) {
      clearTimeout(this.timer);
      this.timer = null;
    }
  }
}
