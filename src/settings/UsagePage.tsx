import { useEffect, useState } from 'preact/hooks';
import type { SettingsApi } from '../lib/ipc';
import type { Budget, UsageRow, UsageView } from '../lib/types';
import { StatusLine, useStatus } from './SettingsApp';

const usd = (v: number) => `$${v < 0.01 && v > 0 ? v.toFixed(4) : v.toFixed(2)}`;
const num = (v: number) => v.toLocaleString('zh-CN');

export function UsagePage({ api }: { api: SettingsApi }) {
  const [u, setU] = useState<UsageView | null>(null);
  const [budget, setBudget] = useState<Budget | null>(null);
  const [range, setRange] = useState<'today' | 'month' | 'all'>('month');
  const { status, run } = useStatus();

  const load = async () => {
    const v = await api.usage();
    setU(v);
    setBudget(v.budget);
  };
  useEffect(() => {
    load().catch(() => {});
  }, []);

  if (!u || !budget) return <div class="muted">加载中…</div>;
  const rows = range === 'today' ? u.rows_today : range === 'month' ? u.rows_month : u.rows_all;

  return (
    <section>
      <h1>用量与花费</h1>
      <p class="muted">费用是按「设置 → 模型」里填写的价格估算的，实际以服务商账单为准。mock 模式不花钱。</p>
      <div class="meters">
        <Meter label={`今天（${u.today}）`} spent={u.spent_today} limit={u.budget.daily_usd} />
        <Meter label={`本月（${u.month}）`} spent={u.spent_month} limit={u.budget.monthly_usd} />
      </div>

      <div class="card">
        <h2>花费上限</h2>
        <p class="muted">到达上限后角色会提示你，并停止调用模型。填 0 表示不限制。</p>
        <div class="row">
          <label>
            每日上限（美元）
            <input
              type="number"
              min={0}
              step="0.1"
              value={budget.daily_usd}
              onInput={(e) => setBudget({ ...budget, daily_usd: Number((e.currentTarget as HTMLInputElement).value) || 0 })}
            />
          </label>
          <label>
            每月上限（美元）
            <input
              type="number"
              min={0}
              step="1"
              value={budget.monthly_usd}
              onInput={(e) => setBudget({ ...budget, monthly_usd: Number((e.currentTarget as HTMLInputElement).value) || 0 })}
            />
          </label>
          <button type="button" class="btn btn-primary" onClick={() => run(() => api.saveBudget(budget)).then(() => load())}>
            保存
          </button>
          <StatusLine status={status} />
        </div>
      </div>

      <div class="card">
        <div class="row">
          <h2>明细</h2>
          <div class="seg" role="tablist">
            {(['today', 'month', 'all'] as const).map((r) => (
              <button type="button" key={r} class={range === r ? 'on' : ''} onClick={() => setRange(r)} role="tab" aria-selected={range === r}>
                {r === 'today' ? '今天' : r === 'month' ? '本月' : '全部'}
              </button>
            ))}
          </div>
        </div>
        <UsageTable rows={rows} />
      </div>
    </section>
  );
}

function Meter({ label, spent, limit }: { label: string; spent: number; limit: number }) {
  const pct = limit > 0 ? Math.min(100, (spent / limit) * 100) : 0;
  return (
    <div class="card meter">
      <div class="muted">{label}</div>
      <div class="big">{usd(spent)}</div>
      <div class="bar" aria-hidden="true">
        <div class={`fill ${pct >= 100 ? 'full' : pct >= 80 ? 'high' : ''}`} style={{ width: `${pct}%` }} />
      </div>
      <div class="muted small">{limit > 0 ? `上限 ${usd(limit)}` : '不限制'}</div>
    </div>
  );
}

function UsageTable({ rows }: { rows: UsageRow[] }) {
  if (rows.length === 0) return <div class="muted">还没有记录</div>;
  return (
    <table class="table">
      <thead>
        <tr>
          <th>配置</th>
          <th>模型</th>
          <th class="num">次数</th>
          <th class="num">输入 token</th>
          <th class="num">其中缓存</th>
          <th class="num">输出 token</th>
          <th class="num">估算费用</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((r) => (
          <tr key={`${r.backend_id}/${r.model}`}>
            <td>{r.backend_id}</td>
            <td class="mono">{r.model}</td>
            <td class="num">{num(r.requests)}</td>
            <td class="num">{num(r.input_tokens)}</td>
            <td class="num">{num(r.cached_input_tokens)}</td>
            <td class="num">{num(r.output_tokens)}</td>
            <td class="num">{usd(r.cost_usd)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
