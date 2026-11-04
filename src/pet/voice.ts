// 桌宠窗口里的语音播放（v2.1 P14）：新的一句会打断上一句；说话时告诉桌宠「在说话」（嘴动）。
// 两句几乎同时到时只播最后一句（取音频是异步的，先到的那句取回来时发现已经过期就丢掉，不会叠在一起）。
// 系统语音用浏览器自带的 Web Speech（Windows 的语音，离线、免费）；别的引擎由后端合成好，这里只播放。
import type { VoiceApi } from '../lib/ipc';

/** 0.3 秒的静音 WAV：用来先把声音设备叫醒。实测第一次出声要多等约 0.5 秒（设备刚打开），先放一段静音，真正的第一句就不晚了 */
function silentWav(): Blob {
  const rate = 24000;
  const n = Math.floor(rate * 0.3);
  const buf = new ArrayBuffer(44 + n * 2);
  const v = new DataView(buf);
  const tag = (o: number, t: string) => [...t].forEach((c, i) => v.setUint8(o + i, c.charCodeAt(0)));
  tag(0, 'RIFF');
  v.setUint32(4, 36 + n * 2, true);
  tag(8, 'WAVEfmt ');
  v.setUint32(16, 16, true);
  v.setUint16(20, 1, true); // PCM
  v.setUint16(22, 1, true); // 单声道
  v.setUint32(24, rate, true);
  v.setUint32(28, rate * 2, true);
  v.setUint16(32, 2, true);
  v.setUint16(34, 16, true);
  tag(36, 'data');
  v.setUint32(40, n * 2, true);
  return new Blob([buf], { type: 'audio/wav' });
}

const errText = (e: unknown) => (e instanceof Error ? `${e.name}: ${e.message}` : String(e)).slice(0, 120);

export function startVoice(api: VoiceApi, onSpeaking: (on: boolean, poses?: string[]) => void): () => void {
  let audio: HTMLAudioElement | null = null;
  let url = '';
  let warmed = false;
  // 每次停止 / 每来一句新的就加一：取回音频时和它对不上，说明这一句已经过期
  let seq = 0;
  const report = (what: string, e: unknown) => {
    api.report?.(`${what}（${errText(e)}）`)?.catch?.(() => {});
  };
  const stopAll = () => {
    seq++;
    if (audio) {
      audio.pause();
      audio = null;
    }
    if (url) {
      URL.revokeObjectURL(url);
      url = '';
    }
    if (typeof speechSynthesis !== 'undefined') speechSynthesis.cancel();
    onSpeaking(false);
  };
  // 播一小段静音把声音设备叫醒（一次就够；不出声）
  const warmUp = () => {
    if (warmed || typeof Audio === 'undefined' || typeof URL.createObjectURL !== 'function') return;
    warmed = true;
    try {
      const u = URL.createObjectURL(silentWav());
      const a = new Audio(u);
      const done = () => URL.revokeObjectURL(u);
      a.onended = done;
      a.onerror = done;
      a.play().catch(done);
    } catch {
      /* 叫不醒就算了，第一句晚一点而已 */
    }
  };
  // 选声音：中文照旧（用户选的，没选就挑中文的）；日语只用日语的声音（没有就返回 undefined，改念中文）
  const pickVoice = (name: string, lang: string): SpeechSynthesisVoice | undefined => {
    const all = speechSynthesis.getVoices();
    const chosen = all.find((v) => v.name === name);
    if (lang !== 'ja') return chosen ?? all.find((v) => v.lang.toLowerCase().startsWith('zh'));
    const ja = (v: SpeechSynthesisVoice) => v.lang.toLowerCase().startsWith('ja');
    return chosen && ja(chosen) ? chosen : all.find(ja);
  };
  const offs = [
    api.onSay((e) => {
      stopAll();
      if (typeof speechSynthesis === 'undefined') return;
      let text = e.text;
      let lang = e.lang || 'zh';
      let v = pickVoice(e.voice, lang);
      if (!v && lang === 'ja' && e.fallback) {
        // 这台电脑没有日语声音：念对应的中文
        text = e.fallback;
        lang = 'zh';
        v = pickVoice(e.voice, lang);
      }
      const u = new SpeechSynthesisUtterance(text);
      if (v) u.voice = v;
      u.lang = v?.lang ?? (lang === 'ja' ? 'ja-JP' : 'zh-CN');
      u.rate = e.rate;
      u.pitch = e.pitch;
      u.volume = e.volume;
      u.onstart = () => onSpeaking(true, e.poses);
      u.onend = () => onSpeaking(false);
      u.onerror = () => onSpeaking(false);
      speechSynthesis.speak(u);
    }),
    api.onPlay(async (e) => {
      stopAll();
      const mine = seq;
      try {
        const buf = await api.audio(e.id);
        if (mine !== seq) return;
        url = URL.createObjectURL(new Blob([buf]));
        const a = new Audio(url);
        audio = a;
        a.volume = Math.max(0, Math.min(1, e.volume));
        a.onplay = () => onSpeaking(true, e.poses);
        a.onended = () => {
          if (audio === a) stopAll();
        };
        a.onerror = () => {
          if (audio === a) {
            report('音频解码 / 播放出错', a.error?.message ?? a.error?.code);
            stopAll();
          }
        };
        await a.play();
      } catch (err) {
        // 取音频失败、浏览器不让播（没有用户操作 / 设备被占用）……以前这里什么都不留，只是没声音
        report('没能播出来', err);
        if (mine === seq) stopAll();
      }
    }),
    api.onStop(stopAll),
    api.onWarm?.(warmUp),
  ].filter((p): p is Promise<() => void> => !!p);
  return () => {
    stopAll();
    offs.forEach((p) => p.then((f) => f()).catch(() => {}));
  };
}
