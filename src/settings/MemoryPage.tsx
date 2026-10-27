// 记忆管理：暂停、查看、搜索、编辑、删除、手动添加、导出、清空。
import { useCallback, useEffect, useState } from 'preact/hooks';
import type { SettingsApi } from '../lib/ipc';
import type { Memory, MemoryFilter, MemoryOverview } from '../lib/types';
import { StatusLine, useStatus } from './SettingsApp';

const SOURCE_LABELS: Record<string, string> = {
  chat: '聊天',
  user_edit: '你手动填写',
  task: '任务结果',
  file: '文件内容',
  web: '网页',
};

const PAGE = 50;

function fmtDate(ts: number): string {
  const d = new Date(ts * 1000);
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

export function MemoryPage({ api }: { api: SettingsApi }) {
  const [ov, setOv] = useState<MemoryOverview | null>(null);
  const [filter, setFilter] = useState<MemoryFilter>({
    query: '',
    kind: '',
    scope: '',
    include_history: false,
    limit: PAGE,
    offset: 0,
  });
  const [items, setItems] = useState<Memory[]>([]);
  const [total, setTotal] = useState(0);
  const { status, run } = useStatus();

  const loadOverview = useCallback(() => api.memoryOverview().then(setOv).catch(() => {}), [api]);
  const loadList = useCallback(
    async (f: MemoryFilter) => {
      const r = await api.memoryList(f);
      setItems((prev) => (f.offset === 0 ? r.items : [...prev, ...r.items]));
      setTotal(r.total);
    },
    [api],
  );

  useEffect(() => {
    loadOverview();
  }, [loadOverview]);
  useEffect(() => {
    loadList(filter).catch(() => {});
  }, [filter, loadList]);

  const refresh = () => {
    loadOverview();
    setFilter((f) => ({ ...f, offset: 0 }));
  };
  const scopeName = (id: string) => ov?.scopes.find((s) => s.id === id)?.name ?? id;

  return (
    <section>
      <h1>记忆</h1>
      <p class="muted">
        角色会记住你在聊天里说过的重要事情（名字、喜好、计划……），每次对话结束后在后台整理。
        密码、验证码、证件号、银行卡号之类的信息不会被记住。你也可以在聊天里说「忘掉这件事」。
      </p>

      {ov && (
        <div class="card">
          <label class="row">
            <input
              type="checkbox"
              checked={ov.paused}
              onChange={(e) =>
                run(() => api.memorySetPaused((e.currentTarget as HTMLInputElement).checked), '已更新').then(loadOverview)
              }
            />
            <span>
              暂停记忆
              <small>暂停期间的聊天不会被记住，聊完后自动删除；已有的记忆仍然会用到</small>
            </span>
          </label>
          <div class="row small muted">
            <span>长期事实 {ov.stats.facts} 条</span>
            <span>聊天摘要 {ov.stats.summaries} 条</span>
            <span>历史版本 {ov.stats.history} 条</span>
            <span>会话 {ov.stats.conversations} 次</span>
          </div>
          <div class="row small">
            <span class="muted">最近备份：{ov.last_backup ?? '还没有（启动 1 分钟后自动备份）'}</span>
            <button type="button" class="link-btn" onClick={() => api.openDataDir('backups').catch(() => {})}>
              打开备份文件夹
            </button>
          </div>
        </div>
      )}

      {ov && <AddMemory api={api} scopes={ov.scopes} onAdded={refresh} />}

      <div class="card">
        <div class="row">
          <input
            type="search"
            placeholder="搜索记忆…"
            aria-label="搜索记忆"
            value={filter.query}
            onInput={(e) => setFilter({ ...filter, query: (e.currentTarget as HTMLInputElement).value, offset: 0 })}
          />
          <select
            aria-label="类型"
            value={filter.kind}
            onChange={(e) => setFilter({ ...filter, kind: (e.currentTarget as HTMLSelectElement).value, offset: 0 })}
          >
            <option value="">全部类型</option>
            <option value="fact">长期事实</option>
            <option value="summary">聊天摘要</option>
          </select>
          <select
            aria-label="范围"
            value={filter.scope}
            onChange={(e) => setFilter({ ...filter, scope: (e.currentTarget as HTMLSelectElement).value, offset: 0 })}
          >
            <option value="">全部范围</option>
            {ov?.scopes.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name}
              </option>
            ))}
          </select>
          <label class="inline small">
            <input
              type="checkbox"
              checked={filter.include_history}
              onChange={(e) =>
                setFilter({ ...filter, include_history: (e.currentTarget as HTMLInputElement).checked, offset: 0 })
              }
            />
            显示历史版本
          </label>
        </div>
        <StatusLine status={status} />
        {items.length === 0 ? (
          <div class="muted">{filter.query ? '没有找到' : '还没有记忆。多聊几句，角色就会慢慢记住你～'}</div>
        ) : (
          <ul class="memory-list">
            {items.map((m) => (
              <MemoryRow
                key={m.id}
                m={m}
                scopeName={scopeName(m.scope)}
                onSave={(content, importance) =>
                  run(() => api.memoryUpdate(m.id, content, importance)).then((ok) => ok && refresh())
                }
                onDelete={() => {
                  if (confirm('确定删除这条记忆吗？删除后无法恢复。')) {
                    run(() => api.memoryDelete(m.id), '已删除').then((ok) => ok && refresh());
                  }
                }}
              />
            ))}
          </ul>
        )}
        {items.length < total && (
          <button type="button" class="btn" onClick={() => setFilter({ ...filter, offset: items.length })}>
            加载更多（还有 {total - items.length} 条）
          </button>
        )}
      </div>

      <DangerZone api={api} onCleared={refresh} />
    </section>
  );
}

interface RowProps {
  m: Memory;
  scopeName: string;
  onSave: (content: string, importance: number) => void;
  onDelete: () => void;
}

function MemoryRow({ m, scopeName, onSave, onDelete }: RowProps) {
  const [editing, setEditing] = useState(false);
  const [text, setText] = useState(m.content);
  const [imp, setImp] = useState(m.importance);
  const history = m.status !== 'active';

  return (
    <li class={`memory-item ${history ? 'history' : ''}`} data-testid="memory-item">
      {editing ? (
        <div class="memory-edit">
          <textarea rows={2} value={text} aria-label="记忆内容" onInput={(e) => setText((e.currentTarget as HTMLTextAreaElement).value)} />
          <div class="row small">
            重要度
            <select value={imp} aria-label="重要度" onChange={(e) => setImp(Number((e.currentTarget as HTMLSelectElement).value))}>
              {[1, 2, 3, 4, 5].map((n) => (
                <option key={n} value={n}>
                  {n}
                </option>
              ))}
            </select>
            <button
              type="button"
              class="btn btn-small btn-primary"
              onClick={() => {
                onSave(text, imp);
                setEditing(false);
              }}
            >
              保存
            </button>
            <button type="button" class="btn btn-small" onClick={() => setEditing(false)}>
              取消
            </button>
          </div>
        </div>
      ) : (
        <div class="memory-content">
          {m.kind === 'summary' && <span class="tag">摘要</span>} {m.content}
        </div>
      )}
      <div class="memory-meta small muted">
        <span>{scopeName}</span>
        <span>{'★'.repeat(m.importance)}</span>
        <span>来源：{SOURCE_LABELS[m.source_kind] ?? m.source_kind}</span>
        {!m.trusted && <span class="tag tag-warn">不可信来源</span>}
        {history && <span class="tag">已被新信息取代</span>}
        <span>{m.day || fmtDate(m.updated_at)}</span>
        {!editing && !history && (
          <button type="button" class="link-btn" onClick={() => setEditing(true)}>
            编辑
          </button>
        )}
        <button type="button" class="link-btn danger" onClick={onDelete}>
          删除
        </button>
      </div>
    </li>
  );
}

function AddMemory({ api, scopes, onAdded }: { api: SettingsApi; scopes: { id: string; name: string }[]; onAdded: () => void }) {
  const [text, setText] = useState('');
  const [scope, setScope] = useState('global');
  const [imp, setImp] = useState(3);
  const { status, run } = useStatus();
  return (
    <div class="card">
      <h2>手动告诉角色一件事</h2>
      <div class="row">
        <input
          class="grow"
          placeholder="比如：我对花生过敏"
          aria-label="新记忆"
          value={text}
          onInput={(e) => setText((e.currentTarget as HTMLInputElement).value)}
        />
        <select aria-label="新记忆范围" value={scope} onChange={(e) => setScope((e.currentTarget as HTMLSelectElement).value)}>
          {scopes.map((s) => (
            <option key={s.id} value={s.id}>
              {s.name}
            </option>
          ))}
        </select>
        <select aria-label="新记忆重要度" value={imp} onChange={(e) => setImp(Number((e.currentTarget as HTMLSelectElement).value))}>
          {[1, 2, 3, 4, 5].map((n) => (
            <option key={n} value={n}>
              重要度 {n}
            </option>
          ))}
        </select>
        <button
          type="button"
          class="btn btn-primary"
          disabled={!text.trim()}
          onClick={() =>
            run(() => api.memoryAdd(text, scope, imp), '已记住').then((ok) => {
              if (ok) {
                setText('');
                onAdded();
              }
            })
          }
        >
          添加
        </button>
      </div>
      <StatusLine status={status} />
    </div>
  );
}

function DangerZone({ api, onCleared }: { api: SettingsApi; onCleared: () => void }) {
  const [confirmText, setConfirmText] = useState('');
  const [files, setFiles] = useState<string[] | null>(null);
  const [file, setFile] = useState('');
  const { status, run, setStatus } = useStatus();
  const loadFiles = async () => {
    const list = await api.memoryImportList();
    setFiles(list);
    setFile(list[0] ?? '');
  };
  const doImport = async () => {
    try {
      const r = await api.memoryImport(file);
      setStatus({ ok: true, text: `导入完成：新增 ${r.added} 条，重复跳过 ${r.duplicates} 条，不合规拒绝 ${r.rejected} 条` });
      onCleared();
    } catch (e) {
      setStatus({ ok: false, text: String(e) });
    }
  };
  return (
    <div class="card">
      <h2>导出、导入与清空</h2>
      <div class="actions">
        <button type="button" class="btn" onClick={() => run(() => api.memoryExport(), '已导出到数据目录的 exports 文件夹')}>
          导出全部记忆（JSON + Markdown）
        </button>
        <button type="button" class="btn" onClick={() => loadFiles().catch(() => {})}>
          导入…
        </button>
      </div>
      {files !== null && (
        <div class="row">
          {files.length === 0 ? (
            <span class="muted small">exports 文件夹里还没有导出的 JSON 文件。可以把以前导出的文件放进 {'%APPDATA%\\DeskMate\\exports'} 再试。</span>
          ) : (
            <>
              <select aria-label="选择导入文件" value={file} onChange={(e) => setFile((e.currentTarget as HTMLSelectElement).value)}>
                {files.map((f) => (
                  <option key={f} value={f}>
                    {f}
                  </option>
                ))}
              </select>
              <button type="button" class="btn btn-primary" disabled={!file} onClick={doImport}>
                导入这个文件
              </button>
              <span class="muted small">重复的会跳过；密码等敏感内容不会导入</span>
            </>
          )}
        </div>
      )}
      <p class="muted small">清空会删除所有记忆和聊天记录（清空前会自动备份一份）。请输入「清空」两个字确认：</p>
      <div class="row">
        <input aria-label="清空确认" value={confirmText} onInput={(e) => setConfirmText((e.currentTarget as HTMLInputElement).value)} />
        <button
          type="button"
          class="btn btn-danger"
          disabled={confirmText.trim() !== '清空'}
          onClick={() => {
            if (confirm('真的要清空全部记忆和聊天记录吗？')) {
              run(() => api.memoryClearAll(confirmText), '已清空').then((ok) => {
                if (ok) {
                  setConfirmText('');
                  onCleared();
                }
              });
            }
          }}
        >
          清空全部
        </button>
      </div>
      <StatusLine status={status} />
    </div>
  );
}
