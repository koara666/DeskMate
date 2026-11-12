// 一项事项：勾掉 = 做完；「⋯」里可以只记一部分、跳过、推到明天、改一改、写备注、删掉。
import { useState } from 'preact/hooks';
import type { PlansApi } from '../lib/ipc';
import type { PlanTaskView } from '../lib/types';
import { addDays, clampInt, mins, relDay } from './util';

type Api = Pick<PlansApi, 'doneTask' | 'taskStatus' | 'moveTask' | 'deleteTask' | 'saveTask'>;

type Mode = 'view' | 'menu' | 'edit' | 'partial' | 'move';

export function TaskRow({
  task,
  today,
  api,
  onChanged,
  onError,
  onFocus,
  showLabel = true,
}: {
  task: PlanTaskView;
  today: string;
  api: Api;
  onChanged: () => void;
  onError: (e: unknown) => void;
  /** 开始专注（不传就没有这个按钮） */
  onFocus?: (t: PlanTaskView) => void;
  showLabel?: boolean;
}) {
  const [mode, setMode] = useState<Mode>('view');
  const [title, setTitle] = useState(task.title);
  const [minutes, setMinutes] = useState(String(task.minutes || ''));
  const [start, setStart] = useState(task.start);
  const [note, setNote] = useState(task.note);
  const [spent, setSpent] = useState(String(Math.max(5, Math.round(task.minutes / 2))));
  const [finished, setFinished] = useState(false);
  const [moveTo, setMoveTo] = useState(addDays(today, 1));
  const done = task.status === 'done';
  const skipped = task.status === 'skipped';

  const run = async (fn: () => Promise<unknown>) => {
    try {
      await fn();
      setMode('view');
      onChanged();
    } catch (e) {
      onError(e);
    }
  };

  const toggle = () =>
    run(() => (task.status === 'todo' ? api.doneTask(task.id, null, null) : api.taskStatus(task.id, 'todo')));

  const openEdit = () => {
    setTitle(task.title);
    setMinutes(String(task.minutes || ''));
    setStart(task.start);
    setNote(task.note);
    setMode('edit');
  };

  const meta: string[] = [];
  if (task.start) meta.push(task.start);
  if (done && task.spent > 0) meta.push(task.spent === task.minutes ? mins(task.spent) : `做了 ${mins(task.spent)}`);
  else if (task.minutes > 0) meta.push(mins(task.minutes));
  if (skipped) meta.push('跳过了');

  return (
    <li class={`task task-${task.status}`}>
      <div class="task-line">
        <button
          type="button"
          class={`check ${done ? 'on' : ''}`}
          role="checkbox"
          aria-checked={done}
          aria-label={done ? `改回没做：${task.title}` : `做完了：${task.title}`}
          onClick={toggle}
        >
          {done ? '✓' : ''}
        </button>
        <div class="task-main">
          <div class="task-title">{task.title}</div>
          <div class="task-meta">
            {meta.map((m) => (
              <span key={m}>{m}</span>
            ))}
            {showLabel && task.label && <span class="task-plan">{task.label}</span>}
            {task.orig_day && task.orig_day !== task.day && <span class="tag">从{relDay(task.orig_day, today)}挪过来</span>}
          </div>
          {task.note && mode !== 'edit' && <div class="task-note">📝 {task.note}</div>}
        </div>
        <div class="task-actions">
          {onFocus && task.status === 'todo' && (
            <button type="button" class="icon-btn" title="开始专注" aria-label={`开始专注：${task.title}`} onClick={() => onFocus(task)}>
              ▶
            </button>
          )}
          <button
            type="button"
            class="icon-btn"
            title="更多"
            aria-label={`更多：${task.title}`}
            aria-expanded={mode === 'menu'}
            onClick={() => setMode(mode === 'menu' ? 'view' : 'menu')}
          >
            ⋯
          </button>
        </div>
      </div>

      {mode === 'menu' && (
        <div class="task-menu" role="menu">
          {task.status === 'todo' && (
            <button type="button" role="menuitem" class="btn btn-small" onClick={() => setMode('partial')}>
              只做了一部分
            </button>
          )}
          {task.status === 'todo' && (
            <button type="button" role="menuitem" class="btn btn-small" onClick={() => run(() => api.moveTask(task.id, addDays(today, 1)))}>
              推到明天
            </button>
          )}
          {task.status === 'todo' && (
            <button type="button" role="menuitem" class="btn btn-small" onClick={() => setMode('move')}>
              挪到某天…
            </button>
          )}
          {task.status === 'todo' && (
            <button type="button" role="menuitem" class="btn btn-small" onClick={() => run(() => api.taskStatus(task.id, 'skipped'))}>
              今天跳过
            </button>
          )}
          {task.status !== 'todo' && (
            <button type="button" role="menuitem" class="btn btn-small" onClick={() => run(() => api.taskStatus(task.id, 'todo'))}>
              改回没做
            </button>
          )}
          <button type="button" role="menuitem" class="btn btn-small" onClick={openEdit}>
            改一改 / 备注
          </button>
          <button type="button" role="menuitem" class="btn btn-small btn-danger" onClick={() => run(() => api.deleteTask(task.id))}>
            删掉
          </button>
        </div>
      )}

      {mode === 'partial' && (
        <form
          class="task-form"
          onSubmit={(e) => {
            e.preventDefault();
            run(() => api.doneTask(task.id, clampInt(spent, 0, 1440), task.item_id ? finished : true));
          }}
        >
          <label>
            做了多久
            <span class="inline">
              <input type="number" min={0} max={1440} step={5} value={spent} onInput={(e) => setSpent(e.currentTarget.value)} aria-label="做了多少分钟" />
              分钟
            </span>
          </label>
          {task.item_id !== null && (
            <label class="check-label">
              <input type="checkbox" checked={finished} onChange={(e) => setFinished(e.currentTarget.checked)} />
              这部分其实已经做完了
            </label>
          )}
          <p class="muted small">没做完的部分会排到后面的日子里。</p>
          <div class="actions">
            <button type="submit" class="btn btn-primary btn-small">
              记下
            </button>
            <button type="button" class="btn btn-small" onClick={() => setMode('view')}>
              算了
            </button>
          </div>
        </form>
      )}

      {mode === 'move' && (
        <form
          class="task-form"
          onSubmit={(e) => {
            e.preventDefault();
            run(() => api.moveTask(task.id, moveTo));
          }}
        >
          <label>
            挪到哪天
            <input type="date" min={today} value={moveTo} onInput={(e) => setMoveTo(e.currentTarget.value)} aria-label="挪到哪天" />
          </label>
          <div class="actions">
            <button type="submit" class="btn btn-primary btn-small">
              挪过去
            </button>
            <button type="button" class="btn btn-small" onClick={() => setMode('view')}>
              算了
            </button>
          </div>
        </form>
      )}

      {mode === 'edit' && (
        <form
          class="task-form"
          onSubmit={(e) => {
            e.preventDefault();
            run(() =>
              api.saveTask({
                ...task,
                title,
                minutes: clampInt(minutes || 0, 0, 720),
                start,
                note,
              }),
            );
          }}
        >
          <div class="grid">
            <label class="span2">
              要做什么
              <input value={title} maxLength={100} onInput={(e) => setTitle(e.currentTarget.value)} aria-label="要做什么" />
            </label>
            <label>
              大概多久（分钟）
              <input type="number" min={0} max={720} step={5} value={minutes} onInput={(e) => setMinutes(e.currentTarget.value)} aria-label="大概多少分钟" />
            </label>
            <label>
              几点开始（可空）
              <input type="time" value={start} onInput={(e) => setStart(e.currentTarget.value)} aria-label="几点开始" />
            </label>
            <label class="span2">
              备注
              <textarea rows={2} maxLength={1000} value={note} onInput={(e) => setNote(e.currentTarget.value)} aria-label="备注" />
            </label>
          </div>
          <div class="actions">
            <button type="submit" class="btn btn-primary btn-small" disabled={!title.trim()}>
              保存
            </button>
            <button type="button" class="btn btn-small" onClick={() => setMode('view')}>
              算了
            </button>
          </div>
        </form>
      )}
    </li>
  );
}
