# -*- coding: utf-8 -*-
"""给取出来的千秋原声做日语文字识别（faster-whisper large-v3），生成 GPT-SoVITS 的训练清单。

用法（在装了 faster-whisper 的环境里，有 NVIDIA 显卡更快）：
  python scripts/voice-train/transcribe.py chiaki-voice
输出：
  chiaki-voice/dataset/asr.json     每条的识别结果和置信度
  chiaki-voice/dataset/chiaki.list  训练清单：文件名|chiaki|ja|日语文字（过滤掉不可靠的）
模型从 HuggingFace 下载；国内网络先设 HF_ENDPOINT=https://hf-mirror.com。
"""
import json
import os
import re
import sys
import wave

# Whisper 在没声音或很短的片段上常见的「幻听」
HALLUCINATIONS = ("ご視聴", "チャンネル登録", "字幕", "おやすみなさい。おやすみなさい", "ありがとうございました。ありがとう")
# 人名纠错：Whisper 听错的、日语词典（pyopenjtalk）读错的，改成假名（读音不对会学错发音）
# 七海 → 词典读「ななうみ」；日向 → 词典读「ひゅうが」，游戏里是日向创「ひなた」
NAME_FIXES = {"七道明": "ななみちあき", "七海": "ななみ", "日向": "ひなた"}


def duration(path):
    with wave.open(path) as w:
        return w.getnframes() / w.getframerate()


def add_torch_dlls():
    """Windows：ctranslate2 要的 cuBLAS / cuDNN 直接用 PyTorch 自带的那份"""
    if os.name != "nt":
        return
    try:
        import torch

        lib = os.path.join(os.path.dirname(torch.__file__), "lib")
        os.add_dll_directory(lib)
        os.environ["PATH"] = lib + os.pathsep + os.environ["PATH"]
    except ImportError:
        pass


def main():
    root = sys.argv[1]
    ds = os.path.join(root, "dataset")
    wav_dir = os.path.join(ds, "wavs")
    out_json = os.path.join(ds, "asr.json")
    res = json.load(open(out_json, encoding="utf-8")) if os.path.exists(out_json) else {}
    names = sorted(n for n in os.listdir(wav_dir) if n.endswith(".wav"))
    model = None
    for k, n in enumerate(names):
        if n in res:
            continue
        if model is None:  # 全部识别过就不用加载模型，直接重新过滤
            add_torch_dlls()
            from faster_whisper import WhisperModel

            import ctranslate2

            device = "cuda" if ctranslate2.get_cuda_device_count() > 0 else "cpu"
            # WHISPER_MODEL 可以指向已经下载好的模型文件夹
            model = WhisperModel(
                os.environ.get("WHISPER_MODEL", "large-v3"), device=device,
                compute_type="float16" if device == "cuda" else "int8",
            )
        segs, _info = model.transcribe(
            os.path.join(wav_dir, n), language="ja", beam_size=5, temperature=0.0,
            condition_on_previous_text=False, vad_filter=False,
        )
        segs = list(segs)
        res[n] = {
            "text": "".join(s.text for s in segs).strip(),
            "logprob": min((s.avg_logprob for s in segs), default=-9),
            "no_speech": max((s.no_speech_prob for s in segs), default=1),
            "dur": round(duration(os.path.join(wav_dir, n)), 2),
        }
        if k % 25 == 0:
            print(k, n, res[n]["text"], flush=True)
            json.dump(res, open(out_json, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    json.dump(res, open(out_json, "w", encoding="utf-8"), ensure_ascii=False, indent=1)

    keep, drop = [], []
    for n in names:
        r = res[n]
        t = r["text"].replace("|", " ").replace("\n", " ").strip()
        for wrong, right in NAME_FIXES.items():
            t = t.replace(wrong, right)
        cps = len(t) / max(r["dur"], 0.1)
        why = None
        if not t:
            why = "没识别出文字"
        elif r["dur"] < 0.8 or r["dur"] > 15:
            why = "太短或太长"
        elif r["no_speech"] > 0.6 or r["logprob"] < -1.0:
            why = "置信度低"
        elif any(h in t for h in HALLUCINATIONS):
            why = "疑似幻听"
        elif re.search(r"(.)\1{7,}", t):
            why = "同一个字拖太长（呻吟）"
        elif cps < 1.0 or cps > 18:
            why = "语速对不上"
        (drop if why else keep).append((n, t, why))
    with open(os.path.join(ds, "chiaki.list"), "w", encoding="utf-8") as f:
        for n, t, _ in keep:
            f.write("%s|chiaki|ja|%s\n" % (n, t))
    print("保留 %d 条（%.1f 分钟），去掉 %d 条" % (len(keep), sum(res[n]["dur"] for n, _, _ in keep) / 60, len(drop)))
    for n, t, why in drop:
        print("  去掉", n, why, t)


if __name__ == "__main__":
    main()
