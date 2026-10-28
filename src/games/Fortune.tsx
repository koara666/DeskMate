// 今日运势：每天抽一支签（纯本地随机），她用自己的口吻解读。
import { useEffect, useState } from 'preact/hooks';
import type { Fortune } from '../lib/types';
import type { GameProps } from './registry';

export function FortuneGame({ api }: GameProps) {
  const [f, setF] = useState<Fortune | null | undefined>(undefined);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    api
      .fortuneToday()
      .then(setF)
      .catch(() => setF(null));
  }, [api]);

  if (f === undefined) return <div class="muted">…</div>;
  if (!f)
    return (
      <div class="game fortune">
        <p class="muted">每天可以抽一支签。</p>
        <button
          type="button"
          class="btn btn-small"
          disabled={busy}
          onClick={async () => {
            setBusy(true);
            try {
              setF(await api.fortuneDraw());
            } finally {
              setBusy(false);
            }
          }}
        >
          抽签
        </button>
      </div>
    );
  return (
    <div class="game fortune">
      <div class={`fortune-level lv-${f.level}`}>{f.name}</div>
      <div class="fortune-items">
        幸运色 <b>{f.color}</b> · 幸运物 <b>{f.item}</b> · 方位 <b>{f.direction}</b>
      </div>
      <p class="fortune-comment">{f.comment}</p>
      <p class="muted small">今天已经抽过了，明天再来吧。</p>
    </div>
  );
}
