import type { SettingsApi } from '../lib/ipc';
import type { SettingsView } from '../lib/types';

export function AboutPage({ api, view }: { api: SettingsApi; view: SettingsView }) {
  return (
    <section>
      <h1>关于与隐私</h1>
      <div class="card">
        <div class="row">
          <span class="label">版本</span>
          <span class="mono">{view.version}</span>
        </div>
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
          <li>DeskMate 不收集任何使用数据，也不会自动更新；只会连接你在「模型」里配置的服务。</li>
          <li>聊天内容会发送给你选择的模型服务商（比如 DeepSeek），请遵守他们的隐私政策。</li>
        </ul>
      </div>
    </section>
  );
}
