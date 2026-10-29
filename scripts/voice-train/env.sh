# 被 setup.sh / train.sh 引用（source）：把所有东西都放在训练包文件夹里面。
# conda 环境、Miniforge（服务器上没有 conda 时才装）、pip / HuggingFace / conda 的下载缓存、临时文件——
# 训练完删掉整个文件夹，服务器上就不留任何东西（不碰你的 ~/.cache、~/.conda 和已有的 conda 环境）。
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ENV_DIR="$ROOT/env"
export PIP_CACHE_DIR="$ROOT/.cache/pip"
export HF_HOME="$ROOT/.cache/huggingface"
export XDG_CACHE_HOME="$ROOT/.cache"
export CONDA_PKGS_DIRS="$ROOT/.cache/conda-pkgs"
export TMPDIR="$ROOT/.cache/tmp"
export TORCH_HOME="$ROOT/.cache/torch"
export MPLCONFIGDIR="$ROOT/.cache/matplotlib"
export NUMBA_CACHE_DIR="$ROOT/.cache/numba"
# 显卡编号按 PCI 顺序，和 nvidia-smi 显示的一致（不然 --gpu 1 可能用到别的卡）
export CUDA_DEVICE_ORDER=PCI_BUS_ID
mkdir -p "$PIP_CACHE_DIR" "$HF_HOME" "$CONDA_PKGS_DIRS" "$TMPDIR" "$TORCH_HOME" "$MPLCONFIGDIR" "$NUMBA_CACHE_DIR"

# 找 conda：优先用包里自己装的，其次用服务器上已有的（只借用 conda 命令，环境建在包里）
find_conda() {
    if [ -x "$ROOT/miniforge3/bin/conda" ]; then
        source "$ROOT/miniforge3/etc/profile.d/conda.sh"
    elif command -v conda >/dev/null 2>&1; then
        source "$(conda info --base)/etc/profile.d/conda.sh"
    else
        return 1
    fi
}
