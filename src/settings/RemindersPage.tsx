// 「提醒」：查看、新建、编辑、暂停、删除提醒。聊天里说「半小时后提醒我收衣服」也能设。
import { useCallback, useEffect, useState } from 'preact/hooks';
import type { SettingsApi } from '../lib/ipc';
import type { Reminder, ReminderView } from '../lib/types';
import { ReminderForm, emptyReminder } from '../reminders/ReminderForm';
import { StatusLine, useStatus } from './SettingsApp';

type Api = Pick<SettingsApi, 'reminderList' | 'reminderSave' | 'reminderDelete' | 'reminderSetStatus'>;

const STATUS: Record<string, string> = { active: '', paused: '已暂停', done: '已完成' };

export function RemindersPage({ api }: { api: Api }) {
  const [list, setList] = useState<ReminderView[] | null>(null);
  /** 正在编辑的：0 = 新建 */
  const [editing, setEditing] = useState<Reminder | null>(null);
  const { status, run } = useStatus();

  const load = useCallback(() => {
    api
      .reminderList()
      .then(setList)
      .catch(() => setList([]));
  }, [api]);
  useEffect(load, [load]);

  const save = async (r: Reminder) => {
    await api.reminderSave(r);
    setEditing(null);
    load();
  };
  const active = list?.filter((r) => r.status !== 'done') ?? [];
  const done = list?.filter((r) => r.status === 'done') ?? [];

  return (
    <section>
      <h1>提醒</h1>
      <p class="muted">
        在聊天里说「半小时后提醒我收衣服」「每周五晚上提醒我倒垃圾」「明天开机时提醒我交作业」，她会复述一遍，你点「好」就设好了。
        电脑关着的时候错过的提醒，下次开机时会补上。玩游戏、看全屏视频时到了的提醒会改发系统通知（可以在「陪伴」页关掉）。
      </p>

      <div class="card">
        {editing && editing.id === 0 ? (
          <ReminderForm initial={editing} onSave={save} onCancel={() => setEditing(null)} />
        ) : (
          <button type="button" class="btn btn-primary" onClick={() => setEditing(emptyReminder())}>
            新建提醒
          </button>
        )}
      </div>

      <div class="card">
        <h2>进行中</h2>
        {list === null && <div class="muted">加载中…</div>}
        {list && active.length === 0 && <div class="muted small">还没有提醒。</div>}
        <ul class="plain reminder-list">
          {active.map((r) => (
            <li key={r.id}>
              {editing?.id === r.id ? (
                <ReminderForm initial={r} onSave={save} onCancel={() => setEditing(null)} />
              ) : (
                <div class="row">
                  <span class="grow">
                    <b>{r.content}</b>
                    <small class="muted">
                      {' '}
                      · {r.when}
                      {r.important ? ' · 重要' : ''}
                      {STATUS[r.status] ? ` · ${STATUS[r.status]}` : ''}
                    </small>
                  </span>
                  <button type="button" class="btn btn-small" onClick={() => setEditing(r)}>
                    编辑
                  </button>
                  <button
                    type="button"
                    class="btn btn-small"
                    onClick={() =>
                      run(
                        () => api.reminderSetStatus(r.id, r.status === 'paused' ? 'active' : 'paused'),
                        r.status === 'paused' ? '已恢复' : '已暂停',
                      ).then(load)
                    }
                  >
                    {r.status === 'paused' ? '恢复' : '暂停'}
                  </button>
                  <button
                    type="button"
                    class="btn btn-small btn-danger"
                    onClick={() => run(() => api.reminderDelete(r.id), '已删除').then(load)}
                  >
                    删除
                  </button>
                </div>
              )}
            </li>
          ))}
        </ul>
        <StatusLine status={status} />
      </div>

      {done.length > 0 && (
        <details class="card">
          <summary>已完成（{done.length}）</summary>
          <ul class="plain reminder-list">
            {done.map((r) => (
              <li key={r.id} class="row">
                <span class="grow muted">
                  {r.content}
                  {r.last_fired_at ? <small> · {new Date(r.last_fired_at * 1000).toLocaleString()}</small> : null}
                </span>
                <button
                  type="button"
                  class="btn btn-small"
                  onClick={() => run(() => api.reminderDelete(r.id), '已删除').then(load)}
                >
                  删除
                </button>
              </li>
            ))}
          </ul>
        </details>
      )}
    </section>
  );
}
