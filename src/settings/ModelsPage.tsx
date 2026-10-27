// 模型设置：选择聊天/后台模型，编辑每个配置，保存或清除 Key，测试连接。
// Key 只进不出：输入框保存后立即清空，界面上只显示末 4 位。
import { useState } from 'preact/hooks';
import type { SettingsApi } from '../lib/ipc';
import type { BackendConfig, BackendView, SettingsView, TestResult } from '../lib/types';
import { copyText } from '../lib/clipboard';
import { StatusLine, useStatus } from './SettingsApp';

interface Props {
  api: SettingsApi;
  view: SettingsView;
  onChanged: () => void;
}

export function ModelsPage({ api, view, onChanged }: Props) {
  const { status, run } = useStatus();
  const [addKind, setAddKind] = useState('');

  const select = (chat: string, bg: string) =>
    run(() => api.select(chat, bg)).then((ok) => ok && onChanged());

  const usable = (b: BackendView) => !b.needs_key || b.key_mask !== null;

  return (
    <section>
      <h1>模型</h1>
      <p class="muted">
        日常聊天建议选便宜、快的模型。API Key 只保存在 Windows 凭据管理器里，不会写进任何文件，也不会显示完整内容。
      </p>

      <div class="card">
        <div class="row">
          <span class="label">日常聊天使用</span>
          <select
            aria-label="日常聊天使用的模型"
            value={view.chat_backend}
            onChange={(e) => select((e.currentTarget as HTMLSelectElement).value, view.background_backend)}
          >
            {view.backends.map((b) => (
              <option key={b.config.id} value={b.config.id}>
                {b.config.name}
                {usable(b) ? '' : '（未填 Key）'}
              </option>
            ))}
          </select>
        </div>
        <div class="row">
          <span class="label">后台整理记忆使用</span>
          <select
            aria-label="后台整理记忆使用的模型"
            value={view.background_backend}
            onChange={(e) => select(view.chat_backend, (e.currentTarget as HTMLSelectElement).value)}
          >
            <option value="">和聊天相同</option>
            {view.backends.map((b) => (
              <option key={b.config.id} value={b.config.id}>
                {b.config.name}
              </option>
            ))}
          </select>
          <small class="muted">建议选最便宜的模型</small>
        </div>
        <StatusLine status={status} />
      </div>

      {view.backends.map((b) => (
        <BackendCard
          key={b.config.id}
          api={api}
          b={b}
          active={b.config.id === view.chat_backend}
          onChanged={onChanged}
        />
      ))}

      <div class="card row">
        <span class="label">添加一个配置</span>
        <select aria-label="新配置类型" value={addKind} onChange={(e) => setAddKind((e.currentTarget as HTMLSelectElement).value)}>
          <option value="">选择类型…</option>
          {view.kinds.map((k) => (
            <option key={k.kind} value={k.kind}>
              {k.display_name}
            </option>
          ))}
        </select>
        <button
          type="button"
          class="btn"
          disabled={!addKind}
          onClick={() => run(() => api.addBackend(addKind), '已添加').then((ok) => ok && onChanged())}
        >
          添加
        </button>
      </div>
    </section>
  );
}

interface CardProps {
  api: SettingsApi;
  b: BackendView;
  active: boolean;
  onChanged: () => void;
}

function BackendCard({ api, b, active, onChanged }: CardProps) {
  const [cfg, setCfg] = useState<BackendConfig>(b.config);
  const [key, setKey] = useState('');
  const [open, setOpen] = useState(active);
  const [testing, setTesting] = useState(false);
  const [test, setTest] = useState<TestResult | null>(null);
  const { status, run } = useStatus();
  const isMock = cfg.kind === 'mock';
  const dirty = JSON.stringify(cfg) !== JSON.stringify(b.config);

  const field = (k: keyof BackendConfig) => (e: Event) =>
    setCfg({ ...cfg, [k]: (e.currentTarget as HTMLInputElement).value });
  const price = (k: keyof BackendConfig['price']) => (e: Event) =>
    setCfg({ ...cfg, price: { ...cfg.price, [k]: Number((e.currentTarget as HTMLInputElement).value) || 0 } });

  const saveKey = async () => {
    // 先清空输入框，Key 不在界面上多停留
    const value = key;
    setKey('');
    const ok = await run(() => api.setKey(cfg.id, value), 'Key 已保存到 Windows 凭据管理器');
    if (ok) onChanged();
  };

  const runTest = async () => {
    setTesting(true);
    setTest(null);
    try {
      setTest(await api.test(cfg.id));
    } finally {
      setTesting(false);
    }
  };

  return (
    <div class={`card backend ${active ? 'active' : ''}`}>
      <button type="button" class="card-head" onClick={() => setOpen(!open)} aria-expanded={open}>
        <span class="title">{b.config.name}</span>
        {b.kind_name !== b.config.name && <span class="tag">{b.kind_name}</span>}
        {active && <span class="tag tag-accent">聊天使用中</span>}
        {b.needs_key && (b.key_mask ? <span class="tag tag-ok">Key {b.key_mask}</span> : <span class="tag tag-warn">未填 Key</span>)}
        <span class="chev">{open ? '▾' : '▸'}</span>
      </button>
      {open && (
        <div class="card-body">
          {b.note && <p class="muted">{b.note}</p>}
          {!isMock && (
            <>
              <div class="grid">
                <label>
                  名称
                  <input value={cfg.name} onInput={field('name')} maxLength={40} />
                </label>
                <label>
                  模型名
                  <input value={cfg.model} onInput={field('model')} spellcheck={false} />
                </label>
                <label class="span2">
                  接口地址
                  <input value={cfg.base_url} onInput={field('base_url')} spellcheck={false} />
                </label>
                <label>
                  思考强度
                  <input value={cfg.reasoning} onInput={field('reasoning')} placeholder="留空 = 接口默认" spellcheck={false} />
                </label>
              </div>
              <fieldset class="prices">
                <legend>价格（美元 / 每百万 token，用于估算花费）</legend>
                <label>
                  输入
                  <input type="number" min={0} step="0.001" value={cfg.price.input} onInput={price('input')} />
                </label>
                <label>
                  缓存命中
                  <input type="number" min={0} step="0.001" value={cfg.price.cached_input} onInput={price('cached_input')} />
                </label>
                <label>
                  缓存写入
                  <input type="number" min={0} step="0.001" value={cfg.price.cache_write} onInput={price('cache_write')} />
                </label>
                <label>
                  输出
                  <input type="number" min={0} step="0.001" value={cfg.price.output} onInput={price('output')} />
                </label>
              </fieldset>
              {cfg.price.input === 0 && cfg.price.output === 0 && (
                <p class="warn-text">价格为 0 时无法估算花费，花费上限对这个配置不起作用。</p>
              )}
              <div class="actions">
                <button
                  type="button"
                  class="btn btn-primary"
                  disabled={!dirty}
                  onClick={() => run(() => api.saveBackend(cfg)).then((ok) => ok && onChanged())}
                >
                  保存配置
                </button>
                {dirty && (
                  <button type="button" class="btn" onClick={() => setCfg(b.config)}>
                    撤销修改
                  </button>
                )}
                {b.removable && (
                  <button
                    type="button"
                    class="btn btn-danger"
                    onClick={() => {
                      if (confirm(`确定删除「${b.config.name}」吗？它的 Key 也会一起删除。`)) {
                        run(() => api.deleteBackend(cfg.id), '已删除').then((ok) => ok && onChanged());
                      }
                    }}
                  >
                    删除
                  </button>
                )}
              </div>
            </>
          )}

          {b.needs_key && (
            <div class="key-box">
              <div class="row">
                <span class="label">API Key</span>
                {b.key_mask ? <span class="mono">已保存 {b.key_mask}</span> : <span class="muted">还没有保存</span>}
                {b.key_mask && (
                  <button
                    type="button"
                    class="btn btn-small"
                    onClick={() => {
                      if (confirm('确定清除这个 Key 吗？')) run(() => api.clearKey(cfg.id), 'Key 已清除').then((ok) => ok && onChanged());
                    }}
                  >
                    清除
                  </button>
                )}
              </div>
              <div class="row">
                <input
                  type="password"
                  autocomplete="off"
                  spellcheck={false}
                  placeholder={b.key_mask ? '粘贴新的 Key 以替换' : '在这里粘贴 Key'}
                  value={key}
                  aria-label={`${b.config.name} 的 API Key`}
                  onInput={(e) => setKey((e.currentTarget as HTMLInputElement).value)}
                  onKeyDown={(e) => e.key === 'Enter' && key.trim() && saveKey()}
                />
                <button type="button" class="btn btn-primary" disabled={!key.trim()} onClick={saveKey}>
                  保存 Key
                </button>
              </div>
              {b.key_url && (
                <div class="row small">
                  <span class="muted">申请地址：</span>
                  <span class="mono selectable">{b.key_url}</span>
                  <button type="button" class="link-btn" onClick={() => copyText(b.key_url)}>
                    复制
                  </button>
                </div>
              )}
            </div>
          )}

          <div class="actions">
            <button type="button" class="btn" disabled={testing || (b.needs_key && !b.key_mask)} onClick={runTest}>
              {testing ? '测试中…' : '测试连接'}
            </button>
            {!isMock && <small class="muted">会发送一条很短的消息，产生极少量费用</small>}
          </div>
          {test && (
            <div class={`alert ${test.ok ? 'alert-ok' : 'alert-error'}`} role="status">
              {test.ok ? (
                <>
                  连接成功（{test.latency_ms} ms）：{test.reply || '（空回复）'}
                </>
              ) : (
                <>
                  <div>连接失败：{test.error_detail}</div>
                  {test.hint && <div class="hint">怎么办：{test.hint}</div>}
                </>
              )}
            </div>
          )}
          <StatusLine status={status} />
        </div>
      )}
    </div>
  );
}
