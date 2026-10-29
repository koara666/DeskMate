#!/usr/bin/env bash
# 在 Linux + NVIDIA 显卡的服务器上装好训练环境（只需要运行一次，大约 10~30 分钟，看网速）。
# 用法（在训练包文件夹里）：
#   bash setup.sh                 # 默认从 hf-mirror.com 下载模型（国内服务器能用）
#   SOURCE=HF bash setup.sh       # 海外服务器：直接从 huggingface.co 下载
#   CUDA_VER=CU128 bash setup.sh  # 显卡驱动很新（nvidia-smi 右上角 CUDA Version ≥ 12.8）可以选 12.8
# 所有东西都装在这个文件夹里（见 env.sh），删掉文件夹就清干净了。
set -eo pipefail
cd "$(dirname "$0")"
source ./env.sh
SOURCE=${SOURCE:-HF-Mirror}
CUDA_VER=${CUDA_VER:-CU126}

say() { printf '\n\033[1;36m==== %s\033[0m\n' "$*"; }

say "检查显卡"
if ! command -v nvidia-smi >/dev/null 2>&1; then
    echo "没有找到 nvidia-smi：这台机器没有装 NVIDIA 显卡驱动，训练不了。" >&2
    exit 1
fi
nvidia-smi --query-gpu=index,name,memory.used,memory.total,driver_version --format=csv

say "准备 conda"
if ! find_conda; then
    echo "服务器上没有 conda，装一个 Miniforge 到 $ROOT/miniforge3（只在这个文件夹里）"
    url_cn="https://mirrors.tuna.tsinghua.edu.cn/github-release/conda-forge/miniforge/LatestRelease/Miniforge3-Linux-x86_64.sh"
    url_gh="https://github.com/conda-forge/miniforge/releases/latest/download/Miniforge3-Linux-x86_64.sh"
    wget -q --show-progress -O "$TMPDIR/miniforge.sh" "$url_cn" || wget -q --show-progress -O "$TMPDIR/miniforge.sh" "$url_gh"
    bash "$TMPDIR/miniforge.sh" -b -p "$ROOT/miniforge3"
    rm -f "$TMPDIR/miniforge.sh"
    find_conda
fi

say "Python 3.10 环境（$ENV_DIR）"
if [ ! -x "$ENV_DIR/bin/python" ]; then
    conda create -y -p "$ENV_DIR" -c conda-forge --override-channels python=3.10
fi
conda activate "$ENV_DIR"

say "下载预训练模型（只下训练要用的，约 1.4 GB）"
case "$SOURCE" in
    HF) HF_BASE="https://huggingface.co" ;;
    *) HF_BASE="https://hf-mirror.com" ;;
esac
PM="$ROOT/GPT-SoVITS/GPT_SoVITS/pretrained_models"
for f in \
    chinese-roberta-wwm-ext-large/config.json \
    chinese-roberta-wwm-ext-large/tokenizer.json \
    chinese-roberta-wwm-ext-large/pytorch_model.bin \
    chinese-hubert-base/config.json \
    chinese-hubert-base/preprocessor_config.json \
    chinese-hubert-base/pytorch_model.bin \
    v2Pro/s2Gv2ProPlus.pth \
    v2Pro/s2Dv2ProPlus.pth \
    sv/pretrained_eres2netv2w24s4ep4.ckpt \
    s1v3.ckpt; do
    mkdir -p "$PM/$(dirname "$f")"
    if [ ! -s "$PM/$f" ]; then
        echo "  $f"
        wget -c --tries=25 --wait=5 --read-timeout=40 -q --show-progress -O "$PM/$f" "$HF_BASE/lj1995/GPT-SoVITS/resolve/main/$f"
    fi
done
# install.sh 看到 pretrained_models/sv 已经存在，就不会再下载 4.5 GB 的全套模型包

say "PyTorch 2.9.1（固定成本机测试过的版本）"
# 官方 install.sh 装 torch 不写版本，会装到当时最新的；先装好固定版本，它看到已经装了就不会再换
TORCH_INDEX="https://download.pytorch.org/whl/$(echo "$CUDA_VER" | tr 'A-Z' 'a-z')"
python -m pip install torch==2.9.1 torchaudio==2.9.1 --index-url "$TORCH_INDEX"
python -m pip install torchcodec==0.8.1

say "运行 GPT-SoVITS 官方安装脚本（依赖、日语词典）"
cd "$ROOT/GPT-SoVITS"
# 只用 conda-forge：不碰 Anaconda 官方频道（新版 conda 用官方频道要先接受服务条款，非交互运行会直接报错）
sed -i 's/conda install --yes --quiet -c conda-forge/conda install --yes --quiet --override-channels -c conda-forge/' install.sh
bash install.sh --device "$CUDA_VER" --source "$SOURCE"

say "检查"
python -c "import torch; print('PyTorch', torch.__version__, '显卡可用:', torch.cuda.is_available(), torch.cuda.get_device_name(0) if torch.cuda.is_available() else '')"
python -c "import pyopenjtalk; print('日语词典 OK:', pyopenjtalk.g2p('こんにちは'))"
export LD_LIBRARY_PATH="$ENV_DIR/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
python -c "import glob, torchaudio; w, sr = torchaudio.load(sorted(glob.glob('$ROOT/dataset/wavs/*.wav'))[0]); print('读音频 OK:', sr)"
say "装好了。下一步：bash train.sh --quick（先试跑几分钟）"
