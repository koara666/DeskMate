// 「活动感知」：她会不会注意你在做什么、对哪些事可以说上一句、哪些要回避；活动日志查看与删除。
import { useCallback, useEffect, useState } from 'preact/hooks';
import type { SettingsApi } from '../lib/ipc';
import type { ActivityAction, ActivityCategory, ActivityOverview, ActivitySettings, SettingsView } from '../lib/types';
import { StatusLine, useStatus } from './SettingsApp';
import { VisionSection } from './VisionSection';

type Api = Pick<SettingsApi, 'activityOverview' | 'saveActivity' | 'deleteActivity'> &
  Partial<Pick<SettingsApi, 'saveVision' | 'visionCost' | 'visionPreview' | 'visionRecords'>>;

export const CATEGORIES: { id: ActivityCategory; name: string; def: ActivityAction }[] = [
  { id: 'game', name: '玩游戏', def: 'comment' },
  { id: 'coding', name: '写代码', def: 'comment' },
  { id: 'writing', name: '写作业 / 写东西', def: 'comment' },
  { id: 'video', name: '看视频', def: 'comment' },
  { id: 'music', name: '听音乐', def: 'quiet' },
  { id: 'browsing', name: '上网', def: 'quiet' },
  { id: 'chat', name: '聊天软件', def: 'quiet' },
  { id: 'meeting', name: '开会', def: 'quiet' },
  { id: 'other', name: '其他程序', def: 'quiet' },
];
const CAT_NAME: Record<string, string> = Object.fromEntries(CATEGORIES.map((c) => [c.id, c.name]));

const ACTIONS: { id: ActivityAction; name: string }[] = [
  { id: 'comment', name: '可以评论' },
  { id: 'quiet', name: '保持安静' },
  { id: 'avoid', name: '回避（藏起来）' },
];

function dur(secs: number) {
  const m = Math.max(1, Math.round(secs / 60));
  return m < 60 ? `${m} 分钟` : `${Math.floor(m / 60)} 小时${m % 60 ? ` ${m % 60} 分钟` : ''}`;
}

export function ActivityPage({ api, view }: { api: Api; view: SettingsView }) {
  const [a, setA] = useState<ActivitySettings>(view.activity);
  const [ov, setOv] = useState<ActivityOverview | null>(null);
  const [newPrivate, setNewPrivate] = useState('');
  const { status, run } = useStatus();

  const load = useCallback(() => {
    api.activityOverview().then(setOv).catch(() => {});
  }, [api]);
  useEffect(load, [load]);

  const save = async (next: ActivitySettings) => {
    setA(next);
    let saved: ActivitySettings | null = null;
    const ok = await run(async () => {
      saved = await api.saveActivity(next);
    });
    if (ok && saved) setA(saved);
    return ok;
  };
  const actionOf = (target: string, def: ActivityAction) => a.rules.find((r) => r.target === target)?.action ?? def;
  const setRule = (target: string, action: ActivityAction | null) => {
    const rules = a.rules.filter((r) => r.target !== target);
    if (action) rules.push({ target, action });
    save({ ...a, rules });
  };
  const exeRules = a.rules.filter((r) => r.target.endsWith('.exe'));

  return (
    <section>
      <h1>活动感知</h1>
      <p class="muted">
        她会看看你现在在用哪个程序（只在你的电脑上判断），偶尔说上一句：比如打了很久游戏、深夜还没睡、刚从写作业切到看视频。
        窗口标题不会被保存，也不会发给模型；发给模型的只有「在做什么 + 多久 + 程序名」。密码管理器、网银、浏览器隐身窗口在前台时，她会暂时藏起来，什么都不记。
      </p>

      <div class="card">
        <label class="row">
          <input type="checkbox" checked={a.enabled} onChange={(e) => save({ ...a, enabled: (e.currentTarget as HTMLInputElement).checked })} />
          <span>
            注意我在做什么
            <small>关掉后她不会看前台程序，也就不会评论、不会记录、「开始玩游戏时提醒我」也不会触发</small>
          </span>
        </label>
        <label class="row">
          <input type="checkbox" checked={a.comments} disabled={!a.enabled} onChange={(e) => save({ ...a, comments: (e.currentTarget as HTMLInputElement).checked })} />
          <span>
            偶尔说上一句
            <small>频率和安静时段在「陪伴」页统一设置</small>
          </span>
        </label>
        <label class="row">
          <input type="checkbox" checked={a.record} disabled={!a.enabled} onChange={(e) => save({ ...a, record: (e.currentTarget as HTMLInputElement).checked })} />
          <span>
            记录活动日志
            <small>只记「哪天、做什么、多久」，用于她的日记和关心你；可以在下面查看、删除</small>
          </span>
        </label>
        {ov?.current && (
          <p class="muted small">
            现在：{CAT_NAME[ov.current.category] ?? '用电脑'}
            {ov.current.label ? `（${ov.current.label}）` : ''}
          </p>
        )}
      </div>

      <div class="card">
        <h2>按类型</h2>
        {CATEGORIES.map((c) => (
          <div class="row" key={c.id}>
            <span class="label">{c.name}</span>
            <select aria-label={`${c.name}时`} value={actionOf(c.id, c.def)} onChange={(e) => setRule(c.id, (e.currentTarget as HTMLSelectElement).value as ActivityAction)}>
              {ACTIONS.map((x) => (
                <option key={x.id} value={x.id}>
                  {x.name}
                </option>
              ))}
            </select>
          </div>
        ))}
      </div>

      <div class="card">
        <h2>按程序</h2>
        <p class="muted small">单独设置某个程序（优先于上面的类型）。下面列出了最近用过的程序，只在本次运行期间保存在内存里。</p>
        <ul class="plain">
          {exeRules.map((r) => (
            <li key={r.target} class="row">
              <span class="grow mono">{r.target}</span>
              <select aria-label={`${r.target} 时`} value={r.action} onChange={(e) => setRule(r.target, (e.currentTarget as HTMLSelectElement).value as ActivityAction)}>
                {ACTIONS.map((x) => (
                  <option key={x.id} value={x.id}>
                    {x.name}
                  </option>
                ))}
              </select>
              <button type="button" class="btn btn-small" onClick={() => setRule(r.target, null)}>
                移除
              </button>
            </li>
          ))}
        </ul>
        {ov && ov.recent.length > 0 && (
          <>
            <h3>最近用过的程序</h3>
            <ul class="plain">
              {ov.recent
                .filter(([exe]) => !a.rules.some((r) => r.target === exe))
                .slice(0, 10)
                .map(([exe, cat]) => (
                  <li key={exe} class="row">
                    <span class="grow">
                      <span class="mono">{exe}</span> <small class="muted">{CAT_NAME[cat] ?? ''}</small>
                    </span>
                    <button type="button" class="btn btn-small" onClick={() => setRule(exe, 'quiet')}>
                      设为安静
                    </button>
                    <button type="button" class="btn btn-small" onClick={() => setRule(exe, 'avoid')}>
                      设为回避
                    </button>
                  </li>
                ))}
            </ul>
          </>
        )}
      </div>

      <div class="card">
        <h2>隐私名单</h2>
        <p class="muted small">
          内置：常见的密码管理器（KeePass、1Password、Bitwarden 等）、标题里带「隐身 / 无痕 / InPrivate / 网银 / 银行 / 支付宝」的窗口。这些在前台时完全不记录、不评论，她会暂时藏起来。可以再加程序：
        </p>
        <ul class="plain">
          {a.private_apps.map((p) => (
            <li key={p} class="row">
              <span class="grow mono">{p}</span>
              <button type="button" class="btn btn-small" onClick={() => save({ ...a, private_apps: a.private_apps.filter((x) => x !== p) })}>
                移除
              </button>
            </li>
          ))}
        </ul>
        <div class="row">
          <input class="grow" placeholder="进程名，比如 mybank.exe" aria-label="隐私程序" value={newPrivate} onInput={(e) => setNewPrivate((e.currentTarget as HTMLInputElement).value)} />
          <button
            type="button"
            class="btn btn-small"
            disabled={!newPrivate.trim().toLowerCase().endsWith('.exe')}
            onClick={async () => {
              if (await save({ ...a, private_apps: [...a.private_apps, newPrivate.trim().toLowerCase()] })) setNewPrivate('');
            }}
          >
            添加
          </button>
        </div>
      </div>

      <div class="card">
        <h2>活动日志（最近 7 天）</h2>
        {ov && ov.days.length === 0 && <div class="muted small">还没有记录。</div>}
        {ov?.days.map(([day, totals]) => (
          <div key={day} class="activity-day">
            <div class="row">
              <b class="grow">{day}</b>
              <button type="button" class="btn btn-small" onClick={() => run(() => api.deleteActivity(day), '已删除').then(load)}>
                删除这天
              </button>
            </div>
            <div class="muted small">
              {totals
                .filter((t) => t.category !== 'idle')
                .map((t) => `${CAT_NAME[t.category] ?? t.category}${t.label ? `（${t.label}）` : ''} ${dur(t.secs)}`)
                .join('、')}
            </div>
          </div>
        ))}
        {ov && ov.days.length > 0 && (
          <button type="button" class="btn btn-danger" onClick={() => run(() => api.deleteActivity('all'), '已全部删除').then(load)}>
            删除全部活动日志
          </button>
        )}
        <StatusLine status={status} />
      </div>

      {api.saveVision && api.visionCost && api.visionPreview && api.visionRecords && (
        <VisionSection
          api={{
            saveVision: api.saveVision,
            visionCost: api.visionCost,
            visionPreview: api.visionPreview,
            visionRecords: api.visionRecords,
          }}
          view={view}
          games={Array.from(
            new Set(
              (ov?.days ?? []).flatMap(([, totals]) => totals.filter((t) => t.category === 'game' && t.label).map((t) => t.label)),
            ),
          )}
        />
      )}
    </section>
  );
}
