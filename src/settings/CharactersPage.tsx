// 角色设置：选择角色、放自己的角色图片、禁词与自定义要求（全局 + 每个角色）。
// 角色设定（persona）本身是只读的，这里不能修改。
import { useEffect, useState } from 'preact/hooks';
import type { SettingsApi } from '../lib/ipc';
import type { CharacterItem, CharacterPrefs, CharactersView } from '../lib/types';
import { StatusLine, useStatus } from './SettingsApp';

export const STATE_LABELS: Record<string, string> = {
  idle: '待机',
  talking: '说话',
  thinking: '思考',
  happy: '开心',
  sad: '难过/困惑',
  sleep: '睡觉',
};

const toText = (list: string[]) => list.join('\n');
const toList = (text: string) =>
  text
    .split(/\r?\n/)
    .map((s) => s.trim())
    .filter(Boolean);

export function CharactersPage({ api }: { api: SettingsApi }) {
  const [view, setView] = useState<CharactersView | null>(null);
  const [editing, setEditing] = useState('');
  const { status, run } = useStatus();

  const apply = (v: CharactersView) => {
    setView(v);
    setEditing((cur) => (cur && v.list.some((c) => c.id === cur) ? cur : v.current));
  };

  useEffect(() => {
    api.characters().then(apply).catch(() => {});
  }, [api]);

  if (!view) return <div class="muted">加载中…</div>;
  const current = view.list.find((c) => c.id === editing) ?? view.list[0];

  const select = (id: string) =>
    run(() => api.selectCharacter(id), '已切换').then(async (ok) => {
      if (ok) apply(await api.characters());
    });

  return (
    <section>
      <h1>角色</h1>
      <p class="muted">点卡片查看和设置；「切换」后桌面上的角色会马上换成 TA。</p>
      <div class="char-grid">
        {view.list.map((c) => (
          <div
            key={c.id}
            class={`char-card ${c.id === editing ? 'editing' : ''}`}
            style={c.color ? ({ '--accent': c.color } as Record<string, string>) : undefined}
          >
            <button type="button" class="char-main" onClick={() => setEditing(c.id)} aria-pressed={c.id === editing}>
              {c.preview && <img src={c.preview} alt="" />}
              <span class="char-name">{c.name}</span>
            </button>
            {c.id === view.current ? (
              <span class="tag tag-accent">使用中</span>
            ) : (
              <button type="button" class="btn btn-small" onClick={() => select(c.id)}>
                切换
              </button>
            )}
          </div>
        ))}
      </div>
      <StatusLine status={status} />

      {current && <CharacterDetail key={current.id} api={api} view={view} c={current} onUpdated={apply} />}
    </section>
  );
}

interface DetailProps {
  api: SettingsApi;
  view: CharactersView;
  c: CharacterItem;
  onUpdated: (v: CharactersView) => void;
}

function CharacterDetail({ api, view, c, onUpdated }: DetailProps) {
  const [global, setGlobal] = useState<CharacterPrefs>(view.global);
  const [banned, setBanned] = useState(toText(c.prefs.banned_words));
  const [rules, setRules] = useState(toText(c.prefs.custom_rules));
  const [gBanned, setGBanned] = useState(toText(global.banned_words));
  const [gRules, setGRules] = useState(toText(global.custom_rules));
  const { status, run } = useStatus();

  const save = async () => {
    const g = { banned_words: toList(gBanned), custom_rules: toList(gRules) };
    const p = { banned_words: toList(banned), custom_rules: toList(rules) };
    if (await run(() => api.savePrefs(g, c.id, p))) {
      setGlobal(g);
      onUpdated(await api.characters());
    }
  };

  return (
    <>
      <div class="card">
        <h2>{c.name}</h2>
        <p>{c.description}</p>
        {c.source && <p class="muted small">{c.source}</p>}
        {c.origin === 'user' && <p class="muted small">这是你自己放进去的角色包。</p>}
        {c.quotes_fixed + c.quotes_keyed > 0 && (
          <p class="muted small">
            已加载原作台词库：常驻 {c.quotes_fixed} 段，聊到相关话题时再用 {c.quotes_keyed} 段。
          </p>
        )}
      </div>

      <div class="card">
        <h2>换成自己的形象图片</h2>
        <p class="muted">
          默认是一个简单的占位形象。把图片放进这个角色的图片文件夹，点「重新加载」就会用上。文件名按状态命名，比如
          <span class="mono"> idle.png</span>；只放一张 idle 的话，所有状态都用它。支持 PNG / WebP / GIF / JPG，单张不超过 5 MB，建议
          320×360 左右、背景透明的 PNG。
        </p>
        <ul class="state-list">
          {view.states.map((s) => (
            <li key={s}>
              <span class={c.user_images.includes(s) ? 'ok' : 'muted'}>{c.user_images.includes(s) ? '✓' : '○'}</span>{' '}
              <span class="mono">{s}</span> {STATE_LABELS[s] ?? ''}
            </li>
          ))}
        </ul>
        <div class="actions">
          <button type="button" class="btn" onClick={() => run(() => api.openCharacterDir(c.id), '已打开文件夹')}>
            打开图片文件夹
          </button>
          <button
            type="button"
            class="btn"
            onClick={() =>
              run(async () => onUpdated(await api.reloadCharacters()), '已重新加载')
            }
          >
            重新加载
          </button>
        </div>
      </div>

      <div class="card">
        <h2>禁词与自定义要求</h2>
        <p class="muted">每行一条。禁词既会告诉模型不要说，也会在显示前自动替换成 *。自定义要求比如「回复不超过三句话」「叫我小林」。</p>
        <div class="grid">
          <label>
            {c.name} 的禁词
            <textarea rows={4} value={banned} onInput={(e) => setBanned((e.currentTarget as HTMLTextAreaElement).value)} />
          </label>
          <label>
            {c.name} 的自定义要求
            <textarea rows={4} value={rules} onInput={(e) => setRules((e.currentTarget as HTMLTextAreaElement).value)} />
          </label>
          <label>
            全局禁词（所有角色）
            <textarea rows={4} value={gBanned} onInput={(e) => setGBanned((e.currentTarget as HTMLTextAreaElement).value)} />
          </label>
          <label>
            全局自定义要求（所有角色）
            <textarea rows={4} value={gRules} onInput={(e) => setGRules((e.currentTarget as HTMLTextAreaElement).value)} />
          </label>
        </div>
        <div class="actions">
          <button type="button" class="btn btn-primary" onClick={save}>
            保存
          </button>
          <StatusLine status={status} />
        </div>
      </div>
    </>
  );
}
