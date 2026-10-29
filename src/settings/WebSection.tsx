// 设置「模型」页里的联网搜索开关（DECISIONS #59）。
import { useState } from 'preact/hooks';
import type { SettingsApi } from '../lib/ipc';

type Api = Pick<SettingsApi, 'webSave'>;

export function WebSection({ api, initial }: { api: Api; initial: boolean }) {
  const [on, setOn] = useState(initial);
  return (
    <div class="card">
      <label class="row">
        <input
          type="checkbox"
          checked={on}
          onChange={(e) => {
            const next = (e.currentTarget as HTMLInputElement).checked;
            setOn(next);
            api.webSave(next).catch(() => setOn(!next));
          }}
        />
        <span>
          允许她上网搜索
          <small>
            问新闻、最新消息、比分、股价，或对她说「搜一下……」时，她会先上网查再回答，气泡下面会写明查了什么。
            只会把搜索关键词发给搜索服务（新闻用 Google 新闻，其他用 DuckDuckGo，查不到再用维基百科），不发聊天记录和记忆；
            含邮箱、手机号、密码这类内容的话不会拿去搜
          </small>
        </span>
      </label>
    </div>
  );
}
