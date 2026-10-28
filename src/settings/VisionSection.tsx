// 「活动感知」页最下面：第三层「看一眼屏幕」（默认关闭，有成本）。
// 隐私保护（敏感窗口不看、只截那一个窗口、本机涂黑敏感文字）在后端做，这里能预览她会看到什么。
import { useEffect, useState } from 'preact/hooks';
import type { SettingsApi } from '../lib/ipc';
import type { LookRecord, SettingsView, VisionPreview, VisionSettings } from '../lib/types';
import { StatusLine, useStatus } from './SettingsApp';

const IMAGE_KINDS = new Set(['anthropic', 'gemini', 'openai_compat']);
const AUTO: { id: string; name: string; hint: string }[] = [
  { id: 'game', name: '玩游戏时', hint: '最推荐：她会对局势、画面说两句' },
  { id: 'video', name: '看视频时', hint: '' },
  { id: 'coding', name: '写代码时', hint: '代码里的密钥会被涂黑，但还是建议只在需要时打开' },
  { id: 'writing', name: '写东西时', hint: '' },
];
const OUTCOME: Record<string, string> = {
  sent: '看了',
  preview: '预览',
  private: '没看：隐私窗口',
  sensitive_app: '没看：聊天 / 邮件 / 支付类程序',
  sensitive_title: '没看：登录 / 账户 / 付款页面',
  password: '没看：正在输密码',
  risky_text: '没看：画面上有账户或付款信息',
  nothing: '没看：前面只有桌面',
  error: '没看成',
};
const TRIGGER: Record<string, string> = { hotkey: '你让她看', auto: '她主动看', preview: '预览' };

interface Props {
  api: Pick<SettingsApi, 'saveVision' | 'visionCost' | 'visionPreview' | 'visionRecords'>;
  view: SettingsView;
  /** 最近玩过的游戏（活动日志里的游戏名） */
  games: string[];
}

export function VisionSection({ api, view, games }: Props) {
  const [v, setV] = useState<VisionSettings>(view.vision);
  const [cost, setCost] = useState<[number, number] | null>(null);
  const [records, setRecords] = useState<LookRecord[]>([]);
  const [preview, setPreview] = useState<VisionPreview | null>(null);
  const [countdown, setCountdown] = useState(0);
  const { status, run } = useStatus();
  const loadRecords = () => api.visionRecords().then(setRecords).catch(() => {});
  useEffect(() => {
    api.visionCost().then(setCost).catch(() => {});
    loadRecords();
  }, [api]);

  const models = view.backends.filter((b) => IMAGE_KINDS.has(b.config.kind));
  const save = async (next: VisionSettings) => {
    setV(next);
    let saved: VisionSettings | null = null;
    const ok = await run(async () => {
      saved = await api.saveVision(next);
    });
    if (ok && saved) setV(saved);
    else if (!ok) setV(v);
  };
  const toggle = (list: string[], x: string, on: boolean) => (on ? [...list, x] : list.filter((y) => y !== x));
  const allGames = Array.from(new Set([...v.exclude, ...games]));

  // 倒计时 3 秒（给你时间切到想让她看的窗口），然后截一次、做完隐私处理，只在这里显示
  const startPreview = () => {
    setPreview(null);
    setCountdown(3);
    let n = 3;
    const t = setInterval(() => {
      n -= 1;
      setCountdown(n);
      if (n > 0) return;
      clearInterval(t);
      api
        .visionPreview()
        .then((p) => {
          setPreview(p);
          loadRecords();
        })
        .catch(() => {});
    }, 1000);
  };

  return (
    <div class="card">
      <h2>看一眼屏幕（第三层，默认关闭）</h2>
      <p class="muted small">
        打开后，她会在你玩游戏时偶尔看一眼、说句话，你也可以按快捷键让她看。看图比聊天贵一些，用的是你选的模型。
      </p>
      <div class="privacy-box">
        <strong>怎么保护你的隐私（全部在这台电脑上完成）</strong>
        <ul>
          <li>聊天软件（QQ、微信……）、邮件、支付和证券软件、远程桌面、系统设置一律不看；标题里有登录、密码、邮箱、支付、订单、账户、身份证等字样的网页不看；正在输密码时不看。</li>
          <li>只截你正在用的那一个窗口，不截整个屏幕、旁边的窗口和任务栏。</li>
          <li>发出去之前，先用 Windows 自带的文字识别（不联网）把邮箱、手机号、证件号、卡号、金额、像密钥的字符串、带「密码 / 验证码 / 账号」的那一行涂黑；她主动看时发现这类内容就干脆不看。</li>
          <li>截图只在内存里，发给模型后立刻丢掉，不保存、不进记忆；她看完说的话也不进聊天记录。</li>
        </ul>
      </div>
      <label class="row">
        <input
          type="checkbox"
          checked={v.enabled}
          disabled={models.length === 0}
          onChange={(e) => save({ ...v, enabled: (e.currentTarget as HTMLInputElement).checked, backend: v.backend || models[0]?.config.id || '' })}
        />
        <span>
          允许她看屏幕
          <small>{models.length === 0 ? '需要先在「模型」里配置 Claude、Gemini 或支持图片的 OpenAI 兼容模型' : '选快一点的模型（比如 Gemini Flash、Claude Haiku）她会回得更快'}</small>
        </span>
      </label>
      {v.enabled && (
        <>
          <div class="row">
            <span class="label">用哪个模型看</span>
            <select aria-label="看图用的模型" value={v.backend} onChange={(e) => save({ ...v, backend: (e.currentTarget as HTMLSelectElement).value })}>
              {models.map((m) => (
                <option key={m.config.id} value={m.config.id}>
                  {m.config.name}
                </option>
              ))}
            </select>
          </div>
          <div class="row">
            <span class="label">「看一眼」快捷键</span>
            <input value={v.hotkey} aria-label="看一眼快捷键" onInput={(e) => setV({ ...v, hotkey: (e.currentTarget as HTMLInputElement).value })} />
            <button type="button" class="btn btn-small" onClick={() => save(v)}>
              应用
            </button>
          </div>

          <h3>她什么时候主动看</h3>
          <p class="muted small">做这些事超过 5 分钟后，她每隔一阵看一眼、说句话；全屏时不看（气泡显示不出来）。聊天、上网、其他程序从不主动看。</p>
          {AUTO.map((a) => (
            <label class="row" key={a.id}>
              <input
                type="checkbox"
                checked={v.auto_categories.includes(a.id)}
                onChange={(e) => save({ ...v, auto_categories: toggle(v.auto_categories, a.id, (e.currentTarget as HTMLInputElement).checked) })}
              />
              <span>
                {a.name}
                {a.hint && <small>{a.hint}</small>}
              </span>
            </label>
          ))}
          <div class="row">
            <label>
              两次之间至少隔（分钟）
              <input
                type="number"
                min={10}
                max={240}
                value={v.interval_minutes}
                onChange={(e) => save({ ...v, interval_minutes: Number((e.currentTarget as HTMLInputElement).value) || 20 })}
              />
            </label>
            <label>
              每天最多主动看几次
              <input
                type="number"
                min={0}
                max={20}
                value={v.daily_max}
                onChange={(e) => save({ ...v, daily_max: Number((e.currentTarget as HTMLInputElement).value) || 0 })}
              />
            </label>
          </div>
          {allGames.length > 0 && (
            <>
              <h4>这些游戏不要主动看</h4>
              {allGames.map((g) => (
                <label class="row" key={g}>
                  <input
                    type="checkbox"
                    checked={v.exclude.includes(g)}
                    onChange={(e) => save({ ...v, exclude: toggle(v.exclude, g, (e.currentTarget as HTMLInputElement).checked) })}
                  />
                  <span>{g}</span>
                </label>
              ))}
            </>
          )}
          <label class="row">
            <input type="checkbox" checked={v.mask_text} onChange={(e) => save({ ...v, mask_text: (e.currentTarget as HTMLInputElement).checked })} />
            <span>
              发出去前涂黑敏感文字
              <small>强烈建议保持打开</small>
            </span>
          </label>

          <h3>预览她会看到什么</h3>
          <p class="muted small">点了以后有 3 秒切到想让她看的窗口（不切就看这个设置窗口后面那个）。和她真正看的时候处理方式一样，但图只在这里显示，不发送、不保存，关掉就没了。</p>
          <button type="button" class="btn" disabled={countdown > 0} onClick={startPreview}>
            {countdown > 0 ? `${countdown}…` : '预览一次'}
          </button>
          {preview && (
            <div class="vision-preview">
              {preview.refused ? (
                <p>她会说：「{preview.refused}」（不看）</p>
              ) : (
                <>
                  <p class="muted small">
                    看的是「{preview.app}」{preview.masked > 0 ? `，涂黑了 ${preview.masked} 处` : '，没有需要涂黑的文字'}
                    {preview.ocr_missing ? '。这台电脑没有可用的文字识别，没能涂黑文字。' : '。'}
                  </p>
                  {preview.image && <img src={preview.image} alt="她会看到的画面" />}
                  <button type="button" class="btn btn-small btn-plain" onClick={() => setPreview(null)}>
                    关掉预览
                  </button>
                </>
              )}
            </div>
          )}

          {records.length > 0 && (
            <>
              <h3>最近看过的（这次打开 DeskMate 以来，不存图）</h3>
              <ul class="look-records">
                {records.map((r, i) => (
                  <li key={i}>
                    {new Date(r.at * 1000).toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })} · {TRIGGER[r.trigger] ?? r.trigger} ·{' '}
                    {OUTCOME[r.outcome] ?? r.outcome}
                    {r.app && `「${r.app}」`}
                    {r.masked > 0 && ` · 涂黑 ${r.masked} 处`}
                  </li>
                ))}
              </ul>
            </>
          )}
        </>
      )}
      {cost && (
        <p class="muted small">
          累计看了 {cost[0]} 次，大约花了 ${cost[1].toFixed(4)}。
        </p>
      )}
      <StatusLine status={status} />
    </div>
  );
}
