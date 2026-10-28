// 「日记」：她每天写的日记（第二天补写前一天的）。可以翻看、删除。
import { useCallback, useEffect, useState } from 'preact/hooks';
import type { SettingsApi } from '../lib/ipc';
import type { Diary } from '../lib/types';

export function DiaryPage({ api }: { api: Pick<SettingsApi, 'diaryList' | 'diaryDelete'> }) {
  const [list, setList] = useState<Diary[] | null>(null);
  const load = useCallback(() => {
    api
      .diaryList()
      .then(setList)
      .catch(() => setList([]));
  }, [api]);
  useEffect(load, [load]);
  return (
    <section>
      <h1>日记</h1>
      <p class="muted">
        每天结束后，她会用自己的口吻写一篇短短的日记：那天你做了什么、你们聊了什么、她的心情。日记只根据活动汇总和聊天摘要来写，也会成为她记忆的一部分。
      </p>
      {list === null && <div class="muted">加载中…</div>}
      {list && list.length === 0 && <div class="card muted">还没有日记。和她相处一天以后再来看看吧。</div>}
      {list?.map((d) => (
        <article key={d.id} class="card diary">
          <div class="row">
            <b class="grow">{d.day}</b>
            <button type="button" class="btn btn-small" onClick={() => api.diaryDelete(d.id).then(load).catch(() => {})}>
              删除
            </button>
          </div>
          <p class="diary-text">{d.content}</p>
        </article>
      ))}
    </section>
  );
}
