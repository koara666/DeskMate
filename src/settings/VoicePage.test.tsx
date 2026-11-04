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
    fireEvent.click(screen.getByText(/Fish Audio（云端/).closest('label')!.querySelector('input')!);
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

  it('GPT-SoVITS 的设置按角色分别保存：改红莉栖的不会动千秋的', async () => {
    const saved: VoiceSettings[] = [];
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
    const base = testView();
    const voice = {
      ...base.voice,
      engine: 'sovits',
      sovits_ref_audio: 'D:/chiaki.wav',
      sovits_profiles: { 'makise-kurisu': { url: 'http://127.0.0.1:9881', ref_audio: 'D:/kurisu.wav', ref_text: 'くりす', ref_lang: 'ja' } },
    };
    // 红莉栖：显示她自己的，改参考音频只改她那份
    const { unmount } = render(<VoicePage api={api} view={{ ...base, voice, character_id: 'makise-kurisu' }} />);
    const ref = screen.getByDisplayValue('D:/kurisu.wav') as HTMLInputElement;
    expect((screen.getByDisplayValue('http://127.0.0.1:9881') as HTMLInputElement).value).toBe('http://127.0.0.1:9881');
    fireEvent.change(ref, { target: { value: 'D:/kurisu2.wav' } });
    await flush();
    expect(saved.at(-1)?.sovits_profiles['makise-kurisu'].ref_audio).toBe('D:/kurisu2.wav');
    expect(saved.at(-1)?.sovits_profiles['makise-kurisu'].url).toBe('http://127.0.0.1:9881');
    expect(saved.at(-1)?.sovits_ref_audio).toBe('D:/chiaki.wav');
    unmount();
    // 千秋：还没单独存过，显示通用的那份；一改就给她存一份自己的
    render(<VoicePage api={api} view={{ ...base, voice, character_id: 'nanami-chiaki' }} />);
    fireEvent.change(screen.getByDisplayValue('D:/chiaki.wav'), { target: { value: 'D:/chiaki2.wav' } });
    await flush();
    expect(saved.at(-1)?.sovits_profiles['nanami-chiaki'].ref_audio).toBe('D:/chiaki2.wav');
    expect(saved.at(-1)?.sovits_profiles['nanami-chiaki'].url).toBe('http://127.0.0.1:9880');
    expect(saved.at(-1)?.sovits_profiles['makise-kurisu'].ref_audio).toBe('D:/kurisu.wav');
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

  it('预录语音：装好了就显示句数，不显示语言 / 语速 / 字数和聊天回复；没装好就说清楚', async () => {
    const saved: VoiceSettings[] = [];
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
    const base = testView();
    const view = { ...base, voice: { ...base.voice, engine: 'pack' } };
    const { unmount } = render(<VoicePage api={api} view={view} />);
    expect(screen.getByText(/已装好：243 句/)).toBeTruthy();
    expect(screen.queryByText('她说话的语言')).toBeNull();
    expect(screen.queryByText(/^语速/)).toBeNull();
    expect(screen.queryByText('每次最多读几个字')).toBeNull();
    expect(screen.queryByText(/聊天回复（只读开头一两句）/)).toBeNull();
    expect(screen.getByText('小游戏里的台词')).toBeTruthy();
    unmount();
    render(<VoicePage api={api} view={{ ...view, voice_pack: 0 }} />);
    expect(screen.getByText(/当前角色没有预录语音/)).toBeTruthy();
    // 切到完整的 GPT-SoVITS：语言、语速又出现了
    fireEvent.click(screen.getByText(/GPT-SoVITS（完整/).closest('label')!.querySelector('input')!);
    await flush();
    expect(saved.at(-1)?.engine).toBe('sovits');
  });
});

describe('最近没出声的话', () => {
  const stubs = {
    voiceSave: async (v: VoiceSettings) => v,
    voiceSetKey: async () => '',
    voiceClearKey: async () => {},
    voiceTest: async () => {},
    voiceClearCache: async () => 0,
  };
  const enabledView = () => {
    const v = testView();
    return { ...v, voice: { ...v.voice, enabled: true, engine: 'pack' } };
  };

  it('开着声音时列出没念的话和原因，分得清是没有录音还是出了故障', async () => {
    const api = {
      ...stubs,
      voiceSkips: async () => [
        { at: 1_800_000_100, text: '今天天气不错', reason: '预录语音里没有这句' },
        { at: 1_800_000_000, text: '', reason: '播放失败：NotAllowedError' },
      ],
    };
    render(<VoicePage api={api} view={enabledView()} />);
    await flush();
    const card = screen.getByTestId('voice-skips');
    expect(card.textContent).toContain('「今天天气不错」预录语音里没有这句');
    expect(card.textContent).toContain('播放失败：NotAllowedError');
  });

  it('一条都没有、或者声音没开，就不显示这一块', async () => {
    const none = { ...stubs, voiceSkips: async () => [] };
    render(<VoicePage api={none} view={enabledView()} />);
    await flush();
    expect(screen.queryByTestId('voice-skips')).toBeNull();
    cleanup();
    const some = { ...stubs, voiceSkips: async () => [{ at: 1, text: 'x', reason: 'y' }] };
    render(<VoicePage api={some} view={testView()} />);
    await flush();
    expect(screen.queryByTestId('voice-skips')).toBeNull();
  });
});
