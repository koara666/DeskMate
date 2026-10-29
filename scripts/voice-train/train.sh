#!/usr/bin/env bash
# 训练千秋的声音。先运行过 setup.sh。
#   bash train.sh --quick                    # 试跑：40 条、各 1 轮，几分钟，确认环境没问题
#   nohup bash train.sh > train.log 2>&1 &   # 正式训练（放后台，SSH 断了也不影响）；看进度：tail -f train.log
# 其他参数见 python train.py --help（比如 --gpu 1、--batch 8、--sovits-epochs 15）
set -eo pipefail
cd "$(dirname "$0")"
source ./env.sh
find_conda || { echo "找不到 conda，先运行 bash setup.sh" >&2; exit 1; }
conda activate "$ENV_DIR"
# torchaudio 读音频要用 conda 环境里的 FFmpeg 动态库
export LD_LIBRARY_PATH="$ENV_DIR/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
python train.py "$@"
