# -*- coding: utf-8 -*-
"""在你电脑上跑千秋的语音服务（GPT-SoVITS 的 api_v2.py），给 DeskMate 的「GPT-SoVITS」声音引擎用。

用法（用装好 GPT-SoVITS 依赖的 Python，比如 D:\\dev\\chiaki-gsv-venv\\Scripts\\python.exe）：
  python scripts/voice-train/serve_local.py            # 用 chiaki-voice/chiaki-model/（服务器上训练出来的包解压后）里轮数最大的模型
  python scripts/voice-train/serve_local.py --base     # 还没训练时：用底模测试整条链路
只监听 127.0.0.1（本机），端口默认 9880；DeskMate「设置 → 声音」里接口地址填 http://127.0.0.1:9880。
"""
import argparse
import glob
import os
import re
import subprocess
import sys

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
PKG = os.path.join(REPO, "chiaki-voice")
GSV = os.path.join(PKG, "GPT-SoVITS")
PM = "GPT_SoVITS/pretrained_models"


def newest(pattern, rx):
    """按文件名里的轮数挑最大的"""
    files = glob.glob(pattern)
    if not files:
        return None
    return max(files, key=lambda f: int(re.search(rx, os.path.basename(f)).group(1)))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--models", default=os.path.join(PKG, "chiaki-model"), help="放 chiaki_e*.pth 和 chiaki-e*.ckpt 的文件夹")
    ap.add_argument("--exp", default="chiaki", help="模型文件名前缀（试跑出来的是 chiaki_quick）")
    ap.add_argument("--base", action="store_true", help="用底模（没训练时测试用）")
    ap.add_argument("--port", type=int, default=9880)
    a = ap.parse_args()

    if a.base:
        sovits, gpt = PM + "/v2Pro/s2Gv2ProPlus.pth", PM + "/s1v3.ckpt"
    else:
        # 必须是绝对路径：api_v2.py 在 GPT-SoVITS 目录里运行，相对路径找不到文件时它不报错，
        # 而是悄悄换回底模、还把配置文件改写成底模（实测），听起来就不是她了
        models = os.path.abspath(a.models)
        sovits = newest(os.path.join(models, a.exp + "_e*.pth"), r"_e(\d+)_s")
        gpt = newest(os.path.join(models, a.exp + "-e*.ckpt"), r"-e(\d+)\.ckpt")
        if not sovits or not gpt:
            sys.exit("在 %s 里没找到训练好的模型（%s_e*.pth 和 %s-e*.ckpt）" % (models, a.exp, a.exp))
    print("SoVITS:", sovits)
    print("GPT:   ", gpt)

    cfg = os.path.join(GSV, "GPT_SoVITS", "configs", "tts_infer_deskmate.yaml")
    with open(cfg, "w", encoding="utf-8", newline="\n") as f:
        f.write(
            "custom:\n"
            "  bert_base_path: %s/chinese-roberta-wwm-ext-large\n"
            "  cnhuhbert_base_path: %s/chinese-hubert-base\n"
            "  device: cuda\n"
            "  is_half: true\n"
            "  t2s_weights_path: %s\n"
            "  version: v2ProPlus\n"
            "  vits_weights_path: %s\n" % (PM, PM, gpt.replace("\\", "/"), sovits.replace("\\", "/"))
        )
    env = os.environ.copy()
    env["PYTHONPATH"] = os.pathsep.join(
        [GSV] + [os.path.join(GSV, p) for p in ("GPT_SoVITS/BigVGAN", "tools", "GPT_SoVITS")]
    )
    env.update({"version": "v2ProPlus", "PYTHONIOENCODING": "utf-8", "KMP_DUPLICATE_LIB_OK": "TRUE",
                "no_proxy": "localhost, 127.0.0.1, ::1"})
    cmd = [sys.executable, "-s", "api_v2.py", "-a", "127.0.0.1", "-p", str(a.port), "-c", cfg]
    print("启动：", " ".join(cmd), flush=True)
    sys.exit(subprocess.call(cmd, cwd=GSV, env=env))


if __name__ == "__main__":
    main()
