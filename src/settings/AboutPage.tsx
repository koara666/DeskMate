import { useState } from 'preact/hooks';
import type { SettingsApi, UpdateApi } from '../lib/ipc';
import type { SettingsView, UpdateView } from '../lib/types';
import { UpdateCard } from '../update/UpdateCard';

type Api = Pick<SettingsApi, 'openDataDir' | 'updateCheck' | 'updateSave'>;

export function AboutPage({ api, view, updateApi }: { api: Api; view: SettingsView; updateApi?: UpdateApi }) {
  const [auto, setAuto] = useState(view.update.auto_check);
  const [checking, setChecking] = useState(false);
  const [result, setResult] = useState<{ found: UpdateView | null; error?: string } | null>(null);
  const check = async () => {
    setChecking(true);
    try {
      setResult({ found: await api.updateCheck() });
    } catch (e) {
      setResult({ found: null, error: String(e) });
    } finally {
      setChecking(false);
    }
  };
  return (
    <section>
      <h1>关于与隐私</h1>
      <div class="card">
        <div class="row">
          <span class="label">版本</span>
          <span class="mono">{view.version}</span>
          <button type="button" class="btn btn-small" disabled={checking} onClick={check}>
            {checking ? '正在检查…' : '检查更新'}
          </button>
        </div>
        {result &&
          (result.error ? (
            <p class="muted small">{result.error}</p>
          ) : result.found ? (
            updateApi && <UpdateCard api={updateApi} view={result.found} />
          ) : (
            <p class="muted small">已经是最新版本了。</p>
          ))}
        <label class="row">
          <input
            type="checkbox"
            checked={auto}
            onChange={(e) => {
              const on = (e.currentTarget as HTMLInputElement).checked;
              setAuto(on);
              api.updateSave(on).catch(() => setAuto(!on));
            }}
          />
          <span>
            打开 DeskMate 时检查有没有新版本
            <small>每天最多一次，只向 GitHub 查询最新的版本号（不发送任何你的信息）；有新版本时她会告诉你，你点了才会下载安装</small>
          </span>
        </label>
        <div class="row">
          <span class="label">数据目录</span>
          <span class="mono selectable">{view.data_dir}</span>
          <button type="button" class="btn btn-small" onClick={() => api.openDataDir().catch(() => {})}>
            打开
          </button>
        </div>
      </div>
      <div class="card">
        <h2>你的数据去了哪里</h2>
        <ul class="plain">
          <li>聊天记录和记忆只保存在你电脑上的数据目录里（一个数据库文件）。</li>
          <li>API Key 只保存在 Windows 凭据管理器里，不写进任何文件。</li>
          <li>
            DeskMate 不收集任何使用数据，也不会悄悄更新。会连接的只有：你在「模型」里配置的服务；选了城市后查天气时的
            Open-Meteo（只发送城市的经纬度）；你打开的新闻源；检查更新时的 GitHub（只查版本号，可以关）；你打开语音后选的语音服务；
            她上网搜索时的 DuckDuckGo / Google 新闻 / 维基百科（只发搜索关键词，可以关）。
          </li>
          <li>
            天气数据来自 <span class="mono">Open-Meteo.com</span>（CC BY 4.0）；城市坐标来自 GeoNames（CC BY 4.0）。
          </li>
          <li>聊天内容会发送给你选择的模型服务商（比如 DeepSeek），请遵守他们的隐私政策。</li>
        </ul>
      </div>
    </section>
  );
}
