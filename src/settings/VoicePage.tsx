// 设置「声音」页（v2.1 P14）：选引擎、填 Key、试听、哪些场合出声、音量语速。默认关闭。
import { useEffect, useState } from 'preact/hooks';
import type { SettingsApi } from '../lib/ipc';
import type { SettingsView, VoiceSettings } from '../lib/types';
import { StatusLine, useStatus } from './SettingsApp';

type Api = Pick<SettingsApi, 'voiceSave' | 'voiceSetKey' | 'voiceClearKey' | 'voiceTest' | 'voiceClearCache'>;

const ENGINES: { id: string; name: string; desc: string }[] = [
  {
    id: 'fish',
    name: 'Fish Audio（推荐：有现成的七海千秋声音）',
    desc: '云端合成。社区里有人做好了七海千秋的声音模型，选好就能用；默认用免费的合成模型（s2.1-pro-free），也可以换成付费的（更稳定，按字数计费，大约每 1 万个汉字 3 元多，一句话不到 1 分钱）。需要注册 Fish Audio 拿一个 Key。',
  },
  {
    id: 'system',
    name: 'Windows 自带的语音（免费、不联网）',
    desc: '开箱就能用，但声音是系统的朗读声音，不像她。适合先试试效果。',
  },
  {
    id: 'sovits',
    name: 'GPT-SoVITS（在你自己电脑上运行）',
    desc: '免费、不联网、声音最像（用她的原声训练的模型）。要先在电脑上运行它的语音服务（api_v2.py，有显卡最好），步骤见 NEEDS_USER 第 13 项。',
  },
  {
    id: 'openai',
    name: 'OpenAI 兼容的语音接口',
    desc: '填接口地址、模型和声音名（比如 OpenAI 的 gpt-4o-mini-tts / nova）。声音不是她，但很自然。',
  },
];

const FISH_PRESETS = [
  { id: 'd5e032724f8149629ec6d03735ec4c85', name: '七海千秋（日语配音风格）' },
  { id: '3d3bed2283b249a684cd8cd79dafe3e8', name: '七海千秋（英语配音风格）' },
];

export function VoicePage({ api, view }: { api: Api; view: SettingsView }) {
  const [v, setV] = useState<VoiceSettings>(view.voice);
  const [keys, setKeys] = useState<[string | null, string | null]>(view.voice_keys);
  const [keyInput, setKeyInput] = useState('');
  const [voices, setVoices] = useState<string[]>([]);
  const { status, run } = useStatus();

  useEffect(() => {
    if (typeof speechSynthesis === 'undefined') return;
    const load = () => setVoices(speechSynthesis.getVoices().map((x) => `${x.name}`));
    load();
    speechSynthesis.addEventListener?.('voiceschanged', load);
    return () => speechSynthesis.removeEventListener?.('voiceschanged', load);
  }, []);

  const save = async (next: VoiceSettings) => {
    setV(next);
    let saved: VoiceSettings | null = null;
    const ok = await run(async () => {
      saved = await api.voiceSave(next);
    });
    if (ok && saved) setV(saved);
  };
  const num = (e: Event) => Number((e.currentTarget as HTMLInputElement).value);
  const keyFor = v.engine === 'fish' ? 'fish' : v.engine === 'openai' ? 'openai' : null;
  const keyHint = keyFor === 'fish' ? keys[0] : keyFor === 'openai' ? keys[1] : null;

  return (
    <section>
      <h1>声音</h1>
      <div class="card">
        <p class="muted small">
          打开后她说话时会出声。只读精简后的一两句（括号里的动作、表情、网址、代码不读），新的一句会打断上一句。合成过的句子会缓存在本机，同一句话不会重复花钱。
        </p>
        <label class="row">
          <input
            type="checkbox"
            checked={v.enabled}
            disabled={!v.engine}
            onChange={(e) => save({ ...v, enabled: (e.currentTarget as HTMLInputElement).checked })}
          />
          <span>
            让她出声
            <small>{v.engine ? '右键她头顶的气泡里也能开关' : '先在下面选一个声音引擎'}</small>
          </span>
        </label>
      </div>

      <div class="card">
        <h2>用什么声音</h2>
        {ENGINES.map((e) => (
          <label class="row engine-choice" key={e.id}>
            <input type="radio" name="engine" checked={v.engine === e.id} onChange={() => save({ ...v, engine: e.id })} />
            <span>
              {e.name}
              <small>{e.desc}</small>
            </span>
          </label>
        ))}
      </div>

      {v.engine && (
        <div class="card">
          <h2>她说话的语言</h2>
          {(
            [
              ['ja', '日语（气泡照常显示中文，像看番的字幕）', '她的原声是日语，日语最像她本人。预写的台词带日语版；聊天回复由模型顺便附上日语（不多花一次请求）；都没有时用聊天模型翻译要念的那一两句，翻译不了就念中文。'],
              ['zh', '中文', '念气泡里的中文。'],
            ] as const
          ).map(([id, name, desc]) => (
            <label class="row" key={id}>
              <input type="radio" name="voice-lang" checked={v.lang === id} onChange={() => save({ ...v, lang: id })} />
              <span>
                {name}
                <small>{desc}</small>
              </span>
            </label>
          ))}
        </div>
      )}

      {v.engine === 'fish' && (
        <div class="card">
          <h2>Fish Audio</h2>
          <div class="row">
            <span class="label">她的声音</span>
            <select
              aria-label="Fish Audio 声音模型"
              value={FISH_PRESETS.some((p) => p.id === v.fish_model) ? v.fish_model : 'custom'}
              onChange={(e) => {
                const id = (e.currentTarget as HTMLSelectElement).value;
                if (id !== 'custom') save({ ...v, fish_model: id });
              }}
            >
              {FISH_PRESETS.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
              <option value="custom">自己填模型编号…</option>
            </select>
          </div>
          <div class="row">
            <span class="label">模型编号</span>
            <input class="mono" value={v.fish_model} onChange={(e) => save({ ...v, fish_model: (e.currentTarget as HTMLInputElement).value })} />
          </div>
          <p class="muted small">在 fish.audio 上找到喜欢的声音，网址 fish.audio/m/后面那一串就是模型编号。这些社区声音模型是别人上传的，Fish Audio 可能随时下架。</p>
          <div class="row">
            <span class="label">合成模型</span>
            <select aria-label="Fish 合成模型" value={v.fish_engine} onChange={(e) => save({ ...v, fish_engine: (e.currentTarget as HTMLSelectElement).value })}>
              <option value="s2.1-pro-free">s2.1-pro-free（免费）</option>
              <option value="s2.1-pro">s2.1-pro（付费，更稳定）</option>
              <option value="s1">s1（付费，旧版）</option>
            </select>
          </div>
        </div>
      )}

      {v.engine === 'system' && (
        <div class="card">
          <h2>Windows 语音</h2>
          <div class="row">
            <span class="label">声音</span>
            <select aria-label="系统声音" value={v.system_voice} onChange={(e) => save({ ...v, system_voice: (e.currentTarget as HTMLSelectElement).value })}>
              <option value="">自动（按说话的语言挑）</option>
              {voices.map((n) => (
                <option key={n} value={n}>
                  {n}
                </option>
              ))}
            </select>
          </div>
          <div class="row">
            <label>
              音调（越大越尖，{v.system_pitch.toFixed(2)}）
              <input type="range" min={0.5} max={2} step={0.05} value={v.system_pitch} onChange={(e) => save({ ...v, system_pitch: num(e) })} />
            </label>
          </div>
        </div>
      )}

      {v.engine === 'sovits' && (
        <div class="card">
          <h2>GPT-SoVITS（本机服务）</h2>
          <div class="row">
            <span class="label">接口地址</span>
            <input class="mono" value={v.sovits_url} onChange={(e) => save({ ...v, sovits_url: (e.currentTarget as HTMLInputElement).value })} />
          </div>
          <div class="row">
            <span class="label">参考音频</span>
            <input
              class="mono"
              placeholder="D:\\GPT-SoVITS\\ref\\chiaki.wav"
              value={v.sovits_ref_audio}
              onChange={(e) => save({ ...v, sovits_ref_audio: (e.currentTarget as HTMLInputElement).value })}
            />
          </div>
          <div class="row">
            <span class="label">参考音频说的话</span>
            <input value={v.sovits_ref_text} onChange={(e) => save({ ...v, sovits_ref_text: (e.currentTarget as HTMLInputElement).value })} />
          </div>
          <div class="row">
            <span class="label">参考音频的语言</span>
            <select aria-label="参考音频语言" value={v.sovits_ref_lang} onChange={(e) => save({ ...v, sovits_ref_lang: (e.currentTarget as HTMLSelectElement).value })}>
              <option value="ja">日语</option>
              <option value="zh">中文</option>
              <option value="en">英语</option>
            </select>
          </div>
          <p class="muted small">参考音频是一段 3~10 秒的她的原声（文件路径由 GPT-SoVITS 那边读取），「参考音频说的话」要一字不差。只能连本机地址。</p>
        </div>
      )}

      {v.engine === 'openai' && (
        <div class="card">
          <h2>OpenAI 兼容接口</h2>
          <div class="row">
            <span class="label">接口地址</span>
            <input class="mono" value={v.openai_url} onChange={(e) => save({ ...v, openai_url: (e.currentTarget as HTMLInputElement).value })} />
          </div>
          <div class="row">
            <span class="label">模型</span>
            <input class="mono" value={v.openai_model} onChange={(e) => save({ ...v, openai_model: (e.currentTarget as HTMLInputElement).value })} />
          </div>
          <div class="row">
            <span class="label">声音</span>
            <input class="mono" value={v.openai_voice} onChange={(e) => save({ ...v, openai_voice: (e.currentTarget as HTMLInputElement).value })} />
          </div>
        </div>
      )}

      {keyFor && (
        <div class="card">
          <h2>Key</h2>
          <p class="muted small">只保存在 Windows 凭据管理器里，界面上只显示最后 4 位。</p>
          <div class="row">
            <span class="label">{keyHint ? `已保存（…${keyHint}）` : '还没填'}</span>
            <input type="password" aria-label="语音服务 Key" value={keyInput} onInput={(e) => setKeyInput((e.currentTarget as HTMLInputElement).value)} />
            <button
              type="button"
              class="btn btn-small"
              disabled={!keyInput.trim()}
              onClick={() =>
                run(async () => {
                  const hint = await api.voiceSetKey(keyFor, keyInput);
                  setKeys((k) => (keyFor === 'fish' ? [hint, k[1]] : [k[0], hint]));
                  setKeyInput('');
                }, 'Key 已保存')
              }
            >
              保存 Key
            </button>
            {keyHint && (
              <button
                type="button"
                class="btn btn-small btn-plain"
                onClick={() =>
                  run(async () => {
                    await api.voiceClearKey(keyFor);
                    setKeys((k) => (keyFor === 'fish' ? [null, k[1]] : [k[0], null]));
                  }, 'Key 已删除')
                }
              >
                删除
              </button>
            )}
          </div>
          {keyFor === 'fish' && <p class="muted small">怎么拿：fish.audio 注册登录 → 右上角头像 → API Keys → 新建一个，复制过来。详细步骤见 NEEDS_USER 第 11 项。</p>}
        </div>
      )}

      {v.engine && (
        <div class="card">
          <h2>试听和细节</h2>
          <div class="row">
            <button type="button" class="btn" onClick={() => run(() => api.voiceTest(), '她应该在说话了（桌宠那边出声）')}>
              试听一句
            </button>
          </div>
          <div class="row">
            <label>
              音量（{v.volume}）
              <input type="range" min={0} max={100} value={v.volume} onChange={(e) => save({ ...v, volume: num(e) })} />
            </label>
            <label>
              语速（{v.speed.toFixed(2)}）
              <input type="range" min={0.5} max={2} step={0.05} value={v.speed} onChange={(e) => save({ ...v, speed: num(e) })} />
            </label>
          </div>
          <div class="row">
            <label>
              每次最多读几个字
              <input type="number" min={10} max={120} value={v.max_chars} onChange={(e) => save({ ...v, max_chars: num(e) || 40 })} />
            </label>
            <label>
              付费引擎每天最多合成几个字
              <input type="number" min={0} max={100000} value={v.daily_chars} onChange={(e) => save({ ...v, daily_chars: num(e) || 0 })} />
            </label>
          </div>
          <h3>哪些时候出声</h3>
          {(
            [
              ['reactions', '摸头、拖动、戳她时的小反应'],
              ['proactive', '她主动说的话（播报、提醒、关怀、看画面）'],
              ['chat', '聊天回复（只读开头一两句）'],
              ['games', '小游戏里的台词'],
            ] as const
          ).map(([k, label]) => (
            <label class="row" key={k}>
              <input
                type="checkbox"
                checked={v.scenes[k]}
                onChange={(e) => save({ ...v, scenes: { ...v.scenes, [k]: (e.currentTarget as HTMLInputElement).checked } })}
              />
              <span>{label}</span>
            </label>
          ))}
          <label class="row">
            <input type="checkbox" checked={v.fullscreen_voice} onChange={(e) => save({ ...v, fullscreen_voice: (e.currentTarget as HTMLInputElement).checked })} />
            <span>
              全屏时（比如玩全屏游戏）也用声音说她主动的话
              <small>全屏时她是藏起来的，气泡看不到；打开后只出声、不弹窗。勿扰时不出声。</small>
            </span>
          </label>
          <div class="row">
            <button type="button" class="btn btn-small btn-plain" onClick={() => run(async () => {
              const n = await api.voiceClearCache();
              return void n;
            }, '缓存已清空')}>
              清空语音缓存
            </button>
          </div>
        </div>
      )}
      <StatusLine status={status} />
    </section>
  );
}
