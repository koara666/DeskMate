// 任务卡片：确认 → 执行中（实时步骤）→ 需要批准 → 结果。
// 只有用户在这里点「开始执行」才会真正调用 Claude Code / Codex。
import { useEffect, useRef, useState } from 'preact/hooks';
import type { TaskApi } from '../lib/ipc';
import type { ApprovalItem, Executor, StepKind, TaskDefaults, TaskEvent } from '../lib/types';

type Phase = 'confirm' | 'running' | 'approval' | 'done' | 'failed';

const STEP_ICONS: Record<StepKind, string> = {
  read: '📖',
  edit: '✏️',
  command: '⌨️',
  network: '🌐',
  outside: '📂',
  note: '💬',
  other: '•',
};

interface Props {
  api: TaskApi;
  initialGoal: string;
  onClose: () => void;
  /** 任务结束后请角色转述结果 */
  onFinished: (taskId: string) => void;
}

export function TaskCard({ api, initialGoal, onClose, onFinished }: Props) {
  const [defaults, setDefaults] = useState<TaskDefaults | null>(null);
  const [goal, setGoal] = useState(initialGoal);
  const [workdir, setWorkdir] = useState('');
  const [executor, setExecutor] = useState<Executor>('claude');
  const [allowWrite, setAllowWrite] = useState(false);
  const [phase, setPhase] = useState<Phase>('confirm');
  const [steps, setSteps] = useState<{ kind: StepKind; text: string }[]>([]);
  const [items, setItems] = useState<ApprovalItem[]>([]);
  const [checked, setChecked] = useState<boolean[]>([]);
  const [result, setResult] = useState<{ title: string; text: string; detail?: string; extra?: string } | null>(null);
  const [error, setError] = useState('');
  const taskId = useRef('');
  const reported = useRef(false);

  useEffect(() => {
    api
      .defaults()
      .then((d) => {
        setDefaults(d);
        setWorkdir(d.root);
        const preferred = d.settings.executor;
        const ok = (e: Executor) => (e === 'claude' ? d.claude.found : d.codex.found);
        setExecutor(ok(preferred) ? preferred : ok('claude') ? 'claude' : 'codex');
      })
      .catch((e) => setError(String(e)));
  }, [api]);

  const finish = () => {
    if (!reported.current && taskId.current) {
      reported.current = true;
      onFinished(taskId.current);
    }
  };

  const onEvent = (e: TaskEvent) => {
    switch (e.type) {
      case 'started':
        taskId.current = e.task_id;
        setPhase('running');
        break;
      case 'step':
        setSteps((s) => [...s.slice(-49), { kind: e.kind, text: e.text }]);
        break;
      case 'needs_approval':
        setItems(e.items);
        setChecked(e.items.map(() => false));
        setResult({ title: '需要你批准', text: e.summary });
        setPhase('approval');
        break;
      case 'done':
        setResult({
          title: '完成了',
          text: e.summary,
          extra: [
            e.cost_usd > 0 ? `本次花费约 $${e.cost_usd.toFixed(4)}` : '',
            e.changed.length ? `改动的文件：${e.changed.join('、')}` : '',
            e.backup ? `开始前的备份：${e.backup}` : '',
          ]
            .filter(Boolean)
            .join('\n'),
        });
        setPhase('done');
        finish();
        break;
      case 'failed':
        setResult({ title: '没能完成', text: e.message, detail: e.detail });
        setPhase('failed');
        finish();
        break;
    }
  };

  const start = async () => {
    setError('');
    setSteps([]);
    try {
      taskId.current = await api.start(goal, workdir, executor, allowWrite, onEvent);
      setPhase('running');
    } catch (e) {
      setError(String(e));
    }
  };

  const approve = async () => {
    const idx = checked.map((c, i) => (c ? i : -1)).filter((i) => i >= 0);
    try {
      await api.approve(taskId.current, idx, onEvent);
      setPhase('running');
    } catch (e) {
      setError(String(e));
      setPhase('done');
      finish();
    }
  };

  const decline = async () => {
    await api.decline(taskId.current).catch(() => {});
    setPhase('done');
    finish();
  };

  const canRun = (e: Executor) => (defaults ? (e === 'claude' ? defaults.claude.found : defaults.codex.found) : false);
  const busy = phase === 'running';

  return (
    <div class="task-card" role="region" aria-label="任务">
      <div class="task-head">
        <span class="task-title">🛠 交给执行工具</span>
        {!busy && (
          <button type="button" class="icon-btn" aria-label="关闭任务卡片" onClick={onClose}>
            ✕
          </button>
        )}
      </div>

      {phase === 'confirm' && (
        <div class="task-body">
          <label class="task-field">
            要做的事
            <textarea rows={3} value={goal} aria-label="要做的事" onInput={(e) => setGoal((e.currentTarget as HTMLTextAreaElement).value)} />
          </label>
          <label class="task-field">
            在哪个文件夹做（必须在任务文件夹里）
            <input value={workdir} aria-label="工作目录" spellcheck={false} onInput={(e) => setWorkdir((e.currentTarget as HTMLInputElement).value)} />
          </label>
          <div class="task-row">
            <select aria-label="执行工具" value={executor} onChange={(e) => setExecutor((e.currentTarget as HTMLSelectElement).value as Executor)}>
              <option value="claude" disabled={!canRun('claude')}>
                Claude Code{canRun('claude') ? '' : '（未找到）'}
              </option>
              <option value="codex" disabled={!canRun('codex')}>
                Codex{canRun('codex') ? '' : '（未找到）'}
              </option>
            </select>
            {defaults && (
              <span class="muted small">
                最长 {defaults.settings.timeout_minutes} 分钟
                {executor === 'claude' ? `，最多花费 $${defaults.settings.budget_usd.toFixed(2)}` : ''}
              </span>
            )}
          </div>
          {executor === 'claude' ? (
            <p class="task-note">只会在这个文件夹里读写文件。执行命令、删除文件、联网、访问别的文件夹之前都会先问你。</p>
          ) : (
            <>
              <p class="task-note">Codex 默认只读：只能查看和分析，不会改动任何文件，也不能联网。</p>
              <label class="task-row small">
                <input type="checkbox" checked={allowWrite} onChange={(e) => setAllowWrite((e.currentTarget as HTMLInputElement).checked)} />
                允许 Codex 修改文件（包括新建和删除）。Codex 不能逐条询问，所以开始前会先把整个文件夹备份一份。
              </label>
            </>
          )}
          {error && <div class="task-error">{error}</div>}
          <div class="task-actions">
            <button type="button" class="btn btn-send" disabled={!goal.trim() || !canRun(executor)} onClick={start}>
              开始执行
            </button>
            <button type="button" class="btn" onClick={onClose}>
              先不做了
            </button>
          </div>
        </div>
      )}

      {(phase === 'running' || phase === 'approval' || phase === 'done' || phase === 'failed') && steps.length > 0 && (
        <ul class="task-steps" aria-label="执行步骤">
          {steps.slice(-6).map((s, i) => (
            <li key={i}>
              <span aria-hidden="true">{STEP_ICONS[s.kind]}</span> {s.text}
            </li>
          ))}
        </ul>
      )}

      {phase === 'running' && (
        <div class="task-actions">
          <span class="muted small">执行中…</span>
          <button type="button" class="btn btn-stop" onClick={() => api.cancel(taskId.current).catch(() => {})}>
            停止任务
          </button>
        </div>
      )}

      {phase === 'approval' && (
        <div class="task-body">
          {result?.text && <div class="task-summary">{result.text}</div>}
          <p class="task-note">下面这些操作需要你同意才能继续，勾选你允许的：</p>
          <ul class="task-approvals">
            {items.map((it, i) => {
              const ok = !!(it.rule || it.path);
              return (
                <li key={i}>
                  <label>
                    <input
                      type="checkbox"
                      disabled={!ok}
                      checked={checked[i]}
                      onChange={(e) => setChecked((c) => c.map((v, j) => (j === i ? (e.currentTarget as HTMLInputElement).checked : v)))}
                    />{' '}
                    {it.detail}
                    {!ok && <span class="muted small">（包含特殊字符，无法单独批准）</span>}
                  </label>
                </li>
              );
            })}
          </ul>
          <div class="task-actions">
            <button type="button" class="btn btn-send" disabled={!checked.some(Boolean)} onClick={approve}>
              批准并继续
            </button>
            <button type="button" class="btn" onClick={decline}>
              不批准，结束
            </button>
          </div>
        </div>
      )}

      {(phase === 'done' || phase === 'failed') && result && (
        <div class="task-body">
          <div class={`task-result ${phase}`}>{result.title}</div>
          {result.text && <div class="task-summary">{result.text}</div>}
          {result.extra && <div class="muted small task-extra">{result.extra}</div>}
          {result.detail && (
            <details class="small">
              <summary>查看详情</summary>
              <div class="task-extra">{result.detail}</div>
            </details>
          )}
          <div class="task-actions">
            <button type="button" class="btn" onClick={onClose}>
              关闭
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
