# -*- coding: utf-8 -*-
"""把训练包整理好并打成 chiaki-voice.tar.gz（上传到服务器用）。

用法：python scripts/voice-train/pack.py chiaki-voice
- 把本目录的 setup.sh / train.sh / train.py / README.md 复制进训练包；
- 打包时不带预训练模型、TEMP、logs、output（服务器上 setup.sh 会自己下载，体积小很多）；
- .sh 统一成 LF 换行，并设成可执行。
"""
import os
import shutil
import sys
import tarfile

HERE = os.path.dirname(os.path.abspath(__file__))
SKIP_DIRS = {"pretrained_models", "TEMP", "logs", "output", "__pycache__", ".git", "env", ".cache", "miniforge3"}
SKIP_PREFIX = ("SoVITS_weights", "GPT_weights")


def main():
    root = os.path.abspath(sys.argv[1])
    for f in ("env.sh", "setup.sh", "train.sh", "train.py", "README.md"):
        src = os.path.join(HERE, f)
        data = open(src, "rb").read()
        if f.endswith(".sh"):
            data = data.replace(b"\r\n", b"\n")
        open(os.path.join(root, f), "wb").write(data)
    for need in ("dataset/chiaki.list", "dataset/wavs", "GPT-SoVITS/webui.py"):
        if not os.path.exists(os.path.join(root, need)):
            sys.exit("训练包里缺少 " + need)

    name = os.path.basename(root)
    out = os.path.join(os.path.dirname(root), name + ".tar.gz")

    def filt(ti):
        base = os.path.basename(ti.name)
        if ti.isdir() and (base in SKIP_DIRS or base.startswith(SKIP_PREFIX)):
            return None
        if base.endswith((".tar.gz", ".pyc")) or base == "chiaki_quick.list":
            return None
        if base.endswith(".sh"):
            ti.mode = 0o755
        return ti

    with tarfile.open(out, "w:gz") as t:
        t.add(root, arcname=name, filter=filt)
    print("打包好了：", out, "（%.0f MB）" % (os.path.getsize(out) / 1e6))


if __name__ == "__main__":
    main()
