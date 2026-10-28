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
  const pickVoice = (name: string): SpeechSynthesisVoice | undefined => {
    const all = speechSynthesis.getVoices();
    return all.find((v) => v.name === name) ?? all.find((v) => v.lang.toLowerCase().startsWith('zh'));
  };
  const offs = [
    api.onSay((e) => {
      stopAll();
      if (typeof speechSynthesis === 'undefined') return;
      const u = new SpeechSynthesisUtterance(e.text);
      const v = pickVoice(e.voice);
      if (v) u.voice = v;
      u.lang = v?.lang ?? 'zh-CN';
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
