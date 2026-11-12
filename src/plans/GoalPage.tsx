// 计划书：图（时间线 / 树状图 / 流程图）+ 选中的地方在下面改；这周的情况和自查清单、调整下周；原文（可以整段改，让她重新理解）；学习资源；设置。
// 新建、调整下周、重新理解得到的都是草稿，用 GoalDraftEditor 看一看、改一改再保存。
import { useEffect, useRef, useState } from 'preact/hooks';
import type { PlansApi } from '../lib/ipc';
import type { GoalDraft, GoalFull, GoalResource, GoalView, GoalViz, PlanBoot } from '../lib/types';
import { WeekEditor } from './editors';
import { UpcomingDays, VizPanel, type Sel } from './GoalViz';
import { WEEKDAY_FULL, isRoutine, laneColor, rangeLabel } from './goalUtil';
import { NodePanel } from './NodePanel';
import { daysBetween, dayLabel, mins, relDay } from './util';

type Api = Pick<
  PlansApi,
  'goalGet' | 'goalPreview' | 'goalApply' | 'goalSetStatus' | 'goalDelete' | 'goalExpand' | 'goalRethink' | 'goalRenderDoc' | 'milestoneCheck'
>;

const STATUS: Record<string, string> = { active: '进行中', paused: '暂停中', done: '已完成', archived: '已归档' };

/** 计划书的时间：开始、结束、平时每天多久、几点开始 */
function GoalTimeForm({ full, onChange, today }: { full: GoalFull; onChange: (f: GoalFull) => void; today: string }) {
  const g = full.goal;
  const up = (patch: Partial<typeof g>) => onChange({ ...full, goal: { ...g, ...patch } });
  return (
    <div class="grid">
      <label class="span2 title-row">
        名字
        <span class="inline">
          <input class="emoji-input" value={g.emoji} maxLength={4} onInput={(e) => up({ emoji: e.currentTarget.value })} aria-label="计划书的图标" />
          <input class="grow" value={g.title} maxLength={60} onInput={(e) => up({ title: e.currentTarget.value })} aria-label="计划书的名字" />
        </span>
      </label>
      <label class="span2">
        目标（一句话）
        <input value={g.summary} maxLength={500} onInput={(e) => up({ summary: e.currentTarget.value })} aria-label="目标" />
      </label>
      <label>
        从哪天开始
        <input type="date" value={g.start_day} onInput={(e) => up({ start_day: e.currentTarget.value || today })} aria-label="计划书开始" />
      </label>
      <label>
        到哪天为止
        <input type="date" value={g.end_day} min={g.start_day} onInput={(e) => up({ end_day: e.currentTarget.value })} aria-label="计划书结束" />
      </label>
      <div class="span2">
        <div class="field-label">平时每天一共能用多少时间（所有线加起来；阶段可以单独设）</div>
        <WeekEditor value={g.week_minutes} onChange={(w) => up({ week_minutes: w })} />
      </div>
      <label>
        每天几点开始（可空）
        <input type="time" value={g.remind_at} onInput={(e) => up({ remind_at: e.currentTarget.value })} aria-label="计划书每天几点开始" />
      </label>
      <label>
        每周哪天自查
        <select value={g.review_day} onChange={(e) => up({ review_day: Number(e.currentTarget.value) })} aria-label="每周哪天自查">
          {WEEKDAY_FULL.map((w, i) => (
            <option key={w} value={i}>
              {w}
            </option>
          ))}
          <option value={-1}>不自查</option>
        </select>
      </label>
    </div>
  );
}

/** 图 + 选中的地方的编辑面板（草稿和已保存的都用） */
function GoalEditor({
  full,
  viz,
  today,
  sel,
  onSel,
  onChange,
  onOpenTrack,
}: {
  full: GoalFull;
  viz: GoalViz;
  today: string;
  sel: Sel | null;
  onSel: (s: Sel | null) => void;
  onChange: (f: GoalFull, items?: boolean) => void;
  onOpenTrack?: (id: number) => void;
}) {
  const panel = useRef<HTMLDivElement>(null);
  const pick = (s: Sel) => {
    onSel(s);
    setTimeout(() => panel.current?.scrollIntoView?.({ block: 'nearest', behavior: 'smooth' }), 0);
  };
  return (
    <>
      <div class="card viz-card">
        <VizPanel full={full} viz={viz} today={today} sel={sel} onSel={pick} />
      </div>
      <div ref={panel}>
        <NodePanel full={full} viz={viz} sel={sel} today={today} onSel={onSel} onChange={onChange} onOpenTrack={onOpenTrack} />
      </div>
    </>
  );
}

/** 草稿：看一看、改一改（改了就重新推算），确认后保存 */
export function GoalDraftEditor({
  draft,
  api,
  boot,
  confirmLabel,
  busy,
  onConfirm,
  onBack,
  backLabel = '返回改写',
}: {
  draft: GoalDraft;
  api: Pick<PlansApi, 'goalPreview'>;
  boot: PlanBoot;
  confirmLabel: string;
  busy?: boolean;
  onConfirm: (full: GoalFull) => void;
  onBack: () => void;
  backLabel?: string;
}) {
  const [full, setFull] = useState<GoalFull>(draft.full);
  const [viz, setViz] = useState<GoalViz>(draft.viz);
  const [sel, setSel] = useState<Sel | null>(null);
  const [err, setErr] = useState('');
  const first = useRef(true);
  useEffect(() => {
    setFull(draft.full);
    setViz(draft.viz);
    first.current = true;
  }, [draft]);
  useEffect(() => {
    if (first.current) {
      first.current = false;
      return;
    }
    const t = setTimeout(() => {
      api
        .goalPreview(full)
        .then((v) => {
          setViz(v);
          setErr('');
        })
        .catch((e) => setErr(String(e)));
    }, 350);
    return () => clearTimeout(t);
  }, [api, full]);
  const avatar = boot.images.happy ?? boot.images.idle;
  const g = full.goal;
  return (
    <div class="draft goal-draft">
      {g.strategy && (
        <div class="her-say">
          {avatar && <img class="her-avatar" src={avatar} alt="" />}
          <div class="bubble">
            {g.strategy}
            {draft.source === 'local' && <div class="muted small">（这次是按本地规则做的）</div>}
          </div>
        </div>
      )}
      {draft.notes.length > 0 && (
        <ul class="draft-notes">
          {draft.notes.map((n, i) => (
            <li key={i}>{n}</li>
          ))}
        </ul>
      )}
      <div class="card goal-sum">
        <div class="row">
          <span class="big">
            {g.emoji} {g.title}
          </span>
          <span class="muted">{rangeLabel(g.start_day, g.end_day || g.start_day)}</span>
        </div>
        <div class="row small muted">
          {full.tracks.length} 条线 · {full.phases.length} 个阶段 · {full.milestones.length} 个节点
          {g.buffers.length ? ` · ${g.buffers.length} 段缓冲` : ''}
          {viz.total ? ` · 内容共约 ${mins(viz.total)}` : ''}
          {viz.finish ? ` · 预计 ${dayLabel(viz.finish)} 做完` : ''}
        </div>
      </div>
      <GoalEditor full={full} viz={viz} today={boot.today} sel={sel} onSel={setSel} onChange={(f) => setFull(f)} />
      <details class="card">
        <summary>时间、名字（可以改）</summary>
        <GoalTimeForm full={full} onChange={setFull} today={boot.today} />
      </details>
      <div class="card">
        <h2>接下来两周大概是这样</h2>
        <UpcomingDays full={full} viz={viz} today={boot.today} />
      </div>
      {err && <div class="alert alert-error">{err}</div>}
      <div class="actions sticky-actions">
        <button type="button" class="btn btn-primary" disabled={busy || !g.title.trim() || !!err || full.tracks.length === 0} onClick={() => onConfirm(full)}>
          {confirmLabel}
        </button>
        <button type="button" class="btn" onClick={onBack}>
          {backLabel}
        </button>
      </div>
    </div>
  );
}

type Tab = 'book' | 'week' | 'doc' | 'res' | 'settings';

export function GoalPage({
  api,
  boot,
  id,
  tick,
  focus,
  onDeleted,
  onOpenPlan,
  onError,
}: {
  api: Api;
  boot: PlanBoot;
  id: number;
  tick: number;
  /** 先打开哪一块：week / adjust / m<节点 id> */
  focus?: string;
  onDeleted: () => void;
  onOpenPlan: (id: number) => void;
  onError: (e: unknown) => void;
}) {
  const [v, setV] = useState<GoalView | null>(null);
  const [tab, setTab] = useState<Tab>('book');
  const [sel, setSel] = useState<Sel | null>(null);
  const [edit, setEdit] = useState<GoalFull | null>(null);
  const [editItems, setEditItems] = useState(false);
  const [editViz, setEditViz] = useState<GoalViz | null>(null);
  const [draft, setDraft] = useState<{ d: GoalDraft; label: string } | null>(null);
  const [adjust, setAdjust] = useState<string | null>(null);
  const [doc, setDoc] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState('');
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [checked, setChecked] = useState<Set<number>>(new Set());
  const adjustRef = useRef<HTMLTextAreaElement>(null);
  const dirty = useRef(false);
  dirty.current = edit !== null || doc !== null || draft !== null;

  const load = () => {
    api
      .goalGet(id)
      .then((x) => {
        setV(x);
      })
      .catch(onError);
  };
  useEffect(() => {
    setEdit(null);
    setEditViz(null);
    setDraft(null);
    setDoc(null);
    setSel(null);
    setConfirmDelete(false);
    setTab('book');
    setAdjust(null);
  }, [id]);
  // 别处改了（勾掉了一项、换了一天）：没在改的时候刷新
  useEffect(() => {
    if (!dirty.current || !v) load();
  }, [api, id, tick]);
  useEffect(() => {
    if (!focus) return;
    if (focus === 'week' || focus === 'adjust') {
      setTab('week');
      if (focus === 'adjust') {
        setAdjust('');
        setTimeout(() => adjustRef.current?.focus(), 50);
      }
    } else if (/^m\d+$/.test(focus)) {
      setTab('book');
      setSel({ kind: 'milestone', id: Number(focus.slice(1)) });
    }
  }, [focus, id]);
  // 在改的时候重新推算
  useEffect(() => {
    if (!edit) return;
    const t = setTimeout(() => {
      api
        .goalPreview(edit)
        .then(setEditViz)
        .catch(() => {});
    }, 350);
    return () => clearTimeout(t);
  }, [api, edit]);

  if (!v) return <div class="muted">加载中…</div>;

  const full = edit ?? v.full;
  const viz = (edit && editViz) || v.viz;
  const g = v.full.goal;
  const flash = (m: string) => {
    setSaved(m);
    setTimeout(() => setSaved(''), 2500);
  };
  const save = async (next: GoalFull, withItems: boolean, msg = '已保存') => {
    setBusy(true);
    try {
      await api.goalApply(next, withItems);
      setEdit(null);
      setEditViz(null);
      setEditItems(false);
      flash(msg);
      load();
    } catch (e) {
      onError(e);
    } finally {
      setBusy(false);
    }
  };
  const change = (f: GoalFull, items?: boolean) => {
    setEdit(f);
    if (items) setEditItems(true);
  };
  const run = async (fn: () => Promise<GoalDraft>, label: string) => {
    setBusy(true);
    try {
      setDraft({ d: await fn(), label });
    } catch (e) {
      onError(e);
    } finally {
      setBusy(false);
    }
  };

  if (draft) {
    return (
      <section>
        <header class="page-head">
          <h1>
            {draft.label}：{g.title}
          </h1>
        </header>
        <GoalDraftEditor
          draft={draft.d}
          api={api}
          boot={boot}
          confirmLabel="按这个改"
          busy={busy}
          backLabel="不改了"
          onConfirm={async (f) => {
            await save(f, true, '已更新计划书');
            setDraft(null);
            setAdjust(null);
            setDoc(null);
          }}
          onBack={() => setDraft(null)}
        />
      </section>
    );
  }

  const today = boot.today;
  const current = viz.phases.find((p) => p.state === 'current');
  const next = v.full.milestones.find((m) => m.status === 'todo' && m.day >= today);
  const overdue = v.full.milestones.filter((m) => m.status === 'todo' && m.day < today);
  const idx = new Map(full.tracks.map((t, i) => [t.plan.id, i]));
  const name = new Map(full.tracks.map((t) => [t.plan.id, t.plan.title]));
  const missed = viz.missed.map(([lane, item]) => `${name.get(lane) ?? ''}「${full.tracks.find((t) => t.plan.id === lane)?.items.find((i) => i.id === item)?.title ?? ''}」`);

  return (
    <section class="goal-page">
      <header class="page-head">
        <div>
          <h1>
            {g.emoji} {g.title}
          </h1>
          <div class="row small">
            <span class={`tag ${g.status === 'active' ? 'tag-accent' : ''}`}>{STATUS[g.status]}</span>
            <span class="muted">
              {rangeLabel(g.start_day, g.end_day || g.start_day)}
              {g.end_day && daysBetween(today, g.end_day) >= 0 ? `（还有 ${daysBetween(today, g.end_day)} 天）` : ''}
              {g.remind_at ? ` · 每天 ${g.remind_at}` : ''}
            </span>
          </div>
        </div>
        <div class="head-actions">
          {g.status === 'active' && (
            <button
              type="button"
              class="btn btn-small"
              onClick={() => {
                setTab('week');
                setAdjust('');
                setTimeout(() => adjustRef.current?.focus(), 50);
              }}
            >
              调整下周
            </button>
          )}
          {g.status === 'active' ? (
            <button type="button" class="btn btn-small" onClick={() => api.goalSetStatus(id, 'paused').then(load).catch(onError)}>
              暂停
            </button>
          ) : (
            <button type="button" class="btn btn-small" onClick={() => api.goalSetStatus(id, 'active').then(load).catch(onError)}>
              {g.status === 'done' ? '再开始' : '恢复'}
            </button>
          )}
        </div>
      </header>

      <div class="goal-stats">
        <div class="card stat">
          <div class="big">{viz.percent}%</div>
          <div class="bar her">
            <div class="fill" style={{ width: `${viz.percent}%` }} />
          </div>
          <div class="muted small">
            做了 {mins(viz.done)} / 共约 {mins(viz.total)}
            {viz.finish ? ` · 预计 ${dayLabel(viz.finish)} 做完` : ''}
          </div>
        </div>
        <div class="card stat">
          <div class="muted small">现在</div>
          {current ? (
            <>
              <div class="stat-title">
                {current.phase.title} <span class="muted small">{current.percent}%</span>
              </div>
              <div class="muted small">
                到 {dayLabel(current.phase.end_day)}（还有 {daysBetween(today, current.phase.end_day)} 天）{current.phase.objective ? ` · ${current.phase.objective}` : ''}
              </div>
            </>
          ) : (
            <div class="stat-title muted">不在任何阶段里</div>
          )}
        </div>
        <div class="card stat">
          <div class="muted small">下一个节点</div>
          {next ? (
            <button type="button" class="link-btn stat-title" onClick={() => setSel({ kind: 'milestone', id: next.id })}>
              ◆ {next.title} <span class="muted small">{relDay(next.day, today)}</span>
            </button>
          ) : (
            <div class="stat-title muted">没有了</div>
          )}
          {overdue.length > 0 && <div class="warn-text small">有 {overdue.length} 个节点还没确认达标没有</div>}
        </div>
      </div>

      {(viz.short.length > 0 || missed.length > 0) && (
        <div class="alert alert-error small">
          {viz.short.map((s) => `⚠ 「${name.get(s.lane) ?? ''}」到 ${dayLabel(s.by)} 还差约 ${mins(s.minutes)}`).join('；')}
          {missed.length > 0 && `${viz.short.length ? '；' : ''}钉在过去日子还没做的：${missed.join('、')}（不会自动顺延，可以在线里改日子）`}
        </div>
      )}

      <nav class="tabs" role="tablist">
        {(
          [
            ['book', '计划书'],
            ['week', '这周'],
            ['doc', '原文'],
            ['res', `资源${g.resources.length ? `（${g.resources.length}）` : ''}`],
            ['settings', '设置'],
          ] as [Tab, string][]
        ).map(([t, label]) => (
          <button type="button" key={t} role="tab" aria-selected={tab === t} class={tab === t ? 'on' : ''} onClick={() => setTab(t)}>
            {label}
          </button>
        ))}
        {saved && <span class="status status-ok">{saved}</span>}
      </nav>

      {tab === 'book' && (
        <>
          {g.strategy && (
            <details class="card strategy">
              <summary>她的安排思路</summary>
              <p>{g.strategy}</p>
            </details>
          )}
          <GoalEditor full={full} viz={viz} today={today} sel={sel} onSel={setSel} onChange={change} onOpenTrack={onOpenPlan} />
          {edit && (
            <div class="actions sticky-actions">
              <button type="button" class="btn btn-primary" disabled={busy || !edit.goal.title.trim()} onClick={() => save(edit, editItems)}>
                保存修改
              </button>
              <button
                type="button"
                class="btn"
                onClick={() => {
                  setEdit(null);
                  setEditViz(null);
                  setEditItems(false);
                }}
              >
                撤销修改
              </button>
              <span class="muted small">保存后今天还没动的安排会重排。</span>
            </div>
          )}
          <div class="card">
            <h2>接下来两周</h2>
            <UpcomingDays full={full} viz={viz} today={today} />
          </div>
        </>
      )}

      {tab === 'week' && (
        <>
          <div class="card">
            <h2>
              这周（{relDay(v.week.from, today)} — {relDay(v.week.to, today)}）
            </h2>
            {v.week.tasks === 0 ? (
              <p class="muted small">这周还没有排到的事项。</p>
            ) : (
              <>
                <p class="small">
                  做完 {v.week.tasks_done}/{v.week.tasks} 项，{mins(v.week.done)} / 排了 {mins(v.week.planned)}
                </p>
                <ul class="plain week-bars">
                  {v.week.tracks
                    .filter((t) => t.tasks > 0)
                    .map((t) => (
                      <li key={t.plan_id}>
                        <span class="wb-name">
                          <i class="sw" style={{ background: laneColor(idx.get(t.plan_id) ?? -1) }} />
                          {name.get(t.plan_id)}
                        </span>
                        <span class="wb-bar" aria-hidden="true">
                          <span class="wb-plan" />
                          <span class="wb-done" style={{ width: `${t.planned ? Math.min(100, (t.done / t.planned) * 100) : 0}%`, background: laneColor(idx.get(t.plan_id) ?? -1) }} />
                        </span>
                        <span class="muted small wb-num">
                          {t.tasks_done}/{t.tasks} 项 · {mins(t.done)}
                          {t.tasks_done * 2 < t.tasks ? ' · 落下了' : ''}
                        </span>
                      </li>
                    ))}
                </ul>
              </>
            )}
          </div>
          {g.checklist.length > 0 && (
            <div class="card">
              <h2>
                {g.review_day >= 0 ? `${WEEKDAY_FULL[g.review_day]}自查清单` : '自查清单'}
              </h2>
              <ul class="plain checklist">
                {g.checklist.map((c, i) => (
                  <li key={i}>
                    <label class="check-label">
                      <input
                        type="checkbox"
                        checked={checked.has(i)}
                        onChange={() => {
                          const s = new Set(checked);
                          if (s.has(i)) s.delete(i);
                          else s.add(i);
                          setChecked(s);
                        }}
                      />
                      {c}
                    </label>
                  </li>
                ))}
              </ul>
              <p class="muted small">过一遍就好，勾选不会记下来。想改清单在「原文」里改。</p>
            </div>
          )}
          <div class="card replan">
            <h2>调整下周</h2>
            {adjust === null ? (
              <>
                <p class="muted small">她会看这周每条线做了多少，把接下来两周要做的重新细化、调整（调一次模型）。不调的话，粗的内容到时候也会按要点自动拆开。</p>
                <button type="button" class="btn btn-small" disabled={g.status !== 'active'} onClick={() => setAdjust('')}>
                  写一句要求
                </button>
              </>
            ) : (
              <>
                <textarea
                  ref={adjustRef}
                  rows={3}
                  maxLength={1000}
                  placeholder="可以不写。也可以说：下周三有考试 / 项目讲解多练几次 / 编程太难了放慢一点"
                  value={adjust}
                  onInput={(e) => setAdjust(e.currentTarget.value)}
                  aria-label="想怎么调整下周"
                  disabled={busy}
                />
                <div class="actions">
                  <button type="button" class="btn btn-primary" disabled={busy} onClick={() => run(() => api.goalExpand(id, adjust), '调整下周')}>
                    {busy ? '她在想…' : '让她调整'}
                  </button>
                  <button type="button" class="btn" onClick={() => setAdjust(null)}>
                    算了
                  </button>
                </div>
              </>
            )}
          </div>
          {v.notes.length > 0 && (
            <div class="card">
              <h2>各条线的备注</h2>
              <ul class="plain note-timeline">
                {v.notes.slice(0, 20).map((n) => (
                  <li key={n.id}>
                    <span class="note-day muted small">{relDay(n.day, today)}</span>
                    <span class="muted small">{name.get(n.plan_id ?? 0)}</span>
                    <span class="grow selectable">{n.content}</span>
                  </li>
                ))}
              </ul>
            </div>
          )}
        </>
      )}

      {tab === 'doc' && (
        <div class="card">
          <p class="muted small">这是计划书的原文，可以整段改。改完点「让她按原文重新理解」，她会按新的原文重新排（做过的都留着）；只想留着文字就「只保存文字」。</p>
          <textarea class="doc-input" rows={22} maxLength={20000} value={doc ?? g.doc} onInput={(e) => setDoc(e.currentTarget.value)} aria-label="计划书原文" disabled={busy} />
          <div class="actions">
            <button type="button" class="btn btn-primary" disabled={busy || !(doc ?? g.doc).trim()} onClick={() => run(() => api.goalRethink(id, doc ?? g.doc), '按原文重新理解')}>
              {busy ? '她在看…' : '让她按原文重新理解'}
            </button>
            <button type="button" class="btn" disabled={busy || doc === null} onClick={() => save({ ...v.full, goal: { ...v.full.goal, doc: doc ?? '' } }, false, '原文已保存').then(() => setDoc(null))}>
              只保存文字
            </button>
            <button
              type="button"
              class="btn"
              disabled={busy}
              onClick={() =>
                api
                  .goalRenderDoc(v.full)
                  .then((t) => setDoc(t))
                  .catch(onError)
              }
            >
              按现在的计划书重新生成
            </button>
            {doc !== null && (
              <button type="button" class="link-btn" onClick={() => setDoc(null)}>
                撤销修改
              </button>
            )}
          </div>
        </div>
      )}

      {tab === 'res' && <Resources full={full} onChange={(f) => setEdit(f)} />}
      {tab === 'res' && edit && (
        <div class="actions sticky-actions">
          <button type="button" class="btn btn-primary" disabled={busy} onClick={() => save(edit, false)}>
            保存资源
          </button>
          <button type="button" class="btn" onClick={() => setEdit(null)}>
            撤销修改
          </button>
        </div>
      )}

      {tab === 'settings' && (
        <div class="card">
          <GoalTimeForm full={full} onChange={(f) => setEdit(f)} today={today} />
          <div class="actions">
            <button type="button" class="btn btn-primary" disabled={busy || !edit || !edit.goal.title.trim()} onClick={() => edit && save(edit, false)}>
              保存设置
            </button>
            {edit && (
              <button type="button" class="btn" onClick={() => setEdit(null)}>
                撤销修改
              </button>
            )}
          </div>
          <div class="danger-zone row">
            {g.status !== 'archived' && (
              <button type="button" class="btn btn-small" onClick={() => api.goalSetStatus(id, 'archived').then(load).catch(onError)}>
                归档
              </button>
            )}
            {g.status !== 'done' && (
              <button type="button" class="btn btn-small" onClick={() => api.goalSetStatus(id, 'done').then(load).catch(onError)}>
                标成完成
              </button>
            )}
            {!confirmDelete ? (
              <button type="button" class="btn btn-small btn-danger" onClick={() => setConfirmDelete(true)}>
                删掉这个计划书
              </button>
            ) : (
              <>
                <span>所有的线、记录、备注都会删掉（不想继续可以「归档」）。确定吗？</span>
                <button type="button" class="btn btn-small btn-danger" onClick={() => api.goalDelete(id).then(onDeleted).catch(onError)}>
                  确定删掉
                </button>
                <button type="button" class="btn btn-small" onClick={() => setConfirmDelete(false)}>
                  不删了
                </button>
              </>
            )}
          </div>
        </div>
      )}
    </section>
  );
}

/** 学习资源：书、网站（可以挂在某条线上） */
function Resources({ full, onChange }: { full: GoalFull; onChange: (f: GoalFull) => void }) {
  const rs = full.goal.resources;
  const set = (list: GoalResource[]) => onChange({ ...full, goal: { ...full.goal, resources: list } });
  const up = (i: number, patch: Partial<GoalResource>) => set(rs.map((r, k) => (k === i ? { ...r, ...patch } : r)));
  const groups = [null, ...full.tracks.filter((t) => !isRoutine(t.plan)).map((t) => t.plan.id)];
  return (
    <div class="card resources">
      {rs.length === 0 && <p class="muted small">还没有学习资源。可以把要用的书、网站记在这里。</p>}
      {groups.map((gid) => {
        const list = rs.map((r, i) => ({ r, i })).filter(({ r }) => (r.plan_id ?? null) === gid);
        if (!list.length) return null;
        const t = full.tracks.find((x) => x.plan.id === gid);
        return (
          <div key={gid ?? 'all'}>
            <div class="field-label">{t ? `${t.plan.emoji} ${t.plan.title}` : '整个计划书'}</div>
            <ul class="plain res-list">
              {list.map(({ r, i }) => (
                <li key={i} class="res">
                  <input value={r.title} maxLength={100} placeholder="书名 / 网站名" onInput={(e) => up(i, { title: e.currentTarget.value })} aria-label="资源名字" />
                  <input value={r.url} maxLength={500} placeholder="网址（可空）" onInput={(e) => up(i, { url: e.currentTarget.value })} aria-label="网址" />
                  <input class="wide" value={r.note} maxLength={300} placeholder="怎么用（可空）" onInput={(e) => up(i, { note: e.currentTarget.value })} aria-label="怎么用" />
                  <select value={r.plan_id ?? ''} onChange={(e) => up(i, { plan_id: e.currentTarget.value ? Number(e.currentTarget.value) : null })} aria-label="挂在哪条线">
                    <option value="">整个计划书</option>
                    {full.tracks.map((x) => (
                      <option key={x.plan.id} value={x.plan.id}>
                        {x.plan.title}
                      </option>
                    ))}
                  </select>
                  <button type="button" class="icon-btn danger" aria-label={`删掉：${r.title}`} onClick={() => set(rs.filter((_, k) => k !== i))}>
                    ×
                  </button>
                </li>
              ))}
            </ul>
          </div>
        );
      })}
      <button type="button" class="btn btn-small" onClick={() => set([...rs, { title: '', url: '', note: '', plan_id: null, phase_id: null }])}>
        加一个资源
      </button>
      <p class="muted small">网址只是记下来，复制到浏览器里打开。</p>
    </div>
  );
}
