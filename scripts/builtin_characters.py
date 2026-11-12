"""内置完整角色的登记表和命名规则（所有角色同一套做法，不允许各搞各的）。

用户在设置里挑哪位角色就下载哪位的「角色包」（src-tauri/src/charpacks.rs）；只有初始角色（登记表里 bundled: true，七海千秋）
的角色包随安装包带着（scripts/chibi/bundle_default_pack.py），全新安装直接装上。
每个角色四个包，都放在 GitHub 发布页，标签不能以 v 加数字开头（会被当成软件版本触发发布）：
  角色包  character-<短名>-v<N> <id>.tar.bz2                用户在软件里选了这位角色才下载（设定、台词、Q 版形象、预录语音都在里面）
  形象包  art-<短名>-v<N>        <短名>-art-v<N>.zip        打角色包的原料；构建时取来给选角色页做预览图
  语音包  voicepack-<短名>-v<N>  <短名>-voicepack-v<N>.zip  打角色包的原料
  声音模型 voice-<短名>-v<N>     <短名>-model.tar.gz         不在角色包里，想要「每句都出声」的人自己下载
"""

import json
import os

BASE = "https://github.com/koara666/DeskMate/releases/download"
HERE = os.path.dirname(os.path.abspath(__file__))
KINDS = ("art", "voicepack", "model")
# 角色包是软件自己下的：登记表里的校验值编进程序，下载下来对不上就不装
ALL_KINDS = ("pack",) + KINDS


def registry():
    with open(os.path.join(HERE, "builtin_characters.json"), encoding="utf-8") as f:
        return {k: v for k, v in json.load(f).items() if not k.startswith("_")}


def tag(short, kind, version):
    return {
        "pack": f"character-{short}-v{version}",
        "art": f"art-{short}-v{version}",
        "voicepack": f"voicepack-{short}-v{version}",
        "model": f"voice-{short}-v{version}",
    }[kind]


def filename(short, kind, version, cid=""):
    return {
        "pack": f"{cid}.tar.bz2",
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
            f"{BASE}/{tag(c['short'], kind, v)}/{filename(c['short'], kind, v, cid)}",
            c[kind]["sha256"],
        )
    return out
