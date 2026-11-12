// 计划的编辑器：每周每天的时间、计划的参数、内容条目、推算预览。新建、重新安排、详情页都用这一套。
import { useState } from 'preact/hooks';
import type { Plan, PlanItem, PlanItemView, PlanMode, PlanPreview } from '../lib/types';
import { WEEK_MON, clampInt, dayLabel, emptyItem, mins, minsShort, relDay } from './util';

/** 周一到周日每天多少分钟（0 = 不排） */
export function WeekEditor({ value, onChange }: { value: number[]; onChange: (v: number[]) => void }) {
  const [all, setAll] = useState('');
  const set = (i: number, m: number) => onChange(value.map((x, k) => (k === i ? m : x)));
  return (
    <div class="week-editor">
      <div class="week-days">
        {WEEK_MON.map((d, i) => (
          <label key={d} class={value[i] > 0 ? 'on' : ''}>
            <span>周{d}</span>
            <input
              type="number"
              min={0}
              max={720}
              step={5}
              value={value[i]}
              onInput={(e) => set(i, clampInt(e.currentTarget.value || 0, 0, 720))}
              aria-label={`周${d}多少分钟`}
            />
          </label>
        ))}
      </div>
      <div class="row small">
        <span class="muted">分钟，0 = 那天不排。快速设置：</span>
        {[30, 60, 90, 120].map((m) => (
          <button type="button" key={m} class="chip" onClick={() => onChange(value.map(() => m))}>
            每天 {mins(m)}
          </button>
        ))}
        <button type="button" class="chip" onClick={() => onChange(value.map((x, i) => (i >= 5 ? 0 : x || 60)))}>
          周末不排
        </button>
        <span class="inline">
          <input
            type="number"
            min={0}
            max={720}
            step={5}
            placeholder="全部"
            class="num"
            value={all}
            onInput={(e) => setAll(e.currentTarget.value)}
            aria-label="每天都设成多少分钟"
          />
          <button
            type="button"
            class="chip"
            disabled={!all}
            onClick={() => {
              onChange(value.map(() => clampInt(all, 0, 720)));
              setAll('');
            }}
          >
            都设成这个
          </button>
        </span>
      </div>
    </div>
  );
}

/** 计划的参数：名字、类型、日期、每周时间、提醒时间、请假的日子。
 * `inGoal`：计划书里的一条线——内容线的时间由计划书统一分（这里只改名字、开始时间、监督），每周固定的事可以改每周几、多久、到哪天 */
export function PlanForm({ plan, onChange, today, inGoal = false }: { plan: Plan; onChange: (p: Plan) => void; today: string; inGoal?: boolean }) {
  const [off, setOff] = useState('');
  const up = (patch: Partial<Plan>) => onChange({ ...plan, ...patch });
  if (inGoal) {
    return (
      <div class="plan-form">
        <div class="grid">
          <label class="span2 title-row">
            名字
            <span class="inline">
              <input class="emoji-input" value={plan.emoji} maxLength={4} onInput={(e) => up({ emoji: e.currentTarget.value })} aria-label="图标" />
              <input class="grow" value={plan.title} maxLength={60} onInput={(e) => up({ title: e.currentTarget.value })} aria-label="计划的名字" />
            </span>
          </label>
          {plan.mode === 'habit' ? (
            <>
              <div class="span2">
                <div class="field-label">每周哪几天、每次多久</div>
                <WeekEditor value={plan.week_minutes} onChange={(w) => up({ week_minutes: w })} />
              </div>
              <label>
                做到哪天（可空）
                <input type="date" value={plan.end_day} min={plan.start_day} onInput={(e) => up({ end_day: e.currentTarget.value })} aria-label="截止日期" />
              </label>
            </>
          ) : (
            <p class="span2 muted small">这条线属于计划书：每天分多少时间由计划书按阶段的份额统一安排，想调就去计划书里点开阶段改份额。</p>
          )}
          <label>
            每天几点开始（空 = 跟计划书）
            <input type="time" value={plan.remind_at} onInput={(e) => up({ remind_at: e.currentTarget.value })} aria-label="每天几点开始" />
          </label>
          <label class="check-label">
            <input type="checkbox" checked={plan.supervise} onChange={(e) => up({ supervise: e.currentTarget.checked })} />
            让她提醒、监督这条线
          </label>
        </div>
      </div>
    );
  }
  return (
    <div class="plan-form">
      <div class="grid">
        <label class="span2 title-row">
          名字
          <span class="inline">
            <input class="emoji-input" value={plan.emoji} maxLength={4} onInput={(e) => up({ emoji: e.currentTarget.value })} aria-label="图标" />
            <input class="grow" value={plan.title} maxLength={60} onInput={(e) => up({ title: e.currentTarget.value })} aria-label="计划的名字" />
          </span>
        </label>
        <div class="span2">
          <div class="seg left" role="radiogroup" aria-label="计划类型">
            {(
              [
                ['content', '有总量（做完就结束）'],
                ['habit', '每天都做（习惯 / 打卡）'],
              ] as [PlanMode, string][]
            ).map(([m, label]) => (
              <button type="button" key={m} role="radio" aria-checked={plan.mode === m} class={plan.mode === m ? 'on' : ''} onClick={() => up({ mode: m })}>
                {label}
              </button>
            ))}
          </div>
        </div>
        <label>
          从哪天开始
          <input type="date" value={plan.start_day} onInput={(e) => up({ start_day: e.currentTarget.value || today })} aria-label="开始日期" />
        </label>
        <label>
          {plan.mode === 'habit' ? '做到哪天（可空）' : '截止（可空）'}
          <span class="inline">
            <input type="date" value={plan.end_day} min={plan.start_day} onInput={(e) => up({ end_day: e.currentTarget.value })} aria-label="截止日期" />
            {plan.end_day && (
              <button type="button" class="link-btn" onClick={() => up({ end_day: '' })}>
                不设
              </button>
            )}
          </span>
        </label>
        {plan.mode === 'content' && plan.end_day && (
          <div class="span2">
            <div class="seg left" role="radiogroup" aria-label="节奏">
              <button type="button" role="radio" aria-checked={plan.pace === 'even'} class={plan.pace === 'even' ? 'on' : ''} onClick={() => up({ pace: 'even' })}>
                均匀排到截止
              </button>
              <button type="button" role="radio" aria-checked={plan.pace === 'asap'} class={plan.pace === 'asap' ? 'on' : ''} onClick={() => up({ pace: 'asap' })}>
                尽快做完
              </button>
            </div>
          </div>
        )}
        <div class="span2">
          <div class="field-label">每周哪几天、每天多久</div>
          <WeekEditor value={plan.week_minutes} onChange={(w) => up({ week_minutes: w })} />
        </div>
        <label>
          每天几点开始（可空）
          <span class="inline">
            <input type="time" value={plan.remind_at} onInput={(e) => up({ remind_at: e.currentTarget.value })} aria-label="每天几点开始" />
            {plan.remind_at && (
              <button type="button" class="link-btn" onClick={() => up({ remind_at: '' })}>
                不固定
              </button>
            )}
          </span>
        </label>
        <label class="check-label">
          <input type="checkbox" checked={plan.supervise} onChange={(e) => up({ supervise: e.currentTarget.checked })} />
          让她提醒、监督这个计划
        </label>
        <div class="span2">
          <div class="field-label">请假的日子（这几天不排）</div>
          <div class="row">
            {plan.off_days.map((d) => (
              <span class="chip on" key={d}>
                {dayLabel(d)}
                <button type="button" class="link-btn" aria-label={`取消 ${d} 请假`} onClick={() => up({ off_days: plan.off_days.filter((x) => x !== d) })}>
                  ×
                </button>
              </span>
            ))}
            <span class="inline">
              <input type="date" min={today} value={off} onInput={(e) => setOff(e.currentTarget.value)} aria-label="请假的日子" />
              <button
                type="button"
                class="chip"
                disabled={!off}
                onClick={() => {
                  if (!plan.off_days.includes(off)) up({ off_days: [...plan.off_days, off].sort() });
                  setOff('');
                }}
              >
                加上
              </button>
            </span>
          </div>
        </div>
      </div>
    </div>
  );
}

/** 内容条目：改名字、估时、顺序、标完成、加删；还能一次粘贴好几条 */
export function ItemsEditor({
  items,
  mode,
  onChange,
}: {
  items: (PlanItem | PlanItemView)[];
  mode: PlanMode;
  onChange: (items: PlanItem[]) => void;
}) {
  const [bulk, setBulk] = useState<string | null>(null);
  const set = (i: number, patch: Partial<PlanItem>) => onChange(items.map((x, k) => (k === i ? { ...x, ...patch } : x)));
  const move = (i: number, d: number) => {
    const j = i + d;
    if (j < 0 || j >= items.length) return;
    const v = [...items];
    [v[i], v[j]] = [v[j], v[i]];
    onChange(v);
  };
  const total = items.filter((i) => i.status === 'todo').reduce((s, i) => s + (i.minutes || 0), 0);
  return (
    <div class="items-editor">
      {items.length === 0 && (
        <p class="muted small">
          {mode === 'habit' ? '只做一件事的话可以不加；有好几项（比如单词、听力）就一条条加上，时间够就每天都做，不够就轮流。' : '还没有内容。'}
        </p>
      )}
      <ol class="plain item-list">
        {items.map((it, i) => {
          const v = it as PlanItemView;
          const spent = v.spent ?? 0;
          return (
            <li key={it.id || `n${i}`} class={`item item-${it.status}`}>
              <input
                type="checkbox"
                checked={it.status === 'done'}
                onChange={(e) => set(i, { status: e.currentTarget.checked ? 'done' : 'todo' })}
                aria-label={`做完了：${it.title}`}
                title="做完了"
              />
              <div class="item-main">
                <input class="item-title" value={it.title} maxLength={100} onInput={(e) => set(i, { title: e.currentTarget.value })} aria-label={`第 ${i + 1} 条的名字`} />
                <input
                  class="item-detail"
                  value={it.detail}
                  maxLength={1000}
                  placeholder="说明（可空）"
                  onInput={(e) => set(i, { detail: e.currentTarget.value })}
                  aria-label={`第 ${i + 1} 条的说明`}
                />
                {spent > 0 && it.status === 'todo' && mode === 'content' && (
                  <div class="mini-bar" title={`已经做了 ${mins(spent)}`}>
                    <div style={{ width: `${Math.min(100, (spent / Math.max(1, it.minutes)) * 100)}%` }} />
                  </div>
                )}
              </div>
              <span class="inline item-min">
                <input
                  type="number"
                  min={0}
                  max={6000}
                  step={5}
                  value={it.minutes}
                  onInput={(e) => set(i, { minutes: clampInt(e.currentTarget.value || 0, 0, 6000) })}
                  aria-label={`第 ${i + 1} 条要多少分钟`}
                />
                <small class="muted">分</small>
              </span>
              <span class="item-tools">
                <button type="button" class="icon-btn" aria-label="往上挪" disabled={i === 0} onClick={() => move(i, -1)}>
                  ↑
                </button>
                <button type="button" class="icon-btn" aria-label="往下挪" disabled={i === items.length - 1} onClick={() => move(i, 1)}>
                  ↓
                </button>
                <button type="button" class="icon-btn danger" aria-label={`删掉：${it.title}`} onClick={() => onChange(items.filter((_, k) => k !== i))}>
                  ×
                </button>
              </span>
            </li>
          );
        })}
      </ol>
      <div class="row">
        <button type="button" class="btn btn-small" onClick={() => onChange([...items, emptyItem(`第 ${items.length + 1} 部分`)])}>
          加一条
        </button>
        <button type="button" class="btn btn-small" onClick={() => setBulk(bulk === null ? '' : null)}>
          一次加好几条
        </button>
        {mode === 'content' && total > 0 && <span class="muted small">还要做的加起来约 {mins(total)}</span>}
      </div>
      {bulk !== null && (
        <div class="bulk">
          <textarea
            rows={4}
            placeholder={'一行一条，可以在后面写时间，比如：\n第4章 特征值 2小时\n第5章 二次型 90分钟'}
            value={bulk}
            onInput={(e) => setBulk(e.currentTarget.value)}
            aria-label="一行一条"
          />
          <button
            type="button"
            class="btn btn-small"
            disabled={!bulk.trim()}
            onClick={() => {
              onChange([...items, ...bulk.split('\n').map(lineToItem).filter((x): x is PlanItem => !!x)]);
              setBulk(null);
            }}
          >
            加进去
          </button>
        </div>
      )}
    </div>
  );
}

/** 「第4章 特征值 2小时」→ 条目（时长写在行尾：2小时、1.5h、90分钟、90min） */
export function lineToItem(line: string): PlanItem | null {
  let t = line.trim().replace(/^([-*•·]|\d+[.、)）]|\(\d+\))\s*/, '');
  if (!t) return null;
  let minutes = 60;
  const m = /[（(\s]*(?:约|大概)?\s*(\d+(?:\.\d+)?)\s*(小时|h|hr|个小时|分钟|分|min)[)）]?\s*$/i.exec(t);
  if (m) {
    const v = Number(m[1]);
    minutes = /分|min/i.test(m[2]) ? Math.round(v) : Math.round(v * 60);
    t = t.slice(0, m.index).trim();
  }
  if (!t) return null;
  return { ...emptyItem(t.slice(0, 100)), minutes: Math.max(5, Math.min(6000, minutes)) };
}

/** 推算出来的安排：预计哪天做完、来不及差多少、前两周每天做什么 */
export function PreviewCard({ preview, plan, today }: { preview: PlanPreview; plan: Plan; today: string }) {
  return (
    <div class="preview">
      <div class="preview-sum">
        <span>{preview.per_week}</span>
        {plan.mode === 'content' && preview.total_minutes > 0 && <span>内容共约 {mins(preview.total_minutes)}</span>}
        {plan.mode === 'content' && preview.finish && (
          <span>
            预计 <b>{dayLabel(preview.finish)}</b> 做完
          </span>
        )}
        {plan.mode === 'content' && !preview.finish && preview.total_minutes > 0 && <span class="warn-text">一年内做不完，时间是不是给少了？</span>}
      </div>
      {preview.short_minutes > 0 && (
        <div class="alert alert-error">到截止还差大约 {mins(preview.short_minutes)}：可以多给点时间、把截止往后挪，或者删掉一些内容。</div>
      )}
      <ul class="plain preview-days">
        {preview.days.map(([d, slots]) => (
          <li key={d}>
            <span class="pd-day">{relDay(d, today)}</span>
            <span class="pd-items">
              {slots.map(([t, m], i) => (
                <span key={i} class="pd-item">
                  {t}
                  <small> {minsShort(m)}</small>
                </span>
              ))}
            </span>
          </li>
        ))}
      </ul>
      {preview.days.length === 0 && <p class="muted small">按现在的设置，前两周没有可排的日子。</p>}
    </div>
  );
}
