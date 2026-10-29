#!/usr/bin/env bash
# 训练千秋的声音。先运行过 setup.sh。
#   bash train.sh --quick                    # 试跑：40 条、各 1 轮，几分钟，确认环境没问题
#   nohup bash train.sh > train.log 2>&1 &   # 正式训练（放后台，SSH 断了也不影响）；看进度：tail -f train.log
# 其他参数见 python train.py --help（比如 --sovits-epochs 15、--batch 8、--gpu 1）
set -eo pipefail
cd "$(dirname "$0")"
ENV_NAME=${ENV_NAME:-chiaki-gsv}
if command -v conda >/dev/null 2>&1; then
    source "$(conda info --base)/etc/profile.d/conda.sh"
else
    source "$HOME/miniforge3/etc/profile.d/conda.sh"
fi
conda activate "$ENV_NAME"
python train.py "$@"
