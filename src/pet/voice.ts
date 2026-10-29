// 桌宠窗口里的语音播放（v2.1 P14）：新的一句会打断上一句；说话时告诉桌宠「在说话」（嘴动）。
// 系统语音用浏览器自带的 Web Speech（Windows 的语音，离线、免费）；别的引擎由后端合成好，这里只播放。
import type { VoiceApi } from '../lib/ipc';

export function startVoice(api: VoiceApi, onSpeaking: (on: boolean) => void): () => void {
  let audio: HTMLAudioElement | null = null;
  let url = '';
  const stopAll = () => {
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
      u.onstart = () => onSpeaking(true);
      u.onend = () => onSpeaking(false);
      u.onerror = () => onSpeaking(false);
      speechSynthesis.speak(u);
    }),
    api.onPlay(async (e) => {
      stopAll();
      try {
        const buf = await api.audio(e.id);
        url = URL.createObjectURL(new Blob([buf]));
        const a = new Audio(url);
        audio = a;
        a.volume = Math.max(0, Math.min(1, e.volume));
        a.onplay = () => onSpeaking(true);
        a.onended = () => {
          if (audio === a) stopAll();
        };
        a.onerror = () => {
          if (audio === a) stopAll();
        };
        await a.play();
      } catch {
        stopAll();
      }
    }),
    api.onStop(stopAll),
  ];
  return () => {
    stopAll();
    offs.forEach((p) => p.then((f) => f()).catch(() => {}));
  };
}
