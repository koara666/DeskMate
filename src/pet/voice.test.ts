import { afterEach, describe, expect, it, vi } from 'vitest';
import { startVoice } from './voice';
import type { VoiceApi } from '../lib/ipc';

type Say = Parameters<Parameters<VoiceApi['onSay']>[0]>[0];

// 假的 Web Speech：记下念了什么、用的哪个声音
function fakeSpeech(voices: { name: string; lang: string }[]) {
  const spoken: { text: string; voice?: string; lang: string }[] = [];
  class Utterance {
    voice: { name: string; lang: string } | null = null;
    lang = '';
    rate = 1;
    pitch = 1;
    volume = 1;
    onstart: (() => void) | null = null;
    onend: (() => void) | null = null;
    onerror: (() => void) | null = null;
    constructor(public text: string) {}
  }
  vi.stubGlobal('SpeechSynthesisUtterance', Utterance);
  vi.stubGlobal('speechSynthesis', {
    getVoices: () => voices,
    cancel: () => {},
    speak: (u: Utterance) => spoken.push({ text: u.text, voice: u.voice?.name, lang: u.lang }),
  });
  return spoken;
}

function fakeApi() {
  let say: ((e: Say) => void) | null = null;
  const api = {
    onSay: (cb: (e: Say) => void) => {
      say = cb;
      return Promise.resolve(() => {});
    },
    onPlay: () => Promise.resolve(() => {}),
    onStop: () => Promise.resolve(() => {}),
    audio: async () => new ArrayBuffer(0),
  } as unknown as VoiceApi;
  return { api, say: (e: Partial<Say>) => say!({ text: '', lang: 'zh', fallback: '', voice: '', rate: 1, pitch: 1, volume: 1, ...e }) };
}

afterEach(() => vi.unstubAllGlobals());

describe('系统语音按语言挑声音', () => {
  it('有日语声音就念日语', () => {
    const spoken = fakeSpeech([
      { name: 'Huihui', lang: 'zh-CN' },
      { name: 'Haruka', lang: 'ja-JP' },
    ]);
    const { api, say } = fakeApi();
    startVoice(api, () => {});
    say({ text: 'おはよう。', lang: 'ja', fallback: '早上好。' });
    expect(spoken.at(-1)).toEqual({ text: 'おはよう。', voice: 'Haruka', lang: 'ja-JP' });
    // 用户选的是中文声音：说日语时不用它
    say({ text: 'おはよう。', lang: 'ja', fallback: '早上好。', voice: 'Huihui' });
    expect(spoken.at(-1)?.voice).toBe('Haruka');
  });

  it('没有日语声音时改念对应的中文', () => {
    const spoken = fakeSpeech([{ name: 'Huihui', lang: 'zh-CN' }]);
    const { api, say } = fakeApi();
    startVoice(api, () => {});
    say({ text: 'おはよう。', lang: 'ja', fallback: '早上好。' });
    expect(spoken.at(-1)).toEqual({ text: '早上好。', voice: 'Huihui', lang: 'zh-CN' });
    say({ text: '你好', lang: 'zh' });
    expect(spoken.at(-1)).toEqual({ text: '你好', voice: 'Huihui', lang: 'zh-CN' });
  });
});
