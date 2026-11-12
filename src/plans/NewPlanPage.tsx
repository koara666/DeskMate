// 新建计划：先随便写（写多写少都行）→ 她来理解、拆内容、排时间 → 看一看、改一改 → 开始。
// 复杂的长期计划（很长、分了好几节、跨好几个月）做成「计划书」：她可以先问最多 3 个问题（能跳过），再出路线图。
// 重新安排也用这里的 DraftEditor（只是确认按钮不一样）。
import { useEffect, useRef, useState } from 'preact/hooks';
import type { PlansApi } from '../lib/ipc';
import type { GoalDraft, Plan, PlanBoot, PlanDraft, PlanFixed, PlanItem, PlanMode, PlanPreview, PlanQuestion } from '../lib/types';
import { ItemsEditor, PlanForm, PreviewCard, WeekEditor } from './editors';
import { GoalDraftEditor } from './GoalPage';
import { emptyPlan } from './util';

const EXAMPLES = [
  '12月20日期末考试，要复习线性代数第1-6章，每天大概1个半小时，周末可以多学点',
  '一个月读完《人类简史》，一共450页，每天晚上睡前读',
  '每天背50个单词，早上8点',
  '健身，每周3次，每次1小时：胸背、腿、肩轮流',
  '这周要做的：\n1. 写完实验报告（2小时）\n2. 准备组会PPT（3小时）\n3. 整理房间',
  '【目标】半年后通过专业资格考试，同时准备找实习\n【约束】每天1〜2小时，周末多一些；1月中旬期末考试\n【要做的】\n1. 把三门专业课吃透\n2. 刷历年真题：选择、计算、案例\n3. 每周花1小时用于联系实习单位\n【时间】2026年11月9日〜2027年5月31日，分阶段，有关键节点',
];

/** 草稿的编辑和预览（改了就重新推算） */
export function DraftEditor({
  draft,
  api,
  boot,
  confirmLabel,
  busy,
  onConfirm,
  onBack,
  backLabel = '返回改写',
}: {
  draft: PlanDraft;
  api: Pick<PlansApi, 'preview'>;
  boot: PlanBoot;
  confirmLabel: string;
  busy?: boolean;
  onConfirm: (plan: Plan, items: PlanItem[]) => void;
  onBack: () => void;
  backLabel?: string;
}) {
  const [plan, setPlan] = useState<Plan>(draft.plan);
  const [items, setItems] = useState<PlanItem[]>(draft.items);
  const [preview, setPreview] = useState<PlanPreview>(draft.preview);
  const [err, setErr] = useState('');
  // 后端给的草稿自带推算结果，第一次不用再算；自己从头填的（没有推算）要马上算一次
  const first = useRef(draft.preview.per_week !== '');
  useEffect(() => {
    setPlan(draft.plan);
    setItems(draft.items);
    setPreview(draft.preview);
    first.current = draft.preview.per_week !== '';
  }, [draft]);
  useEffect(() => {
    if (first.current) {
      first.current = false;
      return;
    }
    const t = setTimeout(() => {
      api
        .preview(plan, items)
        .then((p) => {
          setPreview(p);
          setErr('');
        })
        .catch((e) => setErr(String(e)));
    }, 300);
    return () => clearTimeout(t);
  }, [api, plan, items]);

  const avatar = boot.images.happy ?? boot.images.idle;
  return (
    <div class="draft">
      {plan.strategy && (
        <div class="her-say">
          {avatar && <img class="her-avatar" src={avatar} alt="" />}
          <div class="bubble">
            {plan.strategy}
            {draft.source === 'local' && <div class="muted small">（这次是按本地规则拆的）</div>}
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
      <div class="card">
        <h2>{plan.mode === 'habit' ? '每天做什么' : '要做的内容'}</h2>
        <ItemsEditor items={items} mode={plan.mode} onChange={setItems} />
      </div>
      <div class="card">
        <h2>时间怎么安排</h2>
        <PlanForm plan={plan} onChange={setPlan} today={boot.today} />
      </div>
      <div class="card">
        <h2>排出来大概是这样</h2>
        <PreviewCard preview={preview} plan={plan} today={boot.today} />
        {err && <div class="alert alert-error">{err}</div>}
      </div>
      <div class="actions sticky-actions">
        <button type="button" class="btn btn-primary" disabled={busy || !plan.title.trim() || !!err} onClick={() => onConfirm(plan, items)}>
          {confirmLabel}
        </button>
        <button type="button" class="btn" onClick={onBack}>
          {backLabel}
        </button>
      </div>
    </div>
  );
}

export function NewPlanPage({
  api,
  boot,
  initialText = '',
  onCreated,
  onCreatedGoal,
  onError,
}: {
  api: Pick<PlansApi, 'draft' | 'preview' | 'apply' | 'think' | 'goalPreview' | 'goalApply' | 'onThinking'>;
  boot: PlanBoot;
  initialText?: string;
  onCreated: (id: number) => void;
  onCreatedGoal: (id: number) => void;
  onError: (e: unknown) => void;
}) {
  const [text, setText] = useState(initialText);
  const [more, setMore] = useState(false);
  const [mode, setMode] = useState<PlanMode | null>(null);
  const [complex, setComplex] = useState<boolean | null>(null);
  const [week, setWeek] = useState<number[] | null>(null);
  const [end, setEnd] = useState('');
  const [remind, setRemind] = useState('');
  const [draft, setDraft] = useState<PlanDraft | null>(null);
  const [goalDraft, setGoalDraft] = useState<GoalDraft | null>(null);
  const [questions, setQuestions] = useState<PlanQuestion[] | null>(null);
  const [answers, setAnswers] = useState<string[]>([]);
  const [thinking, setThinking] = useState(false);
  const [stage, setStage] = useState('');
  const [saving, setSaving] = useState(false);
  // 从聊天里带过来一句新的：换上它，回到第一步
  useEffect(() => {
    setText(initialText);
    if (initialText) {
      setDraft(null);
      setGoalDraft(null);
      setQuestions(null);
    }
  }, [initialText]);
  useEffect(() => {
    const off = api.onThinking((s) => setStage(s));
    return () => {
      off.then((f) => f()).catch(() => {});
    };
  }, [api]);

  const fixed = (): PlanFixed => ({
    title: '',
    mode,
    week_minutes: week,
    end_day: end,
    start_day: '',
    remind_at: remind,
  });

  /** answers 为 null = 第一次（她可以先问）；[] = 跳过了问题 */
  const think = async (qa: { q: string; a: string }[] | null) => {
    setThinking(true);
    setStage('');
    try {
      const r = await api.think(text, fixed(), qa, qa === null ? complex : true);
      if (r.kind === 'plan') setDraft(r.draft);
      else if (r.kind === 'questions') {
        setQuestions(r.questions);
        setAnswers(r.questions.map(() => ''));
      } else {
        setQuestions(null);
        setGoalDraft(r.draft);
      }
    } catch (e) {
      onError(e);
    } finally {
      setThinking(false);
      setStage('');
    }
  };
  const answered = () => think((questions ?? []).map((q, i) => ({ q: q.q, a: answers[i]?.trim() ?? '' })));

  const createGoal = async (full: GoalDraft['full']) => {
    setSaving(true);
    try {
      onCreatedGoal(await api.goalApply(full, true));
    } catch (e) {
      onError(e);
    } finally {
      setSaving(false);
    }
  };

  const blank = () =>
    setDraft({
      plan: { ...emptyPlan(boot.today), title: text.trim().split('\n')[0].slice(0, 30) || '新计划', goal: text.trim(), mode: mode ?? 'content', week_minutes: week ?? [60, 60, 60, 60, 60, 60, 60], end_day: end, remind_at: remind },
      items: [],
      source: 'local',
      notes: [],
      preview: { days: [], finish: '', short_minutes: 0, total_minutes: 0, per_week: '' },
    });

  const create = async (plan: Plan, items: PlanItem[]) => {
    setSaving(true);
    try {
      onCreated(await api.apply(plan, items));
    } catch (e) {
      onError(e);
    } finally {
      setSaving(false);
    }
  };

  if (draft) {
    return (
      <section>
        <header class="page-head">
          <h1>看看这样排行不行</h1>
        </header>
        <DraftEditor draft={draft} api={api} boot={boot} confirmLabel="就这样，开始！" busy={saving} onConfirm={create} onBack={() => setDraft(null)} />
      </section>
    );
  }

  if (goalDraft) {
    return (
      <section>
        <header class="page-head">
          <h1>计划书：看看这样排行不行</h1>
        </header>
        <GoalDraftEditor draft={goalDraft} api={api} boot={boot} confirmLabel="就这样，开始！" busy={saving} onConfirm={createGoal} onBack={() => setGoalDraft(null)} />
      </section>
    );
  }

  const avatar = (thinking ? boot.images.think : null) ?? boot.images.idle;

  if (questions) {
    return (
      <section class="new-plan">
        <header class="page-head">
          <h1>先问你几句</h1>
        </header>
        <div class="her-say">
          {avatar && <img class={`her-avatar ${thinking ? 'thinking' : ''}`} src={avatar} alt="" />}
          <div class="bubble">
            {thinking ? `${boot.character.name}在${stage || '想'}……` : '这个计划有点大，排之前想先确认几件事。不想答的空着就行，也可以直接跳过。'}
          </div>
        </div>
        <div class="card questions">
          {questions.map((q, i) => (
            <div key={i} class="question">
              <label>
                <span class="q-text">
                  {i + 1}. {q.q}
                </span>
                <textarea
                  rows={2}
                  maxLength={1000}
                  value={answers[i] ?? ''}
                  onInput={(e) => setAnswers(answers.map((a, k) => (k === i ? e.currentTarget.value : a)))}
                  aria-label={`回答第 ${i + 1} 个问题`}
                  disabled={thinking}
                />
              </label>
              {q.options.length > 0 && (
                <div class="examples">
                  {q.options.map((o) => (
                    <button type="button" key={o} class="chip" disabled={thinking} onClick={() => setAnswers(answers.map((a, k) => (k === i ? o : a)))}>
                      {o}
                    </button>
                  ))}
                </div>
              )}
            </div>
          ))}
          <div class="actions">
            <button type="button" class="btn btn-primary" disabled={thinking} onClick={answered}>
              {thinking ? '她在排…' : '回答好了，开始排'}
            </button>
            <button type="button" class="btn" disabled={thinking} onClick={() => think([])}>
              跳过，直接排
            </button>
            <button type="button" class="link-btn" disabled={thinking} onClick={() => setQuestions(null)}>
              返回改写
            </button>
          </div>
        </div>
      </section>
    );
  }
  return (
    <section class="new-plan">
      <header class="page-head">
        <h1>新建计划</h1>
      </header>
      <div class="her-say">
        {avatar && <img class={`her-avatar ${thinking ? 'thinking' : ''}`} src={avatar} alt="" />}
        <div class="bubble">
          {thinking
            ? `${boot.character.name}在${stage || '帮你想怎么排'}……`
            : '想做什么？写多写少都行——一句话也好，整份大纲也好，我来帮你拆成每天能做完的小块。很长、分好几个阶段的计划，我会做成一份计划书（可能先问你几句）。'}
        </div>
      </div>
      <div class="card">
        <textarea
          class="goal-input"
          rows={7}
          maxLength={8000}
          placeholder="比如：12月20日考试，复习线性代数第1-6章，每天1个半小时"
          value={text}
          onInput={(e) => setText(e.currentTarget.value)}
          aria-label="想做什么计划"
          disabled={thinking}
        />
        <div class="examples">
          <span class="muted small">试试这些：</span>
          {EXAMPLES.map((x) => (
            <button type="button" key={x} class="chip" onClick={() => setText(x)} disabled={thinking}>
              {x.split('\n')[0].slice(0, 18)}…
            </button>
          ))}
        </div>
        <button type="button" class="link-btn" onClick={() => setMore(!more)} aria-expanded={more}>
          {more ? '收起' : '我已经想好了一些（可选）'}
        </button>
        {more && (
          <div class="fixed-box">
            <div class="seg left" role="radiogroup" aria-label="复杂程度">
              {(
                [
                  [null, '让她判断'],
                  [false, '简单的计划'],
                  [true, '长期的计划书（分阶段、几条线）'],
                ] as [boolean | null, string][]
              ).map(([c, label]) => (
                <button type="button" key={label} role="radio" aria-checked={complex === c} class={complex === c ? 'on' : ''} onClick={() => setComplex(c)}>
                  {label}
                </button>
              ))}
            </div>
            <div class="seg left" role="radiogroup" aria-label="计划类型">
              {(
                [
                  [null, '让她判断'],
                  ['content', '有总量'],
                  ['habit', '每天都做'],
                ] as [PlanMode | null, string][]
              ).map(([m, label]) => (
                <button type="button" key={label} role="radio" aria-checked={mode === m} class={mode === m ? 'on' : ''} onClick={() => setMode(m)}>
                  {label}
                </button>
              ))}
            </div>
            <label class="check-label">
              <input type="checkbox" checked={week !== null} onChange={(e) => setWeek(e.currentTarget.checked ? [60, 60, 60, 60, 60, 60, 60] : null)} />
              每天多久我自己定
            </label>
            {week && <WeekEditor value={week} onChange={setWeek} />}
            <div class="grid">
              <label>
                截止（可空）
                <input type="date" min={boot.today} value={end} onInput={(e) => setEnd(e.currentTarget.value)} aria-label="截止日期" />
              </label>
              <label>
                每天几点开始（可空）
                <input type="time" value={remind} onInput={(e) => setRemind(e.currentTarget.value)} aria-label="每天几点开始" />
              </label>
            </div>
          </div>
        )}
        <div class="actions">
          <button type="button" class="btn btn-primary" disabled={thinking || !text.trim()} onClick={() => think(null)}>
            {thinking ? '她在想…' : '让她来安排'}
          </button>
          <button type="button" class="btn" disabled={thinking} onClick={blank}>
            我自己一条条填
          </button>
        </div>
      </div>
    </section>
  );
}
