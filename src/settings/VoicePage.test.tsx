import { afterEach, describe, expect, it } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/preact';
import { VoicePage } from './VoicePage';
import type { VoiceSettings } from '../lib/types';
import { testView } from './fixtures';

afterEach(cleanup);
const flush = () =>
  act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });

describe('声音设置', () => {
  it('默认关闭；选了 Fish Audio 才能打开，Key 只显示末尾 4 位', async () => {
    const saved: VoiceSettings[] = [];
    const calls: string[] = [];
    const api = {
      voiceSave: async (v: VoiceSettings) => {
        saved.push(v);
        return { ...v, enabled: v.enabled && !!v.engine };
      },
      voiceSetKey: async (w: 'fish' | 'openai', k: string) => {
        calls.push(`key:${w}:${k}`);
        return k.slice(-4);
      },
      voiceClearKey: async () => {},
      voiceTest: async () => {
        calls.push('test');
      },
      voiceClearCache: async () => 0,
    };
    render(<VoicePage api={api} view={testView()} />);
    const toggle = screen.getByText('让她出声').closest('label')!.querySelector('input')!;
    expect(toggle.disabled).toBe(true);
    fireEvent.click(screen.getByText(/Fish Audio（推荐/).closest('label')!.querySelector('input')!);
    await flush();
    expect(saved.at(-1)?.engine).toBe('fish');
    expect(screen.getByText('还没填')).toBeTruthy();
    fireEvent.input(screen.getByLabelText('语音服务 Key'), { target: { value: 'fish-key-1234' } });
    fireEvent.click(screen.getByText('保存 Key'));
    await flush();
    expect(calls).toContain('key:fish:fish-key-1234');
    expect(screen.getByText('已保存（…1234）')).toBeTruthy();
    fireEvent.click(screen.getByText('试听一句'));
    await flush();
    expect(calls).toContain('test');
    fireEvent.click(screen.getByText('让她出声').closest('label')!.querySelector('input')!);
    await flush();
    expect(saved.at(-1)?.enabled).toBe(true);
    // 说话的语言：默认日语，可以换成中文
    const ja = screen.getByText(/^日语（气泡照常显示中文/).closest('label')!.querySelector('input')!;
    expect(ja.checked).toBe(true);
    fireEvent.click(screen.getByText('中文').closest('label')!.querySelector('input')!);
    await flush();
    expect(saved.at(-1)?.lang).toBe('zh');
  });

  it('选了引擎但总开关没开：写明「只有试听会出声」，一键打开；打开后提示消失', async () => {
    // 用户实测：引擎和参考音频都配好了，只有试听有声音——因为「让她出声」没勾，而界面什么都没说
    const saved: VoiceSettings[] = [];
    const base = testView().voice;
    const api = {
      voiceSave: async (v: VoiceSettings) => {
        saved.push(v);
        return v;
      },
      voiceSetKey: async () => '',
      voiceClearKey: async () => {},
      voiceTest: async () => {},
      voiceClearCache: async () => 0,
    };
    const view = { ...testView(), voice: { ...base, engine: 'sovits', enabled: false } };
    render(<VoicePage api={api} view={view} />);
    expect(screen.getByRole('alert').textContent).toContain('只有下面的「试听一句」会出声');
    // 试听成功的提示也说清楚
    fireEvent.click(screen.getByText('试听一句'));
    await flush();
    expect(screen.getByText(/试听成功。注意上面的「让她出声」还没打开/)).toBeTruthy();
    fireEvent.click(screen.getByText('现在打开'));
    await flush();
    expect(saved.at(-1)?.enabled).toBe(true);
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('没选引擎时不弹这个提示（先选引擎）', () => {
    const api = {
      voiceSave: async (v: VoiceSettings) => v,
      voiceSetKey: async () => '',
      voiceClearKey: async () => {},
      voiceTest: async () => {},
      voiceClearCache: async () => 0,
    };
    render(<VoicePage api={api} view={testView()} />);
    expect(screen.queryByRole('alert')).toBeNull();
  });
});
