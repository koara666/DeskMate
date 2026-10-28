// 「高级」：委托外部 agent（默认关闭）、内置能力层（允许读取的文件夹、常用程序）、显卡加速。
// 任务执行设置只有开了委托才按需加载（DECISIONS #33）。
import { useEffect, useState } from 'preact/hooks';
import type { ComponentType } from 'preact';
import type { SettingsApi } from '../lib/ipc';
import type { AdvancedSettings, General, SettingsView } from '../lib/types';
import { StatusLine, useStatus } from './SettingsApp';

interface Props {
  api: SettingsApi;
  view: SettingsView;
  onChanged: () => void;
}

export function AdvancedPage({ api, view, onChanged }: Props) {
  const [a, setA] = useState<AdvancedSettings>(view.advanced);
  const [g, setG] = useState<General>(view.general);
  const [newDir, setNewDir] = useState('');
  const [prog, setProg] = useState({ name: '', path: '' });
  const [Tasks, setTasks] = useState<ComponentType | null>(null);
  const { status, run } = useStatus();

  useEffect(() => {
    if (!a.allow_delegation || Tasks) return;
    import('./TasksPage')
      .then((m) => setTasks(() => m.TasksPage))
      .catch(() => {});
  }, [a.allow_delegation, Tasks]);

  const save = async (next: AdvancedSettings) => {
    let saved: AdvancedSettings | null = null;
    const ok = await run(async () => {
      saved = await api.saveAdvanced(next);
    });
    if (ok && saved) {
      setA(saved);
      return true;
    }
    return false;
  };
  const saveGeneral = async (next: General) => {
    setG(next);
    if (await run(() => api.saveGeneral(next))) onChanged();
  };

  return (
    <section>
      <h1>高级</h1>

      <div class="card">
        <h2>让她帮你打开、读取东西</h2>
        <p class="muted small">
          聊天时说「帮我打开 B 站」「打开作业文件夹」，她会在气泡里给出一个按钮，你点了才会打开。程序、脚本这类文件不会直接打开，常用程序要先在下面登记。
          想让她帮你看看某个文本文件写了什么，要先在这里允许她读取那个文件夹（只读，不会修改），文件内容会发给你用的聊天模型。
        </p>
        <h3>允许读取的文件夹</h3>
        {a.read_dirs.length === 0 && <div class="muted small">还没有。</div>}
        <ul class="plain">
          {a.read_dirs.map((d) => (
            <li key={d} class="row">
              <span class="mono grow">{d}</span>
              <button
                type="button"
                class="btn btn-small"
                onClick={() => save({ ...a, read_dirs: a.read_dirs.filter((x) => x !== d) })}
              >
                移除
              </button>
            </li>
          ))}
        </ul>
        <div class="row">
          <input
            class="grow"
            placeholder="文件夹的完整路径，比如 D:\笔记"
            aria-label="添加允许读取的文件夹"
            value={newDir}
            onInput={(e) => setNewDir((e.currentTarget as HTMLInputElement).value)}
          />
          <button
            type="button"
            class="btn btn-small"
            disabled={!newDir.trim()}
            onClick={async () => {
              if (await save({ ...a, read_dirs: [...a.read_dirs, newDir.trim()] })) setNewDir('');
            }}
          >
            添加
          </button>
        </div>

        <h3>常用程序（可以按名字打开）</h3>
        {a.programs.length === 0 && <div class="muted small">还没有。</div>}
        <ul class="plain">
          {a.programs.map((p) => (
            <li key={p.name} class="row">
              <span class="grow">
                {p.name} <small class="mono">{p.path}</small>
              </span>
              <button
                type="button"
                class="btn btn-small"
                onClick={() => save({ ...a, programs: a.programs.filter((x) => x.name !== p.name) })}
              >
                移除
              </button>
            </li>
          ))}
        </ul>
        <div class="row">
          <input
            placeholder="名字，比如 网易云"
            aria-label="程序名字"
            value={prog.name}
            onInput={(e) => setProg({ ...prog, name: (e.currentTarget as HTMLInputElement).value })}
          />
          <input
            class="grow"
            placeholder="程序的完整路径（.exe）"
            aria-label="程序路径"
            value={prog.path}
            onInput={(e) => setProg({ ...prog, path: (e.currentTarget as HTMLInputElement).value })}
          />
          <button
            type="button"
            class="btn btn-small"
            disabled={!prog.name.trim() || !prog.path.trim()}
            onClick={async () => {
              if (await save({ ...a, programs: [...a.programs, { name: prog.name.trim(), path: prog.path.trim() }] }))
                setProg({ name: '', path: '' });
            }}
          >
            登记
          </button>
        </div>
      </div>

      <div class="card">
        <h2>委托外部 agent（Claude Code / Codex）</h2>
        <label class="row">
          <input
            type="checkbox"
            checked={a.allow_delegation}
            onChange={(e) => save({ ...a, allow_delegation: (e.currentTarget as HTMLInputElement).checked })}
          />
          <span>
            允许她把事情交给 Claude Code / Codex 去做
            <small>
              默认关闭。DeskMate 现在是陪伴型的桌宠，真正要干活时直接打开 Claude Code 或 Codex 更方便。开启后，你在聊天里明确要求时，她会弹出任务卡片，你确认后才执行。
            </small>
          </span>
        </label>
      </div>
      {a.allow_delegation && (Tasks ? <Tasks /> : <div class="muted">加载中…</div>)}

      <div class="card">
        <h2>显示</h2>
        <label class="row">
          <input
            type="checkbox"
            checked={g.gpu_acceleration}
            onChange={(e) => saveGeneral({ ...g, gpu_acceleration: (e.currentTarget as HTMLInputElement).checked })}
          />
          <span>
            显卡加速
            <small>
              默认关闭，可以少占约 80 MB 内存。只有角色或窗口显示不正常时才需要打开。
              {g.gpu_acceleration !== view.general.gpu_acceleration && <b>（退出 DeskMate 再打开后生效）</b>}
            </small>
          </span>
        </label>
      </div>
      <StatusLine status={status} />
    </section>
  );
}
