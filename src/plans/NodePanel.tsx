// 图上选中的东西在这里看、改：阶段、线、某条线在某个阶段要做的、节点、缓冲期；没选时可以加阶段 / 节点 / 缓冲 / 线。
// 改的是一份 GoalFull（草稿或者已保存计划书的副本），由外面决定什么时候保存。
import { useState } from 'preact/hooks';
import type { GoalFull, GoalMilestone, GoalPhase, GoalViz, MilestoneStatus, PlanItem } from '../lib/types';
import { ItemsEditor, WeekEditor } from './editors';
import type { Sel } from './GoalViz';
import { MS_STATUS, isRoutine, laneColor, rangeLabel, sharePct, tmpId } from './goalUtil';
import { addDays, clampInt, dayLabel, emptyPlan, mins, relDay } from './util';

export interface NodePanelProps {
  full: GoalFull;
  viz: GoalViz;
  sel: Sel | null;
  today: string;
  onSel: (s: Sel | null) => void;
  /** 改了；`items` = 动了内容条目 */
  onChange: (full: GoalFull, items?: boolean) => void;
  /** 已经保存的计划书：打开某条线的详情页 */
  onOpenTrack?: (id: number) => void;
}

function lines(v: string): string[] {
  return v
    .split('\n')
    .map((s) => s.trim())
    .filter(Boolean);
}

function PhaseEditor({ full, phase, onChange, onSel, today }: { full: GoalFull; phase: GoalPhase; onChange: NodePanelProps['onChange']; onSel: NodePanelProps['onSel']; today: string }) {
  const [crit, setCrit] = useState(phase.criteria.join('\n'));
  const up = (patch: Partial<GoalPhase>) => onChange({ ...full, phases: full.phases.map((p) => (p.id === phase.id ? { ...p, ...patch } : p)) });
  const content = full.tracks.filter((t) => !isRoutine(t.plan));
  const pct = sharePct(phase, content.map((t) => t.plan));
  const setShare = (id: number, v: number) => {
    // 第一次改份额：没写的线按各自的默认份额补上
    const base: Record<string, number> = Object.keys(phase.shares).length ? { ...phase.shares } : Object.fromEntries(content.map((t) => [String(t.plan.id), t.plan.share ?? 1]));
    base[String(id)] = v;
    up({ shares: base });
  };
  const bad = phase.end_day < phase.start_day;
  return (
    <div class="node-form">
      <div class="grid">
        <label class="span2">
          阶段的名字
          <input value={phase.title} maxLength={60} onInput={(e) => up({ title: e.currentTarget.value })} aria-label="阶段的名字" />
        </label>
        <label>
          从
          <input type="date" value={phase.start_day} onInput={(e) => up({ start_day: e.currentTarget.value || phase.start_day })} aria-label="阶段开始" />
        </label>
        <label>
          到
          <input type="date" value={phase.end_day} min={phase.start_day} onInput={(e) => up({ end_day: e.currentTarget.value || phase.end_day })} aria-label="阶段结束" />
        </label>
        {bad && <div class="span2 warn-text">结束比开始还早。</div>}
        <label class="span2">
          这个阶段的目标
          <textarea rows={2} maxLength={500} value={phase.objective} onInput={(e) => up({ objective: e.currentTarget.value })} aria-label="阶段目标" />
        </label>
        <label class="span2">
          完成标准（一行一条，能检验的）
          <textarea
            rows={3}
            value={crit}
            onInput={(e) => {
              setCrit(e.currentTarget.value);
              up({ criteria: lines(e.currentTarget.value) });
            }}
            aria-label="完成标准"
          />
        </label>
        <div class="span2">
          <label class="check-label">
            <input type="checkbox" checked={phase.week_minutes !== null} onChange={(e) => up({ week_minutes: e.currentTarget.checked ? [...full.goal.week_minutes] : null })} />
            这个阶段每天的时间和平时不一样
          </label>
          {phase.week_minutes && <WeekEditor value={phase.week_minutes} onChange={(w) => up({ week_minutes: w })} />}
        </div>
        {content.length > 0 && (
          <div class="span2">
            <div class="field-label">这个阶段各条线分多少时间（份额，0 = 这段时间不做）</div>
            <div class="shares">
              {content.map((t) => {
                const i = full.tracks.indexOf(t);
                const v = Object.keys(phase.shares).length ? (phase.shares[String(t.plan.id)] ?? 0) : (t.plan.share ?? 1);
                return (
                  <label key={t.plan.id} class="share">
                    <i class="sw" style={{ background: laneColor(i) }} />
                    <span class="grow">{t.plan.title}</span>
                    <input type="number" min={0} max={100} value={v} onInput={(e) => setShare(t.plan.id, clampInt(e.currentTarget.value || 0, 0, 100))} aria-label={`${t.plan.title}的份额`} />
                    <span class="muted small pct">{pct.get(t.plan.id) ?? 0}%</span>
                  </label>
                );
              })}
            </div>
          </div>
        )}
      </div>
      <div class="row small">
        <span class="muted">
          {rangeLabel(phase.start_day, phase.end_day)}
          {today >= phase.start_day && today <= phase.end_day ? ' · 现在' : ''}
        </span>
        <span class="grow" />
        <button
          type="button"
          class="link-btn danger"
          onClick={() => {
            onChange(
              {
                ...full,
                phases: full.phases.filter((p) => p.id !== phase.id),
                tracks: full.tracks.map((t) => ({ ...t, items: t.items.map((i) => (i.phase_id === phase.id ? { ...i, phase_id: null } : i)) })),
                milestones: full.milestones.map((m) => (m.phase_id === phase.id ? { ...m, phase_id: null } : m)),
              },
              true,
            );
            onSel(null);
          }}
        >
          删掉这个阶段（里面的内容留着，变成不分阶段的）
        </button>
      </div>
    </div>
  );
}

/** 某条线在某个阶段要做的（phase = null：不属于阶段的） */
function CellEditor({ full, phase, track, onChange, highlight }: { full: GoalFull; phase: number | null; track: number; onChange: NodePanelProps['onChange']; highlight?: number }) {
  const t = full.tracks.find((x) => x.plan.id === track);
  if (!t) return null;
  const ph = full.phases.find((p) => p.id === phase) ?? null;
  const inCell = (i: PlanItem) => (i.phase_id ?? null) === phase;
  const mine = t.items.filter(inCell);
  const setItems = (next: PlanItem[]) => {
    const fixed = next.map((i) => ({ ...i, id: i.id || tmpId(), phase_id: phase }));
    const out: PlanItem[] = [];
    let placed = false;
    for (const i of t.items) {
      if (inCell(i)) {
        if (!placed) {
          out.push(...fixed);
          placed = true;
        }
      } else out.push(i);
    }
    if (!placed) out.push(...fixed);
    onChange({ ...full, tracks: full.tracks.map((x) => (x.plan.id === track ? { ...x, items: out } : x)) }, true);
  };
  const rough = mine.filter((i) => i.rough && i.status === 'todo').length;
  return (
    <div class="node-form">
      {ph && (
        <p class="muted small">
          {rangeLabel(ph.start_day, ph.end_day)}
          {ph.objective ? ` · 目标：${ph.objective}` : ''}
        </p>
      )}
      {rough > 0 && <p class="small hint-line">有 {rough} 块还是粗的：快排到的时候会按说明里用「、」隔开的要点自动拆开；想让她按进度细化，点上面的「调整下周」。</p>}
      <ItemsEditor items={mine} mode="content" onChange={setItems} />
      {highlight !== undefined && <p class="muted small">（从树状图选的是「{t.items.find((i) => i.id === highlight)?.title}」）</p>}
    </div>
  );
}

function TrackEditor({ full, id, onChange, onSel, onOpenTrack }: { full: GoalFull; id: number; onChange: NodePanelProps['onChange']; onSel: NodePanelProps['onSel']; onOpenTrack?: (id: number) => void }) {
  const [confirm, setConfirm] = useState(false);
  const t = full.tracks.find((x) => x.plan.id === id);
  if (!t) return null;
  const p = t.plan;
  const routine = isRoutine(p);
  const up = (patch: Partial<typeof p>) => onChange({ ...full, tracks: full.tracks.map((x) => (x.plan.id === id ? { ...x, plan: { ...x.plan, ...patch } } : x)) });
  const saved = p.id > 0;
  return (
    <div class="node-form">
      <div class="grid">
        <label class="span2 title-row">
          名字
          <span class="inline">
            <input class="emoji-input" value={p.emoji} maxLength={4} onInput={(e) => up({ emoji: e.currentTarget.value })} aria-label="线的图标" />
            <input class="grow" value={p.title} maxLength={60} onInput={(e) => up({ title: e.currentTarget.value })} aria-label="线的名字" />
          </span>
        </label>
        {routine ? (
          <>
            <div class="span2">
              <div class="field-label">每周哪几天、每次多久</div>
              <WeekEditor value={p.week_minutes} onChange={(w) => up({ week_minutes: w })} />
            </div>
            <label>
              做到哪天（可空）
              <input type="date" value={p.end_day} min={full.goal.start_day} onInput={(e) => up({ end_day: e.currentTarget.value })} aria-label="做到哪天" />
            </label>
          </>
        ) : (
          <label>
            默认份额（阶段里没写时用）
            <input type="number" min={0} max={100} value={p.share ?? 1} onInput={(e) => up({ share: clampInt(e.currentTarget.value || 0, 0, 100) })} aria-label="默认份额" />
          </label>
        )}
        <label>
          几点开始（空 = 跟计划书）
          <input type="time" value={p.remind_at} onInput={(e) => up({ remind_at: e.currentTarget.value })} aria-label="这条线几点开始" />
        </label>
        <label class="check-label span2">
          <input type="checkbox" checked={p.supervise} onChange={(e) => up({ supervise: e.currentTarget.checked })} />
          让她提醒、监督这条线
        </label>
      </div>
      {!routine && (
        <>
          <div class="field-label">这条线的全部内容（按顺序；截止早的会先排）</div>
          <ItemsEditor
            items={t.items}
            mode="content"
            onChange={(items) => onChange({ ...full, tracks: full.tracks.map((x) => (x.plan.id === id ? { ...x, items: items.map((i) => ({ ...i, id: i.id || tmpId() })) } : x)) }, true)}
          />
        </>
      )}
      <div class="row small">
        {saved && onOpenTrack && (
          <button type="button" class="btn btn-small" onClick={() => onOpenTrack(id)}>
            打开这条线（日历、备注）
          </button>
        )}
        <span class="grow" />
        {!confirm ? (
          <button type="button" class="link-btn danger" onClick={() => (saved ? setConfirm(true) : (onChange({ ...full, tracks: full.tracks.filter((x) => x.plan.id !== id) }, true), onSel(null)))}>
            删掉这条线
          </button>
        ) : (
          <>
            <span>保存后这条线做过的记录也会删掉，确定吗？</span>
            <button
              type="button"
              class="btn btn-small btn-danger"
              onClick={() => {
                onChange({ ...full, tracks: full.tracks.filter((x) => x.plan.id !== id) }, true);
                onSel(null);
              }}
            >
              确定
            </button>
            <button type="button" class="btn btn-small" onClick={() => setConfirm(false)}>
              不删了
            </button>
          </>
        )}
      </div>
    </div>
  );
}

function MilestoneEditor({ full, m, onChange, onSel }: { full: GoalFull; m: GoalMilestone; onChange: NodePanelProps['onChange']; onSel: NodePanelProps['onSel'] }) {
  const up = (patch: Partial<GoalMilestone>) => onChange({ ...full, milestones: full.milestones.map((x) => (x.id === m.id ? { ...x, ...patch } : x)) });
  return (
    <div class="node-form">
      <div class="grid">
        <label class="span2">
          节点
          <input value={m.title} maxLength={80} onInput={(e) => up({ title: e.currentTarget.value })} aria-label="节点的名字" />
        </label>
        <label>
          哪天
          <input type="date" value={m.day} onInput={(e) => up({ day: e.currentTarget.value || m.day })} aria-label="节点的日期" />
        </label>
        <label>
          属于哪条线
          <select value={m.plan_id ?? ''} onChange={(e) => up({ plan_id: e.currentTarget.value ? Number(e.currentTarget.value) : null })} aria-label="属于哪条线">
            <option value="">整个计划书</option>
            {full.tracks.map((t) => (
              <option key={t.plan.id} value={t.plan.id}>
                {t.plan.title}
              </option>
            ))}
          </select>
        </label>
        <label class="span2">
          怎么算达标
          <textarea rows={2} maxLength={500} value={m.criteria} onInput={(e) => up({ criteria: e.currentTarget.value })} aria-label="达标标准" />
        </label>
        <div class="span2">
          <div class="field-label">达标了吗</div>
          <div class="seg left" role="radiogroup" aria-label="节点状态">
            {(['todo', 'passed', 'missed', 'skipped'] as MilestoneStatus[]).map((s) => (
              <button type="button" key={s} role="radio" aria-checked={m.status === s} class={m.status === s ? 'on' : ''} onClick={() => up({ status: s })}>
                {MS_STATUS[s]}
              </button>
            ))}
          </div>
        </div>
        <label class="span2">
          结果 / 心得
          <textarea rows={2} maxLength={1000} value={m.result} onInput={(e) => up({ result: e.currentTarget.value })} aria-label="结果" />
        </label>
      </div>
      {m.status === 'missed' && <p class="small hint-line">没达标也没关系：可以把节点往后挪一挪、给相关的线多分点份额，或者点「调整下周」让她按现在的进度重新排。</p>}
      <div class="row small">
        <span class="muted">{dayLabel(m.day)}</span>
        <span class="grow" />
        <button
          type="button"
          class="link-btn danger"
          onClick={() => {
            onChange({ ...full, milestones: full.milestones.filter((x) => x.id !== m.id) });
            onSel(null);
          }}
        >
          删掉这个节点
        </button>
      </div>
    </div>
  );
}

function BufferEditor({ full, index, onChange, onSel }: { full: GoalFull; index: number; onChange: NodePanelProps['onChange']; onSel: NodePanelProps['onSel'] }) {
  const b = full.goal.buffers[index];
  if (!b) return null;
  const up = (patch: Partial<typeof b>) => onChange({ ...full, goal: { ...full.goal, buffers: full.goal.buffers.map((x, i) => (i === index ? { ...x, ...patch } : x)) } });
  return (
    <div class="node-form">
      <div class="grid">
        <label>
          从
          <input type="date" value={b.start} onInput={(e) => up({ start: e.currentTarget.value || b.start })} aria-label="缓冲开始" />
        </label>
        <label>
          到
          <input type="date" value={b.end} min={b.start} onInput={(e) => up({ end: e.currentTarget.value || b.end })} aria-label="缓冲结束" />
        </label>
        <label>
          这几天
          <select value={b.percent} onChange={(e) => up({ percent: Number(e.currentTarget.value) })} aria-label="这几天排多少">
            <option value={0}>不排</option>
            <option value={30}>按平时的 30%</option>
            <option value={50}>按平时的一半</option>
            <option value={70}>按平时的 70%</option>
          </select>
        </label>
        <label>
          原因
          <input value={b.note} maxLength={100} placeholder="考试周、答辩、出门…" onInput={(e) => up({ note: e.currentTarget.value })} aria-label="缓冲的原因" />
        </label>
      </div>
      <div class="row small">
        <span class="muted">每周固定的事在「不排」的日子也停。</span>
        <span class="grow" />
        <button
          type="button"
          class="link-btn danger"
          onClick={() => {
            onChange({ ...full, goal: { ...full.goal, buffers: full.goal.buffers.filter((_, i) => i !== index) } });
            onSel(null);
          }}
        >
          删掉这段缓冲
        </button>
      </div>
    </div>
  );
}

/** 没选东西时：加阶段、节点、缓冲、线 */
function AddBar({ full, onChange, onSel, today }: { full: GoalFull; onChange: NodePanelProps['onChange']; onSel: NodePanelProps['onSel']; today: string }) {
  const g = full.goal;
  const lastEnd = full.phases.reduce((m, p) => (p.end_day > m ? p.end_day : m), '');
  const start = lastEnd ? addDays(lastEnd, 1) : g.start_day;
  const addPhase = () => {
    const id = tmpId();
    const end = g.end_day && g.end_day > start ? (addDays(start, 29) < g.end_day ? addDays(start, 29) : g.end_day) : addDays(start, 29);
    onChange({ ...full, phases: [...full.phases, { id, goal_id: g.id, title: `阶段${full.phases.length + 1}`, start_day: start, end_day: end, objective: '', criteria: [], week_minutes: null, shares: {}, sort: full.phases.length }] });
    onSel({ kind: 'phase', id });
  };
  const addMilestone = () => {
    const id = tmpId();
    const day = addDays(today > g.start_day ? today : g.start_day, 14);
    onChange({ ...full, milestones: [...full.milestones, { id, goal_id: g.id, phase_id: null, plan_id: null, title: '新的节点', day, criteria: '', status: 'todo', result: '', checked_at: null, sort: full.milestones.length }] });
    onSel({ kind: 'milestone', id });
  };
  const addBuffer = () => {
    const s = addDays(today > g.start_day ? today : g.start_day, 7);
    onChange({ ...full, goal: { ...g, buffers: [...g.buffers, { start: s, end: addDays(s, 6), percent: 50, note: '' }] } });
    onSel({ kind: 'buffer', index: g.buffers.length });
  };
  const addTrack = (routine: boolean) => {
    const id = tmpId();
    const plan = {
      ...emptyPlan(g.start_day),
      id,
      title: routine ? '每周固定的事' : `新的线`,
      emoji: routine ? '🔁' : '📌',
      mode: routine ? ('habit' as const) : ('content' as const),
      week_minutes: routine ? [0, 0, 0, 0, 0, 0, 30] : [...g.week_minutes],
      end_day: routine ? '' : g.end_day,
      goal_id: g.id > 0 ? g.id : null,
      share: routine ? 0 : 1,
    };
    onChange({ ...full, tracks: [...full.tracks, { plan, items: [] }] }, true);
    onSel({ kind: 'track', id });
  };
  return (
    <div class="row add-bar">
      <button type="button" class="btn btn-small" onClick={addPhase}>
        ＋ 阶段
      </button>
      <button type="button" class="btn btn-small" onClick={addMilestone}>
        ＋ 节点
      </button>
      <button type="button" class="btn btn-small" onClick={addBuffer}>
        ＋ 缓冲期
      </button>
      <button type="button" class="btn btn-small" disabled={full.tracks.length >= 10} onClick={() => addTrack(false)}>
        ＋ 一条线
      </button>
      <button type="button" class="btn btn-small" disabled={full.tracks.length >= 10} onClick={() => addTrack(true)}>
        ＋ 每周固定的事
      </button>
    </div>
  );
}

export function NodePanel(props: NodePanelProps) {
  const { full, viz, sel, today, onSel, onChange, onOpenTrack } = props;
  let title = '';
  let body = null;
  if (sel?.kind === 'phase') {
    const p = full.phases.find((x) => x.id === sel.id);
    const pv = viz.phases.find((x) => x.phase.id === sel.id);
    if (p) {
      title = `阶段：${p.title}${pv ? `（${pv.percent}%）` : ''}`;
      body = <PhaseEditor key={p.id} full={full} phase={p} onChange={onChange} onSel={onSel} today={today} />;
    }
  } else if (sel?.kind === 'cell' || sel?.kind === 'item') {
    const track = sel.track;
    const phase = sel.kind === 'cell' ? sel.phase : (full.tracks.find((t) => t.plan.id === track)?.items.find((i) => i.id === sel.id)?.phase_id ?? null);
    const t = full.tracks.find((x) => x.plan.id === track);
    const p = full.phases.find((x) => x.id === phase);
    if (t) {
      title = `${t.plan.title} · ${p ? p.title : '不分阶段'}`;
      body = <CellEditor key={`${phase}-${track}`} full={full} phase={phase} track={track} onChange={onChange} highlight={sel.kind === 'item' ? sel.id : undefined} />;
    }
  } else if (sel?.kind === 'track') {
    const t = full.tracks.find((x) => x.plan.id === sel.id);
    const tv = viz.tracks.find((x) => x.plan_id === sel.id);
    if (t) {
      title = `${isRoutine(t.plan) ? '每周固定的事' : '线'}：${t.plan.title}${tv && !tv.routine ? `（${tv.percent}%${tv.finish ? `，预计 ${relDay(tv.finish, today)}做完` : ''}）` : ''}`;
      body = <TrackEditor key={t.plan.id} full={full} id={t.plan.id} onChange={onChange} onSel={onSel} onOpenTrack={onOpenTrack} />;
    }
  } else if (sel?.kind === 'milestone') {
    const m = full.milestones.find((x) => x.id === sel.id);
    if (m) {
      title = `节点：${m.title}`;
      body = <MilestoneEditor key={m.id} full={full} m={m} onChange={onChange} onSel={onSel} />;
    }
  } else if (sel?.kind === 'buffer') {
    title = '缓冲期';
    body = <BufferEditor key={sel.index} full={full} index={sel.index} onChange={onChange} onSel={onSel} />;
  }
  const shorts = viz.short.map((s) => {
    const t = full.tracks.find((x) => x.plan.id === s.lane);
    return `「${t?.plan.title ?? ''}」到 ${dayLabel(s.by)} 还差约 ${mins(s.minutes)}`;
  });
  return (
    <div class="node-panel card">
      {body ? (
        <>
          <div class="row node-head">
            <h2 class="grow">{title}</h2>
            <button type="button" class="link-btn" onClick={() => onSel(null)}>
              收起
            </button>
          </div>
          {body}
        </>
      ) : (
        <>
          <h2>改计划书</h2>
          <p class="muted small">在图上点一个阶段、一条线、一段内容或者一个节点就能改。也可以加：</p>
          <AddBar full={full} onChange={onChange} onSel={onSel} today={today} />
        </>
      )}
      {shorts.length > 0 && (
        <div class="alert alert-error small">
          ⚠ 按现在每天的时间排不完：{shorts.join('；')}。可以多给点时间、调份额、把阶段拉长，或者删掉一些内容。
        </div>
      )}
    </div>
  );
}
