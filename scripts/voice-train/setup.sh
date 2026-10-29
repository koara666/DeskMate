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
        # 大文件会被跳转到 HF 的存储。实测偶尔会被解析到连不上的地址、或临时返回错误，
        # wget 会在连接上一等好几分钟、-q 时日志里什么都看不到，所以用 curl：连接 15 秒超时、失败就打印出来再试
        for i in 1 2 3 4 5 6 7 8; do
            curl -fL -sS --connect-timeout 15 --speed-limit 100000 --speed-time 60 -C - -o "$PM/$f" \
                "$HF_BASE/lj1995/GPT-SoVITS/resolve/main/$f" && break
            [ "$i" = 8 ] && { echo "下载失败：$f（重新运行 bash setup.sh 会接着下）" >&2; exit 1; }
            echo "  下载出错，5 秒后重试（第 $i 次）"
            sleep 5
        done
        echo "    $(du -h "$PM/$f" | cut -f1)"
    fi
done
# install.sh 看到 pretrained_models/sv 已经存在，就不会再下载 4.5 GB 的全套模型包

say "PyTorch 2.9.1（固定成本机测试过的版本）"
# 官方 install.sh 装 torch 不写版本，会装到当时最新的；先装好固定版本，它看到已经装了就不会再换
CU="$(echo "$CUDA_VER" | tr 'A-Z' 'a-z')"
if [ "$SOURCE" = HF ]; then
    python -m pip install torch==2.9.1 torchaudio==2.9.1 --index-url "https://download.pytorch.org/whl/$CU"
else
    # 国内：torch 本体从官方源 / 南大镜像下（实测各 20 MB/s 左右；阿里云、上交的 PyTorch 镜像不到 2 MB/s），但不带依赖——
    # 依赖里的 NVIDIA 库在官方源指向 pypi.nvidia.com，国内读超时、几乎下不动，改从阿里云 PyPI 装（实测 26 MB/s）。
    # 不让 pip 直接下：实测它碰上慢的线路会一直死守（0.3 MB/s）。用 curl 下成文件：低于 1 MB/s 超过 30 秒就断开，
    # 换一个源接着下（续传），最后用官方索引里的 sha256 核对。
    WHL="$ROOT/.cache/wheels"
    mkdir -p "$WHL"
    for p in torch torchaudio; do
        name="$p-2.9.1%2B$CU-cp310-cp310-manylinux_2_28_x86_64.whl"
        file="$WHL/${name//%2B/+}"
        want=$(curl -fsS --connect-timeout 15 --retry 5 "https://download.pytorch.org/whl/$CU/$p/" | grep -oE "$name#sha256=[0-9a-f]{64}" | head -1 | cut -d= -f2)
        [ -n "$want" ] || { echo "出错了：官方索引里找不到 $name" >&2; exit 1; }
        for i in $(seq 1 30); do
            [ -s "$file" ] && [ "$(sha256sum "$file" | cut -d' ' -f1)" = "$want" ] && break
            [ "$i" = 30 ] && { echo "下载失败：$name（重新运行 bash setup.sh 会接着下）" >&2; exit 1; }
            for u in "https://download.pytorch.org/whl/$CU/$name" "https://mirrors.nju.edu.cn/pytorch/whl/$CU/$name"; do
                curl -fL -sS --connect-timeout 15 --speed-limit 1000000 --speed-time 30 -C - -o "$file" "$u" && break
                echo "  $p 下载中断（已有 $(du -h "$file" 2>/dev/null | cut -f1)），换一个源接着下"
                sleep 2
            done
            # 下完但对不上：删掉重下
            if [ -s "$file" ] && [ "$(stat -c %s "$file")" -gt 100000000 ] && [ "$(sha256sum "$file" | cut -d' ' -f1)" != "$want" ]; then
                curl -fsS --connect-timeout 15 -I "https://download.pytorch.org/whl/$CU/$name" | grep -qi "content-length: $(stat -c %s "$file")" \
                    && { echo "  $p 核对失败，重新下载"; rm -f "$file"; }
            fi
        done
        echo "  $p 下好了，核对通过"
    done
    python -m pip install --no-deps "$WHL/torch-2.9.1+$CU-cp310-cp310-manylinux_2_28_x86_64.whl" \
        "$WHL/torchaudio-2.9.1+$CU-cp310-cp310-manylinux_2_28_x86_64.whl"
    python - > "$TMPDIR/torch-deps.txt" <<'PY'
import importlib.metadata as md
from pip._vendor.packaging.requirements import Requirement
for name in ("torch", "torchaudio"):
    for r in md.requires(name) or []:
        req = Requirement(r)
        if req.marker is None or req.marker.evaluate({"extra": ""}):
            print(str(req).split(";")[0].strip())
PY
    python -m pip install -r "$TMPDIR/torch-deps.txt" -i https://mirrors.aliyun.com/pypi/simple/
fi
python -m pip install torchcodec==0.8.1

say "运行 GPT-SoVITS 官方安装脚本（依赖、日语词典）"
# 官方脚本里用 wget 下日语词典等：给它连接 / 读取各 20 秒的超时（只在这个文件夹里生效，不改服务器的配置）
printf 'timeout = 20\ntries = 30\nwaitretry = 3\n' > "$ROOT/.cache/wgetrc"
export WGETRC="$ROOT/.cache/wgetrc"
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
