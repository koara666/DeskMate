// 「陪伴」页里的每日播报设置：什么时候播、播什么、城市、新闻源、特殊日期。
import { useState } from 'preact/hooks';
import type { SettingsApi } from '../lib/ipc';
import type { BriefingSettings, City, Region } from '../lib/types';
import { StatusLine, useStatus } from './SettingsApp';

type Api = Pick<SettingsApi, 'saveBriefing' | 'citySearch' | 'briefingNow'>;

const MODULES: { key: keyof BriefingSettings; label: string; hint?: string }[] = [
  { key: 'weather', label: '天气和穿衣、带伞建议', hint: '需要先选城市' },
  { key: 'holiday', label: '今天是周末 / 节假日，放假时推荐玩什么' },
  { key: 'meal', label: '吃什么（只推荐菜式）' },
  { key: 'reminders', label: '今天的提醒' },
  { key: 'special', label: '生日、纪念日、节日' },
  { key: 'news', label: '新闻（你订阅的网站，默认关闭）' },
];

const REGIONS: { id: Region; label: string }[] = [
  { id: 'Auto', label: '跟着城市' },
  { id: 'CN', label: '中国' },
  { id: 'JP', label: '日本' },
  { id: 'US', label: '美国' },
  { id: 'Other', label: '只看周末' },
];

export function BriefingSection({ api, initial }: { api: Api; initial: BriefingSettings }) {
  const [b, setB] = useState<BriefingSettings>(initial);
  const [query, setQuery] = useState('');
  const [found, setFound] = useState<City[] | null>(null);
  const [newTime, setNewTime] = useState('08:00');
  const [feed, setFeed] = useState({ name: '', url: '' });
  const [day, setDay] = useState({ date: '', label: '' });
  const { status, setStatus, run } = useStatus();

  const save = async (next: BriefingSettings) => {
    setB(next);
    let saved: BriefingSettings | null = null;
    const ok = await run(async () => {
      saved = await api.saveBriefing(next);
    });
    if (ok && saved) setB(saved);
    return ok;
  };
  const toggle = (k: keyof BriefingSettings) => (e: Event) =>
    save({ ...b, [k]: (e.currentTarget as HTMLInputElement).checked });

  const search = async () => {
    setFound(null);
    try {
      setFound(await api.citySearch(query));
    } catch (e) {
      setStatus({ ok: false, text: String(e) });
    }
  };

  return (
    <>
      <div class="card">
        <h2>每日播报</h2>
        <p class="muted small">
          她会跟你说说天气、今天是什么日子、吃什么、今天的提醒。每次挑选的内容和顺序都不太一样。有模型时整份只调用一次模型；没有模型或今天的额度用完时，用预先写好的台词说。
        </p>
        <label class="row">
          <input type="checkbox" checked={b.on_startup} onChange={toggle('on_startup')} />
          <span>
            每天第一次打开电脑时播报
            <small>凌晨 5 点前还没睡的话算前一天</small>
          </span>
        </label>
        <div class="row">
          <span class="label">另外的时间点</span>
          {b.times.map((t) => (
            <span key={t} class="chip">
              {t}
              <button type="button" class="link-btn" aria-label={`删除 ${t}`} onClick={() => save({ ...b, times: b.times.filter((x) => x !== t) })}>
                ✕
              </button>
            </span>
          ))}
          <input type="time" value={newTime} aria-label="新的播报时间" onInput={(e) => setNewTime((e.currentTarget as HTMLInputElement).value)} />
          <button type="button" class="btn btn-small" disabled={!newTime} onClick={() => save({ ...b, times: [...b.times, newTime] })}>
            添加
          </button>
        </div>
        <p class="muted small">晚上 9 点以后的时间点是「睡前问候」：说说明天的天气和提醒。</p>
        <h3>播报内容</h3>
        {MODULES.map((m) => (
          <label class="row" key={m.key}>
            <input type="checkbox" checked={Boolean(b[m.key])} onChange={toggle(m.key)} />
            <span>
              {m.label}
              {m.hint && <small>{m.hint}</small>}
            </span>
          </label>
        ))}
        <label class="row">
          <input type="checkbox" checked={b.weather_in_chat} onChange={toggle('weather_in_chat')} />
          <span>
            聊天里问天气时帮你查
            <small>比如「明天会下雨吗」「大阪冷不冷」</small>
          </span>
        </label>
        <div class="row">
          <button type="button" class="btn" onClick={() => run(() => api.briefingNow(), '她马上就说')}>
            现在播报一次
          </button>
        </div>
        <StatusLine status={status} />
      </div>

      <div class="card">
        <h2>你所在的城市</h2>
        <p class="muted small">
          用来查天气（数据来自 Open-Meteo，免费、不需要账号；查询时只发送这个城市的经纬度）。城市只保存在你的电脑上。
        </p>
        <div class="row">
          <span class="grow">{b.city ? `现在：${b.city.name}（${b.city.country}）` : '还没有选城市'}</span>
          {b.city && (
            <button type="button" class="btn btn-small" onClick={() => save({ ...b, city: null })}>
              清除
            </button>
          )}
        </div>
        <div class="row">
          <input
            class="grow"
            value={query}
            placeholder="城市名，比如 上海、大阪；不在列表里的输英文或拼音"
            aria-label="城市"
            onInput={(e) => setQuery((e.currentTarget as HTMLInputElement).value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && !e.isComposing) search();
            }}
          />
          <button type="button" class="btn btn-small" disabled={query.trim().length < 1} onClick={search}>
            查找
          </button>
        </div>
        {found && found.length === 0 && <div class="muted small">没找到。</div>}
        {found && found.length > 0 && (
          <ul class="plain">
            {found.map((c) => (
              <li key={`${c.lat},${c.lon}`} class="row">
                <span class="grow">
                  {c.name} <small class="muted">{c.country}</small>
                </span>
                <button
                  type="button"
                  class="btn btn-small"
                  onClick={async () => {
                    if (await save({ ...b, city: c })) {
                      setFound(null);
                      setQuery('');
                    }
                  }}
                >
                  选这个
                </button>
              </li>
            ))}
          </ul>
        )}
        <div class="row">
          <span class="label">节假日按</span>
          <select aria-label="节假日地区" value={b.region} onChange={(e) => save({ ...b, region: (e.currentTarget as HTMLSelectElement).value as Region })}>
            {REGIONS.map((r) => (
              <option key={r.id} value={r.id}>
                {r.label}
              </option>
            ))}
          </select>
        </div>
      </div>

      <div class="card">
        <h2>生日和纪念日</h2>
        <p class="muted small">到了那天她会特别问候你。在聊天里告诉她也可以（以后的版本会自动记下）。</p>
        <ul class="plain">
          {b.special_dates.map((d) => (
            <li key={d.date + d.label} class="row">
              <span class="grow">
                {d.date.replace('-', ' 月 ')} 日 · {d.label}
              </span>
              <button type="button" class="btn btn-small" onClick={() => save({ ...b, special_dates: b.special_dates.filter((x) => x !== d) })}>
                删除
              </button>
            </li>
          ))}
        </ul>
        <div class="row">
          <input type="date" aria-label="日期" value={day.date} onInput={(e) => setDay({ ...day, date: (e.currentTarget as HTMLInputElement).value })} />
          <input
            placeholder="是什么日子，比如 生日"
            aria-label="日子名称"
            value={day.label}
            onInput={(e) => setDay({ ...day, label: (e.currentTarget as HTMLInputElement).value })}
          />
          <button
            type="button"
            class="btn btn-small"
            disabled={!day.date || !day.label.trim()}
            onClick={async () => {
              if (await save({ ...b, special_dates: [...b.special_dates, { date: day.date.slice(5), label: day.label.trim() }] }))
                setDay({ date: '', label: '' });
            }}
          >
            添加
          </button>
        </div>
      </div>

      <div class="card">
        <h2>新闻源</h2>
        <p class="muted small">只读取你打开的源，每次挑两三条标题简单说说。新闻标题来自网络，她只会转述。</p>
        <ul class="plain">
          {b.feeds.map((f) => (
            <li key={f.url} class="row">
              <label class="grow">
                <input
                  type="checkbox"
                  checked={f.enabled}
                  onChange={(e) =>
                    save({
                      ...b,
                      feeds: b.feeds.map((x) => (x === f ? { ...x, enabled: (e.currentTarget as HTMLInputElement).checked } : x)),
                    })
                  }
                />{' '}
                {f.name} <small class="muted mono">{f.url}</small>
              </label>
              <button type="button" class="btn btn-small" onClick={() => save({ ...b, feeds: b.feeds.filter((x) => x !== f) })}>
                删除
              </button>
            </li>
          ))}
        </ul>
        <div class="row">
          <input placeholder="名字" aria-label="新闻源名字" value={feed.name} onInput={(e) => setFeed({ ...feed, name: (e.currentTarget as HTMLInputElement).value })} />
          <input
            class="grow"
            placeholder="RSS 地址（https://…）"
            aria-label="新闻源地址"
            value={feed.url}
            onInput={(e) => setFeed({ ...feed, url: (e.currentTarget as HTMLInputElement).value })}
          />
          <button
            type="button"
            class="btn btn-small"
            disabled={!feed.url.trim()}
            onClick={async () => {
              if (await save({ ...b, feeds: [...b.feeds, { name: feed.name.trim() || '新闻', url: feed.url.trim(), enabled: true }] }))
                setFeed({ name: '', url: '' });
            }}
          >
            添加
          </button>
        </div>
      </div>
    </>
  );
}
