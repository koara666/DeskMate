# -*- coding: utf-8 -*-
"""七海千秋声音训练：GPT-SoVITS v2ProPlus，不用网页界面，一条命令跑完。

放在训练包根目录（和 GPT-SoVITS/、dataset/ 同级），在装好依赖的 Python 环境里运行：
    python train.py            # 完整训练（4090 约 20~40 分钟）
    python train.py --quick    # 冒烟测试：只用 40 条、各训练 1 轮，几分钟，确认环境没问题
做的事和 GPT-SoVITS 网页界面「1-GPT-SoVITS-TTS」那一页完全一样（照 webui.py 的 open1abc / open1Ba / open1Bb 写的）：
  1A 文字转音素 + BERT 特征 → 1B 音频特征（HuBERT、32k 音频、声纹）→ 1C 语义编码 → SoVITS 训练 → GPT 训练
最后把最后一轮的模型和参考音频放进 output/，打成一个压缩包，拿回来就行（其他几轮留在服务器上）。
中途断了重新运行会跳过已经做完的预处理步骤。
"""
import argparse
import glob
import json
import os
import re
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


MIN_KEEP = 0.9  # 每一步至少要有上一步 90% 的产出（个别音量异常的条目会被 GPT-SoVITS 自己过滤掉）


def log(msg):
    print("\n==== " + msg, flush=True)


def count_lines(path):
    if not os.path.exists(path):
        return 0
    return sum(1 for line in open(path, encoding="utf8") if line.strip())


def count_files(opt_dir, sub):
    d = os.path.join(opt_dir, sub)
    return len(os.listdir(d)) if os.path.isdir(d) else 0


def check(step, got, want, hint=""):
    print("  %s：%d / %d" % (step, got, want), flush=True)
    if got < want * MIN_KEEP:
        sys.exit("出错了：%s 只产出了 %d 条（应该接近 %d 条）。往上翻看看报错。%s" % (step, got, want, hint))


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
    ap.add_argument("--gpu", default="", help="用哪张显卡（编号和 nvidia-smi 一样）；不填就用空闲显存最多的那张")
    ap.add_argument("--quick", action="store_true", help="冒烟测试：40 条、各 1 轮")
    a = ap.parse_args()

    import torch  # 放在这里：先让 --help 不依赖 torch

    for k, p in PRETRAINED.items():
        if not os.path.exists(os.path.join(GSV, p)):
            sys.exit("缺少预训练模型 %s（%s）。先运行 setup.sh。" % (k, p))
    if not torch.cuda.is_available():
        sys.exit("没有检测到 NVIDIA 显卡（torch.cuda.is_available() = False）。")
    # 用 nvidia-smi 看空闲显存：torch.cuda.mem_get_info 会在每张卡上建一个 CUDA 上下文，
    # 这个进程一直开到训练结束，等于在服务器每张卡上都白占几百 MB（别人的卡也算）
    q = subprocess.run(
        ["nvidia-smi", "--query-gpu=index,name,memory.free,memory.total", "--format=csv,noheader,nounits"],
        capture_output=True, text=True,
    )
    gpus = {}
    for row in q.stdout.strip().splitlines():
        idx, name, free_mb, total_mb = [x.strip() for x in row.split(",")]
        gpus[int(idx)] = int(free_mb) / 1024
        print("  显卡 %s：%s，空闲 %.1f / %.1f GB" % (idx, name, int(free_mb) / 1024, int(total_mb) / 1024))
    if not gpus:
        sys.exit("nvidia-smi 没有列出显卡：" + (q.stderr or "").strip())
    gpu_idx = int(a.gpu) if a.gpu else max(gpus, key=gpus.get)
    if gpu_idx not in gpus:
        sys.exit("没有 %d 号显卡，可选：%s" % (gpu_idx, ", ".join(map(str, gpus))))
    free_gb = int(gpus[gpu_idx])
    # 和网页界面一样按「显存 GB / 2」定 batch，但用的是现在空闲的显存（服务器上可能有别的任务）
    batch = a.batch or min(12, max(1, free_gb // 2))
    print("用显卡 %d，空闲 %d GB，batch = %d" % (gpu_idx, free_gb, batch))
    if free_gb < 6 and not a.batch:
        print("  提醒：这张卡空闲显存不多，可能有别人在用。换一张空闲的：--gpu 编号")

    exp = a.exp + ("_quick" if a.quick else "")
    wav_dir = os.path.join(ROOT, "dataset", "wavs")
    list_path = os.path.join(ROOT, "dataset", "chiaki.list")
    if a.quick:
        lines = open(list_path, encoding="utf-8").read().strip().split("\n")[:40]
        list_path = os.path.join(ROOT, "dataset", "chiaki_quick.list")
        open(list_path, "w", encoding="utf-8").write("\n".join(lines) + "\n")
        a.sovits_epochs = a.gpt_epochs = 1
        a.sovits_save_every = a.gpt_save_every = 1
    n_list = count_lines(list_path)
    print("训练清单：%d 条" % n_list)
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
    n_text = count_lines(text_path)
    check("1A 文字", n_text, n_list)
    # ---------- 1B 音频特征 + 声纹 ----------
    # GPT-SoVITS 的预处理脚本遇到单条出错只打印、不退出，所以每一步都数一下产出（太少就停下，别白训练）
    if count_files(opt_dir, "4-cnhubert") < n_text * MIN_KEEP or count_files(opt_dir, "5-wav32k") < n_text * MIN_KEEP:
        log("1B 音频特征（HuBERT、32k）")
        run("GPT_SoVITS/prepare_datasets/2-get-hubert-wav32k.py", env)
    n_hubert = count_files(opt_dir, "4-cnhubert")
    check("1B 音频特征", n_hubert, n_text)
    if count_files(opt_dir, "7-sv_cn") < n_hubert * MIN_KEEP:
        log("1B 声纹（v2Pro 系列需要）")
        run("GPT_SoVITS/prepare_datasets/2-get-sv.py", env)
    check("1B 声纹", count_files(opt_dir, "7-sv_cn"), n_hubert, "常见原因：缺 torchcodec 或 FFmpeg（setup.sh 会装）")
    # ---------- 1C 语义 ----------
    sem_path = os.path.join(opt_dir, "6-name2semantic.tsv")
    if count_lines(sem_path) - 1 < n_hubert * MIN_KEEP:
        log("1C 语义编码")
        run("GPT_SoVITS/prepare_datasets/3-get-semantic.py", env)
        part = os.path.join(opt_dir, "6-name2semantic-0.tsv")
        body = open(part, encoding="utf8").read().strip("\n")
        open(sem_path, "w", encoding="utf8").write("item_name\tsemantic_audio\n" + body + "\n")
        os.remove(part)
    check("1C 语义", count_lines(sem_path) - 1, n_hubert)

    # ---------- SoVITS ----------
    sovits_done = glob.glob(os.path.join(GSV, "SoVITS_weights_%s" % VERSION, "%s_e%d_s*.pth" % (exp, a.sovits_epochs)))
    if sovits_done:
        log("SoVITS 已经训练完（%s），跳过" % os.path.basename(sovits_done[0]))
    else:
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
    gpt_done = os.path.join(GSV, "GPT_weights_%s" % VERSION, "%s-e%d.ckpt" % (exp, a.gpt_epochs))
    if os.path.exists(gpt_done):
        log("GPT 已经训练完（%s），跳过" % os.path.basename(gpt_done))
    else:
        # 上次中途断了留下的检查点不接着训：PyTorch 2.6 起默认只读权重，Lightning 续训读不了检查点里的路径对象
        # （实测 Unsupported global: pathlib.WindowsPath，Linux 上是 PosixPath）。GPT 从头训也就几分钟。
        shutil.rmtree(os.path.join(opt_dir, "logs_s1_" + VERSION, "ckpt"), ignore_errors=True)
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
        env["hz"] = "25hz"
        s1_cfg = os.path.join(tmp, "tmp_s1.yaml")
        yaml.dump(data, open(s1_cfg, "w", encoding="utf8"), default_flow_style=False)
        if os.name == "nt":
            # PyTorch 2.9 的 Windows 版跑不了 DDP（gloo 不可用），换成单卡启动器；Linux 服务器照常走官方脚本
            run(os.path.join(ROOT, "s1_single_gpu.py"), env, "--config_file", s1_cfg)
        else:
            run("GPT_SoVITS/s1_train.py", env, "--config_file", s1_cfg)

    # ---------- 打包 ----------
    # Q 盘只有约 0.4 MB/s，全部 6 个模型约 1 GB 要拷 40 分钟，所以只打包最后一轮的两个（其他几轮留在服务器上，需要时再拿）
    log("打包结果")
    out = os.path.join(ROOT, "output", exp)
    shutil.rmtree(out, ignore_errors=True)
    os.makedirs(out)
    got = []
    for pat, rx in (("SoVITS_weights_%s/%s_e*.pth" % (VERSION, exp), r"_e(\d+)_s"), ("GPT_weights_%s/%s-e*.ckpt" % (VERSION, exp), r"-e(\d+)\.ckpt")):
        files = sorted(glob.glob(os.path.join(GSV, pat)), key=lambda f: int(re.search(rx, os.path.basename(f)).group(1)))
        if files:
            print("  %s：%s" % (os.path.dirname(pat), ", ".join(os.path.basename(f) for f in files)))
            shutil.copy2(files[-1], out)
            got.append(os.path.basename(files[-1]))
    ref = os.path.join(ROOT, "ref")
    if os.path.isdir(ref):
        shutil.copytree(ref, os.path.join(out, "ref"), dirs_exist_ok=True)
    if len(got) < 2:
        sys.exit("没有找到训练出来的模型文件（SoVITS 和 GPT 各一个），往上翻看看有没有报错。")
    tar_path = os.path.join(ROOT, "output", exp + "-model.tar.gz")
    with tarfile.open(tar_path, "w:gz") as t:
        t.add(out, arcname=exp + "-model")
    print("\n完成！打包了最后一轮的模型：", ", ".join(got))
    print("把这个文件拿回来：", tar_path, "（%.0f MB）" % (os.path.getsize(tar_path) / 1e6))


if __name__ == "__main__":
    main()
