// 「活动感知」页最下面：第三层「看一眼屏幕」（默认关闭，有成本）。
import { useEffect, useState } from 'preact/hooks';
import type { SettingsApi } from '../lib/ipc';
import type { SettingsView, VisionSettings } from '../lib/types';
import { StatusLine, useStatus } from './SettingsApp';

const IMAGE_KINDS = new Set(['anthropic', 'gemini', 'openai_compat']);

interface Props {
  api: Pick<SettingsApi, 'saveVision' | 'visionCost'>;
  view: SettingsView;
  /** 最近玩过的游戏（活动日志里的游戏名） */
  games: string[];
}

export function VisionSection({ api, view, games }: Props) {
  const [v, setV] = useState<VisionSettings>(view.vision);
  const [cost, setCost] = useState<[number, number] | null>(null);
  const { status, run } = useStatus();
  useEffect(() => {
    api.visionCost().then(setCost).catch(() => {});
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
  const allGames = Array.from(new Set([...v.games, ...games]));

  return (
    <div class="card">
      <h2>看一眼屏幕（第三层，默认关闭）</h2>
      <p class="muted small">
        打开后，按快捷键可以让她看一眼你的屏幕（比如「这局打得怎么样」），也可以对某个游戏允许她偶尔看看。截图只在内存里，发给你选的模型后立刻丢掉，不保存、不进记忆，她看完说的话也不进聊天记录；密码管理器、网银、隐身窗口在前台时不看。看图比聊天贵一些。
      </p>
      <label class="row">
        <input
          type="checkbox"
          checked={v.enabled}
          disabled={models.length === 0}
          onChange={(e) => save({ ...v, enabled: (e.currentTarget as HTMLInputElement).checked, backend: v.backend || models[0]?.config.id || '' })}
        />
        <span>
          允许她看屏幕
          <small>{models.length === 0 ? '需要先在「模型」里配置 Claude、Gemini 或支持图片的 OpenAI 兼容模型' : '只在你按快捷键、或你允许的游戏里偶尔看'}</small>
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
          <h3>允许偶尔看画面的游戏</h3>
          <p class="muted small">玩这些游戏（窗口模式）时，她每隔一阵会看一眼、说句话；全屏时不看。</p>
          {allGames.length === 0 && <div class="muted small">还没有玩过的游戏记录。玩过以后这里会列出来。</div>}
          {allGames.map((g) => (
            <label class="row" key={g}>
              <input
                type="checkbox"
                checked={v.games.includes(g)}
                onChange={(e) =>
                  save({
                    ...v,
                    games: (e.currentTarget as HTMLInputElement).checked ? [...v.games, g] : v.games.filter((x) => x !== g),
                  })
                }
              />
              <span>{g}</span>
            </label>
          ))}
          <div class="row">
            <label>
              同一个游戏至少隔（分钟）
              <input
                type="number"
                min={10}
                max={240}
                value={v.game_interval_minutes}
                onChange={(e) => save({ ...v, game_interval_minutes: Number((e.currentTarget as HTMLInputElement).value) || 30 })}
              />
            </label>
            <label>
              每天最多看几次
              <input
                type="number"
                min={0}
                max={20}
                value={v.daily_max}
                onChange={(e) => save({ ...v, daily_max: Number((e.currentTarget as HTMLInputElement).value) || 0 })}
              />
            </label>
          </div>
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
