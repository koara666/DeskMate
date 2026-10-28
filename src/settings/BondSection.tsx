// 「陪伴」页最上面：你和她现在的关系（亲密度、心情）、希望她怎么称呼你。
import { useEffect, useState } from 'preact/hooks';
import type { SettingsApi } from '../lib/ipc';
import type { CompanionStatus } from '../lib/types';
import { StatusLine, useStatus } from './SettingsApp';

export function BondSection({ api }: { api: Pick<SettingsApi, 'companionStatus' | 'saveNickname'> }) {
  const [s, setS] = useState<CompanionStatus | null>(null);
  const [nick, setNick] = useState('');
  const { status, run } = useStatus();
  useEffect(() => {
    api
      .companionStatus()
      .then((x) => {
        setS(x);
        setNick(x.nickname);
      })
      .catch(() => {});
  }, [api]);
  if (!s) return null;
  const pct = s.next ? Math.min(100, Math.round((s.points / s.next) * 100)) : 100;
  return (
    <div class="card">
      <h2>你和{s.character}</h2>
      <div class="row">
        <span class="label">关系</span>
        <span class="grow">
          {s.title}（亲密度 {s.level}/5）
        </span>
      </div>
      <div class="bar" aria-label="亲密度进度">
        <div class="fill" style={{ width: `${pct}%` }} />
      </div>
      <p class="muted small">
        每天聊天、摸摸头、一起玩、回应她，亲密度会慢慢增长（每天有上限），解锁新的称呼和台词。不来也不会下降。现在她的心情：{s.mood_word}。
      </p>
      <div class="row">
        <span class="label">希望她怎么称呼你</span>
        <input value={nick} maxLength={20} placeholder="比如 小林" aria-label="称呼" onInput={(e) => setNick((e.currentTarget as HTMLInputElement).value)} />
        <button type="button" class="btn btn-small" onClick={() => run(() => api.saveNickname(nick))}>
          保存
        </button>
      </div>
      <p class="muted small">熟悉一点以后她才会这样叫你（现在叫你「{s.call}」）。</p>
      <StatusLine status={status} />
    </div>
  );
}
