// 任务执行设置：检测 Claude Code / Codex、任务文件夹、时间和花费上限、最近的任务记录。
import { useEffect, useState } from 'preact/hooks';
import { taskSettingsApi } from '../lib/ipc';
import type { ExecutorInfo, TaskDefaults, TaskRecord, TaskSettings } from '../lib/types';
import { StatusLine, useStatus } from './SettingsApp';

type Api = typeof taskSettingsApi;

const STATUS: Record<string, string> = {
  running: '执行中',
  needs_approval: '等待批准',
  done: '完成',
  failed: '失败',
  cancelled: '已停止',
  timeout: '超时',
};

function fmt(ts: number) {
  const d = new Date(ts * 1000);
  const p = (n: number) => String(n).padStart(2, '0');
  return `${d.getMonth() + 1}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

export function TasksPage({ api = taskSettingsApi }: { api?: Api }) {
  const [d, setD] = useState<TaskDefaults | null>(null);
  const [s, setS] = useState<TaskSettings | null>(null);
  const [list, setList] = useState<TaskRecord[]>([]);
  const { status, run } = useStatus();

  useEffect(() => {
    api
      .defaults()
      .then((x) => {
        setD(x);
        setS({ ...x.settings, root: x.root });
      })
      .catch(() => {});
    api.list().then(setList).catch(() => {});
  }, [api]);

  if (!d || !s) return <div class="muted">正在检测本机的 Claude Code 和 Codex…</div>;

  const set = <K extends keyof TaskSettings>(k: K, v: TaskSettings[K]) => setS({ ...s, [k]: v });

  return (
    <section>
      <h1>任务执行</h1>
      <p class="muted">
        需要动手做的事（整理文件、写代码、查资料），角色会先和你确认，你在聊天窗口的卡片上点「开始执行」后，才交给本机的 Claude Code 或 Codex 去做。
      </p>

      <div class="card">
        <h2>执行工具</h2>
        <Detect name="Claude Code" info={d.claude} login="claude" />
        <Detect name="Codex" info={d.codex} login="codex" />
        <ul class="plain small">
          <li>
            通过 Claude Code 执行任务，会消耗你 Claude 订阅里<strong>单独的 Agent SDK 月度额度</strong>（不是聊天额度），用完会停止；如果你用的是 API Key 则按量计费。每次任务的用量会记在「用量与花费」里。
          </li>
          <li>Codex 使用你自己的 ChatGPT 账号额度。</li>
          <li>DeskMate 只调用官方命令行程序，不会读取或转发它们的登录信息；每个人请使用自己的账号。</li>
        </ul>
        <div class="row">
          <span class="label">默认使用</span>
          <select value={s.executor} aria-label="默认执行工具" onChange={(e) => set('executor', (e.currentTarget as HTMLSelectElement).value as TaskSettings['executor'])}>
            <option value="claude">Claude Code（可以逐项确认危险操作，推荐）</option>
            <option value="codex">Codex（默认只读）</option>
          </select>
        </div>
      </div>

      <div class="card">
        <h2>安全</h2>
        <div class="row">
          <span class="label">任务文件夹</span>
          <input class="grow" value={s.root} aria-label="任务文件夹" spellcheck={false} onInput={(e) => set('root', (e.currentTarget as HTMLInputElement).value)} />
        </div>
        <ul class="plain small">
          <li>执行工具只能在这个文件夹（和它的子文件夹）里操作；需要处理的文件请先放进来。</li>
          <li>删除文件、执行命令、联网、访问别的文件夹、安装软件之前，都会在聊天窗口里问你。</li>
          <li class="warn-text">请不要在任务文件夹里放密码、证件照片、银行资料等敏感文件。</li>
        </ul>
        <div class="row">
          <label>
            每个任务最长（分钟）
            <input type="number" min={1} max={120} value={s.timeout_minutes} onInput={(e) => set('timeout_minutes', Number((e.currentTarget as HTMLInputElement).value) || 10)} />
          </label>
          <label>
            每个任务最多花费（美元，Claude Code）
            <input type="number" min={0.01} step="0.1" value={s.budget_usd} onInput={(e) => set('budget_usd', Number((e.currentTarget as HTMLInputElement).value) || 0.5)} />
          </label>
        </div>
        <details>
          <summary class="small">高级：指定模型或程序位置（一般不用填）</summary>
          <div class="grid">
            <label>
              Claude Code 模型
              <input value={s.claude_model} placeholder="留空 = 默认" onInput={(e) => set('claude_model', (e.currentTarget as HTMLInputElement).value)} />
            </label>
            <label>
              Codex 模型
              <input value={s.codex_model} placeholder="留空 = 默认" onInput={(e) => set('codex_model', (e.currentTarget as HTMLInputElement).value)} />
            </label>
            <label>
              claude 程序位置
              <input value={s.claude_path} placeholder="留空 = 自动查找" onInput={(e) => set('claude_path', (e.currentTarget as HTMLInputElement).value)} />
            </label>
            <label>
              codex 程序位置
              <input value={s.codex_path} placeholder="留空 = 自动查找" onInput={(e) => set('codex_path', (e.currentTarget as HTMLInputElement).value)} />
            </label>
          </div>
        </details>
        <div class="actions">
          <button
            type="button"
            class="btn btn-primary"
            onClick={() =>
              run(async () => {
                const root = await api.save(s);
                setS({ ...s, root });
              })
            }
          >
            保存
          </button>
          <StatusLine status={status} />
        </div>
      </div>

      <div class="card">
        <h2>最近的任务</h2>
        {list.length === 0 ? (
          <div class="muted">还没有执行过任务</div>
        ) : (
          <table class="table">
            <thead>
              <tr>
                <th>时间</th>
                <th>工具</th>
                <th>任务</th>
                <th>状态</th>
                <th class="num">花费</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {list.map((t) => (
                <tr key={t.id}>
                  <td>{fmt(t.created_at)}</td>
                  <td>{t.executor}</td>
                  <td title={t.summary}>{t.goal}</td>
                  <td>{STATUS[t.status] ?? t.status}</td>
                  <td class="num">{t.cost_usd > 0 ? `$${t.cost_usd.toFixed(4)}` : '—'}</td>
                  <td>
                    <button type="button" class="link-btn" onClick={() => api.openLog(t.id).catch(() => {})}>
                      日志
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </section>
  );
}

function Detect({ name, info, login }: { name: string; info: ExecutorInfo; login: string }) {
  return (
    <div class="row">
      <span class="label">{name}</span>
      {info.found ? (
        <span>
          <span class="tag tag-ok">已找到</span> <span class="mono small">{info.version}</span>
        </span>
      ) : (
        <span class="small">
          <span class="tag tag-warn">未找到</span> 安装后在命令行里运行一次 <span class="mono">{login}</span> 完成登录即可
        </span>
      )}
    </div>
  );
}
