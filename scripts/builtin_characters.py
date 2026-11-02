"""内置完整角色的登记表和命名规则（所有角色同一套做法，不允许各搞各的）。

每个角色三个下载包，都放在 GitHub 发布页，标签不能以 v 加数字开头（会被当成软件版本触发发布）：
  形象包  art-<短名>-v<N>        <短名>-art-v<N>.zip        构建时放进安装包（用户零步骤）
  语音包  voicepack-<短名>-v<N>  <短名>-voicepack-v<N>.zip  构建时放进安装包（用户零步骤）
  声音模型 voice-<短名>-v<N>     <短名>-model.tar.gz         不进安装包，想要「每句都出声」的人自己下载
"""

import json
import os

BASE = "https://github.com/koara666/DeskMate/releases/download"
HERE = os.path.dirname(os.path.abspath(__file__))
KINDS = ("art", "voicepack", "model")


def registry():
    with open(os.path.join(HERE, "builtin_characters.json"), encoding="utf-8") as f:
        return {k: v for k, v in json.load(f).items() if not k.startswith("_")}


def tag(short, kind, version):
    return {
        "art": f"art-{short}-v{version}",
        "voicepack": f"voicepack-{short}-v{version}",
        "model": f"voice-{short}-v{version}",
    }[kind]


def filename(short, kind, version):
    return {
        "art": f"{short}-art-v{version}.zip",
        "voicepack": f"{short}-voicepack-v{version}.zip",
        "model": f"{short}-model.tar.gz",
    }[kind]


def packs(kind):
    """角色 id → （显示名，下载地址，SHA-256）"""
    out = {}
    for cid, c in registry().items():
        v = c[kind]["version"]
        out[cid] = (
            c["label"],
            f"{BASE}/{tag(c['short'], kind, v)}/{filename(c['short'], kind, v)}",
            c[kind]["sha256"],
        )
    return out
