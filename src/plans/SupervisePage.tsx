// 「她怎么陪你」：监督的开关和强度。改了马上保存。
import { useState } from 'preact/hooks';
import type { PlansApi } from '../lib/ipc';
import type { PlanBoot, PlanSettings, PlanStrictness } from '../lib/types';
import { clampInt } from './util';

export function SupervisePage({
  api,
  boot,
  onSaved,
  onError,
}: {
  api: Pick<PlansApi, 'saveSettings' | 'openSettings'>;
  boot: PlanBoot;
  onSaved: (s: PlanSettings) => void;
  onError: (e: unknown) => void;
}) {
  const [s, setS] = useState<PlanSettings>(boot.settings);
  const [ok, setOk] = useState(false);
  const save = async (next: PlanSettings) => {
    setS(next);
    try {
      const saved = await api.saveSettings(next);
      setS(saved);
      onSaved(saved);
      setOk(true);
      setTimeout(() => setOk(false), 1500);
    } catch (e) {
      onError(e);
    }
  };
  const up = (patch: Partial<PlanSettings>) => save({ ...s, ...patch });
  const name = boot.character.name || '她';
  const check = (key: keyof PlanSettings, label: string, hint: string) => (
    <label class="check-row">
      <input type="checkbox" checked={!!s[key]} disabled={!s.supervise && key !== 'supervise'} onChange={(e) => up({ [key]: e.currentTarget.checked })} />
      <span>
        {label}
        <small>{hint}</small>
      </span>
    </label>
  );
  return (
    <section>
      <header class="page-head">
        <h1>{name}怎么陪你</h1>
        {ok && <span class="status status-ok">已保存</span>}
      </header>
      <div class="card">
        {check('supervise', `让${name}监督我的计划`, '关掉后计划照样能用，只是她不会主动提醒。每个计划也能单独关（计划的「设置」里）。')}
        <div class="row">
          <span class="label">监督强度</span>
          <div class="seg left" role="radiogroup" aria-label="监督强度">
            {(
              [
                ['gentle', '温和'],
                ['normal', '适中'],
                ['strict', '严格'],
              ] as [PlanStrictness, string][]
            ).map(([k, label]) => (
              <button type="button" key={k} role="radio" aria-checked={s.strictness === k} class={s.strictness === k ? 'on' : ''} disabled={!s.supervise} onClick={() => up({ strictness: k })}>
                {label}
              </button>
            ))}
          </div>
        </div>
        <p class="muted small">
          温和：一天最多提一次，分心很久才说；适中：一天最多两次；严格：提得更勤，晚上没做完还会再问一次。
        </p>
      </div>
      <div class="card">
        {check('start_remind', '到了开始时间提醒我', '计划或事项定了几点开始的，到点还没做完会叫你一次（和普通提醒一样，勿扰时也会说）。')}
        {check('distraction', '该做计划的时候在玩游戏 / 看视频，提一句', '需要在设置里开着「活动感知」。全屏时不打断，等退出全屏再说。')}
        {check('checkin', '晚上问一句今天做得怎么样', '还有没做完的才问，选一下就帮你记好。')}
        <div class="row">
          <span class="label">几点问</span>
          <input type="time" value={s.checkin_at} disabled={!s.supervise || !s.checkin} onChange={(e) => up({ checkin_at: e.currentTarget.value || '21:30' })} aria-label="晚上几点问" />
        </div>
        {check('briefing', '每日播报里说说今天的计划', '早上第一次见面时告诉你今天要做什么。')}
        {check('chat_context', '聊天时她知道今天的计划', '可以直接问她「今天还要做什么」。')}
        {check('celebrate', '做完的时候她有反应', '勾掉一项、全部做完、整个计划完成时她会说一句。')}
        <div class="row">
          <span class="label">专注计时默认</span>
          <span class="inline">
            <input
              type="number"
              min={5}
              max={180}
              step={5}
              class="num"
              value={s.focus_minutes}
              onChange={(e) => up({ focus_minutes: clampInt(e.currentTarget.value, 5, 180) })}
              aria-label="专注计时默认多少分钟"
            />
            分钟
          </span>
        </div>
      </div>
      <div class="card">
        <h2>计划书</h2>
        {check('milestone_check', '节点那天问我达标了没有', '选「达标了」就记下来；「还没有」会打开计划书，看看要不要把后面调一调。')}
        {check('review', '每周自查', '计划书里定的那天（默认周日），她带着这周每条线做了多少过一遍，问要不要调下周。调下周要你点了才会用模型。')}
        <div class="row">
          <span class="label">几点开始问</span>
          <input
            type="time"
            value={s.review_at}
            disabled={!s.supervise || (!s.milestone_check && !s.review)}
            onChange={(e) => up({ review_at: e.currentTarget.value || '20:00' })}
            aria-label="节点、自查几点开始问"
          />
        </div>
      </div>
      <p class="muted small">
        她主动说话的总开关、安静时段、冷却时间在「设置 → 陪伴」里，那里的安静时段也管这里的分心提醒和晚上那一问。
        <button type="button" class="link-btn" onClick={() => api.openSettings().catch(onError)}>
          打开设置
        </button>
      </p>
    </section>
  );
}
