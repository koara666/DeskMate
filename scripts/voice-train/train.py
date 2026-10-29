# -*- coding: utf-8 -*-
"""七海千秋声音训练：GPT-SoVITS v2ProPlus，不用网页界面，一条命令跑完。

放在训练包根目录（和 GPT-SoVITS/、dataset/ 同级），在装好依赖的 Python 环境里运行：
    python train.py            # 完整训练（4090 约 20~40 分钟）
    python train.py --quick    # 冒烟测试：只用 40 条、各训练 1 轮，几分钟，确认环境没问题
做的事和 GPT-SoVITS 网页界面「1-GPT-SoVITS-TTS」那一页完全一样（照 webui.py 的 open1abc / open1Ba / open1Bb 写的）：
  1A 文字转音素 + BERT 特征 → 1B 音频特征（HuBERT、32k 音频、声纹）→ 1C 语义编码 → SoVITS 训练 → GPT 训练
最后把训练出来的模型和参考音频放进 output/，打成一个压缩包，拿回来就行。
中途断了重新运行会跳过已经做完的预处理步骤。
"""
import argparse
import glob
import json
import os
import shutil
import subprocess
import sys
import tarfile

ROOT = os.path.dirname(os.path.abspath(__file__))
GSV = os.path.join(ROOT, "GPT-SoVITS")
VERSION = "v2ProPlus"
PM = "GPT_SoVITS/pretrained_models"
PRETRAINED = {
    "bert": PM + "/chinese-roberta-wwm-ext-large",
    "hubert": PM + "/chinese-hubert-base",
    "s2G": PM + "/v2Pro/s2Gv2ProPlus.pth",
    "s2D": PM + "/v2Pro/s2Dv2ProPlus.pth",
    "s1": PM + "/s1v3.ckpt",
    "sv": PM + "/sv/pretrained_eres2netv2w24s4ep4.ckpt",
}


def log(msg):
    print("\n==== " + msg, flush=True)


def run(script, env, *args):
    cmd = [sys.executable, "-s", script, *args]
    print("  $", " ".join(cmd), flush=True)
    r = subprocess.run(cmd, cwd=GSV, env=env)
    if r.returncode != 0:
        sys.exit("出错了：%s 返回 %d。把上面的报错截图发回来。" % (script, r.returncode))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--exp", default="chiaki", help="实验名（输出文件名前缀）")
    ap.add_argument("--sovits-epochs", type=int, default=12)
    ap.add_argument("--sovits-save-every", type=int, default=4)
    ap.add_argument("--gpt-epochs", type=int, default=15)
    ap.add_argument("--gpt-save-every", type=int, default=5)
    ap.add_argument("--batch", type=int, default=0, help="每批多少条，0 = 按显存自动（显存 GB / 2）")
    ap.add_argument("--gpu", default="0", help="用哪张显卡")
    ap.add_argument("--quick", action="store_true", help="冒烟测试：40 条、各 1 轮")
    a = ap.parse_args()

    import torch  # 放在这里：先让 --help 不依赖 torch

    for k, p in PRETRAINED.items():
        if not os.path.exists(os.path.join(GSV, p)):
            sys.exit("缺少预训练模型 %s（%s）。先运行 setup.sh。" % (k, p))
    if not torch.cuda.is_available():
        sys.exit("没有检测到 NVIDIA 显卡（torch.cuda.is_available() = False）。")
    gpu_idx = int(a.gpu)
    props = torch.cuda.get_device_properties(gpu_idx)
    mem_gb = int(props.total_memory / 1024**3 + 0.4)
    batch = a.batch or max(1, mem_gb // 2)
    print("显卡：%s，%d GB，batch = %d" % (props.name, mem_gb, batch))

    exp = a.exp + ("_quick" if a.quick else "")
    wav_dir = os.path.join(ROOT, "dataset", "wavs")
    list_path = os.path.join(ROOT, "dataset", "chiaki.list")
    if a.quick:
        lines = open(list_path, encoding="utf-8").read().strip().split("\n")[:40]
        list_path = os.path.join(ROOT, "dataset", "chiaki_quick.list")
        open(list_path, "w", encoding="utf-8").write("\n".join(lines) + "\n")
        a.sovits_epochs = a.gpt_epochs = 1
        a.sovits_save_every = a.gpt_save_every = 1
    opt_dir = os.path.join(GSV, "logs", exp)
    tmp = os.path.join(GSV, "TEMP")
    os.makedirs(opt_dir, exist_ok=True)
    os.makedirs(tmp, exist_ok=True)
    for d in ("SoVITS_weights_" + VERSION, "GPT_weights_" + VERSION):
        os.makedirs(os.path.join(GSV, d), exist_ok=True)

    env = os.environ.copy()
    # webui.py 启动时会往 site-packages 写 users.pth 把这些目录加进 import 路径；这里用 PYTHONPATH 做同样的事
    env["PYTHONPATH"] = os.pathsep.join(
        [GSV] + [os.path.join(GSV, p) for p in ("GPT_SoVITS/BigVGAN", "tools", "tools/asr", "GPT_SoVITS", "tools/uvr5")]
    )
    env.update(
        {
            "version": VERSION,
            "is_half": "True",
            "TEMP": tmp,
            "KMP_DUPLICATE_LIB_OK": "TRUE",
            "no_proxy": "localhost, 127.0.0.1, ::1",
            "PYTHONIOENCODING": "utf-8",
            "_CUDA_VISIBLE_DEVICES": str(gpu_idx),
            "i_part": "0",
            "all_parts": "1",
            "inp_text": list_path,
            "inp_wav_dir": wav_dir,
            "exp_name": exp,
            "opt_dir": opt_dir,
            "bert_pretrained_dir": PRETRAINED["bert"],
            "cnhubert_base_dir": PRETRAINED["hubert"],
            "sv_path": PRETRAINED["sv"],
            "pretrained_s2G": PRETRAINED["s2G"],
            "s2config_path": "GPT_SoVITS/configs/s2%s.json" % VERSION,
        }
    )

    # ---------- 1A 文字 ----------
    text_path = os.path.join(opt_dir, "2-name2text.txt")
    if not (os.path.exists(text_path) and os.path.getsize(text_path) > 10):
        log("1A 文字转音素 + BERT 特征")
        run("GPT_SoVITS/prepare_datasets/1-get-text.py", env)
        part = os.path.join(opt_dir, "2-name2text-0.txt")
        body = open(part, encoding="utf8").read().strip("\n")
        if not body:
            sys.exit("1A 没有产出任何文字，检查 dataset/chiaki.list。")
        open(text_path, "w", encoding="utf8").write(body + "\n")
        os.remove(part)
    # ---------- 1B 音频特征 + 声纹 ----------
    if not os.path.exists(os.path.join(opt_dir, "7-sv_cn")) or not os.listdir(os.path.join(opt_dir, "7-sv_cn")):
        log("1B 音频特征（HuBERT、32k）")
        run("GPT_SoVITS/prepare_datasets/2-get-hubert-wav32k.py", env)
        log("1B 声纹（v2Pro 系列需要）")
        run("GPT_SoVITS/prepare_datasets/2-get-sv.py", env)
    # ---------- 1C 语义 ----------
    sem_path = os.path.join(opt_dir, "6-name2semantic.tsv")
    if not (os.path.exists(sem_path) and os.path.getsize(sem_path) >= 31):
        log("1C 语义编码")
        run("GPT_SoVITS/prepare_datasets/3-get-semantic.py", env)
        part = os.path.join(opt_dir, "6-name2semantic-0.tsv")
        body = open(part, encoding="utf8").read().strip("\n")
        open(sem_path, "w", encoding="utf8").write("item_name\tsemantic_audio\n" + body + "\n")
        os.remove(part)

    # ---------- SoVITS ----------
    log("SoVITS 训练（%d 轮）" % a.sovits_epochs)
    data = json.load(open(os.path.join(GSV, env["s2config_path"]), encoding="utf8"))
    data["train"].update(
        {
            "batch_size": batch,
            "epochs": a.sovits_epochs,
            "text_low_lr_rate": 0.4,
            "pretrained_s2G": PRETRAINED["s2G"],
            "pretrained_s2D": PRETRAINED["s2D"],
            "if_save_latest": True,
            "if_save_every_weights": True,
            "save_every_epoch": a.sovits_save_every,
            "gpu_numbers": str(gpu_idx),
            "grad_ckpt": False,
            "lora_rank": "32",
        }
    )
    data["model"]["version"] = VERSION
    data["data"]["exp_dir"] = data["s2_ckpt_dir"] = opt_dir
    data["save_weight_dir"] = "SoVITS_weights_" + VERSION
    data["name"] = exp
    data["version"] = VERSION
    os.makedirs(os.path.join(opt_dir, "logs_s2_" + VERSION), exist_ok=True)
    s2_cfg = os.path.join(tmp, "tmp_s2.json")
    json.dump(data, open(s2_cfg, "w", encoding="utf8"))
    run("GPT_SoVITS/s2_train.py", env, "--config", s2_cfg)

    # ---------- GPT ----------
    log("GPT 训练（%d 轮）" % a.gpt_epochs)
    import yaml

    data = yaml.load(open(os.path.join(GSV, "GPT_SoVITS/configs/s1longer-v2.yaml"), encoding="utf8"), Loader=yaml.FullLoader)
    data["train"].update(
        {
            "batch_size": batch,
            "epochs": a.gpt_epochs,
            "save_every_n_epoch": a.gpt_save_every,
            "if_save_every_weights": True,
            "if_save_latest": True,
            "if_dpo": False,
            "half_weights_save_dir": "GPT_weights_" + VERSION,
            "exp_name": exp,
        }
    )
    data["pretrained_s1"] = PRETRAINED["s1"]
    data["train_semantic_path"] = sem_path
    data["train_phoneme_path"] = text_path
    data["output_dir"] = os.path.join(opt_dir, "logs_s1_" + VERSION)
    if os.name == "nt":
        data["data"]["num_workers"] = 0  # Windows 上多进程读数据容易卡住
    env["hz"] = "25hz"
    s1_cfg = os.path.join(tmp, "tmp_s1.yaml")
    yaml.dump(data, open(s1_cfg, "w", encoding="utf8"), default_flow_style=False)
    run("GPT_SoVITS/s1_train.py", env, "--config_file", s1_cfg)

    # ---------- 打包 ----------
    log("打包结果")
    out = os.path.join(ROOT, "output", exp)
    os.makedirs(out, exist_ok=True)
    got = []
    for pat in ("SoVITS_weights_%s/%s_e*.pth" % (VERSION, exp), "GPT_weights_%s/%s-e*.ckpt" % (VERSION, exp)):
        for f in glob.glob(os.path.join(GSV, pat)):
            shutil.copy2(f, out)
            got.append(os.path.basename(f))
    ref = os.path.join(ROOT, "ref")
    if os.path.isdir(ref):
        shutil.copytree(ref, os.path.join(out, "ref"), dirs_exist_ok=True)
    if not got:
        sys.exit("没有找到训练出来的模型文件，往上翻看看有没有报错。")
    tar_path = os.path.join(ROOT, "output", exp + "-model.tar.gz")
    with tarfile.open(tar_path, "w:gz") as t:
        t.add(out, arcname=exp + "-model")
    print("\n完成！模型：", ", ".join(sorted(got)))
    print("把这个文件拿回来：", tar_path, "（%.0f MB）" % (os.path.getsize(tar_path) / 1e6))


if __name__ == "__main__":
    main()
