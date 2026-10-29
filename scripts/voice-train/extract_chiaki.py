# -*- coding: utf-8 -*-
"""从用户自己安装的《超级弹丸论破 2》（Steam 版）里取出七海千秋的日语原声，做 GPT-SoVITS 的训练素材。

只读游戏文件，不修改它。输出只放在用户本机（仓库根目录的 /chiaki-voice/，已写进 .gitignore，绝不进公开仓库）。

用法：
  python scripts/voice-train/extract_chiaki.py "F:/SteamLibrary/steamapps/common/Danganronpa 2 Goodbye Despair" chiaki-voice
需要 ffmpeg 在 PATH 里（把 ogg 转成 wav）。

语音编号是怎么找到的（DECISIONS #56）：
- dr2_data.wad 里 Dr2/data/jp/voice/dr2_voice_hca.awb.NNNNN.ogg 共 8965 条，文件名只有序号。
- 英文脚本（dr2_data_us.wad 里的 *.lin）的「播放语音」指令（0x70 0x08）带 (角色, 章节, 编号)；千秋是角色 9。
- 每段（角色, 章节）在序号里是连续的一段、按编号排列（序号 = 段起点 + 编号 - 1）。用「英文台词长度 ↔ 音频时长」
  的相关性逐段滑动对齐：千秋第 1~6 章各段 r = 0.82~0.91（第二名 ≤ 0.6），起点 4920 / 4966 / 5046 / 5144 / 5227 / 5463，
  首尾相接。紧跟着的 117 条是章节 99（编号 1~80 和 101~137 两组，正好 80 + 37 = 117），r = 0.33（错开一条 ≈ 0.06）。
- 所以千秋 = 序号 4920~5630，共 711 条，约 52 分钟。
"""
import json
import os
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from wad import Wad  # noqa: E402

FIRST, LAST = 4920, 5630
# (章节, 段起点, 段长度)
BLOCKS = [(1, 4920, 46), (2, 4966, 80), (3, 5046, 98), (4, 5144, 83), (5, 5227, 236), (6, 5463, 51)]


def tag(i):
    """序号 → (章节, 编号)"""
    for ch, start, n in BLOCKS:
        if start <= i < start + n:
            return ch, i - start + 1
    k = i - 5514
    return 99, (k + 1 if k < 80 else k - 80 + 101)


def main():
    game, out = sys.argv[1], sys.argv[2]
    wav_dir = os.path.join(out, "dataset", "wavs")
    os.makedirs(wav_dir, exist_ok=True)
    wad = Wad(os.path.join(game, "dr2_data.wad"))
    meta = []
    for i in range(FIRST, LAST + 1):
        ch, vid = tag(i)
        name = "chiaki_%05d_c%02d_%03d" % (i, ch, vid)
        ogg = wad.read("Dr2/data/jp/voice/dr2_voice_hca.awb.%05d.ogg" % i)
        wav = os.path.join(wav_dir, name + ".wav")
        # 单声道 16 位，保持原采样率（GPT-SoVITS 预处理时会自己重采样）
        subprocess.run(
            ["ffmpeg", "-v", "error", "-y", "-i", "pipe:0", "-ac", "1", "-c:a", "pcm_s16le", wav],
            input=ogg, check=True,
        )
        meta.append({"index": i, "chapter": ch, "id": vid, "file": name + ".wav"})
    with open(os.path.join(out, "dataset", "meta.json"), "w", encoding="utf-8") as f:
        json.dump(meta, f, ensure_ascii=False, indent=1)
    print("取出", len(meta), "条 →", wav_dir)


if __name__ == "__main__":
    main()
