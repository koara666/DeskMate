# -*- coding: utf-8 -*-
"""单卡跑 GPT-SoVITS 的 GPT 训练（GPT_SoVITS/s1_train.py），不改官方源码。

为什么要它：官方 s1_train.py 总是用 DDP（多卡分布式），Windows 上走 gloo。
PyTorch 2.9 的 Windows 版既没编 libuv、gloo 也没有可用的传输层（实测
`makeDeviceForHostname(): unsupported gloo device`），所以 Windows 上 GPT 训练一定起不来。
Linux 服务器走 NCCL 没这个问题，train.py 只在 Windows 上用这个启动器。

做法：把 DDPStrategy 换成单卡（Lightning 的 "auto"），并让没初始化分布式时
get_world_size() / get_rank() 返回 1 / 0（官方的 bucket_sampler 会调用它们），然后原样运行 s1_train.py。
用法（在 GPT-SoVITS 目录里，参数和 s1_train.py 一样）：python s1_single_gpu.py --config_file xxx.yaml
"""
import os
import runpy
import sys

import pytorch_lightning.strategies as pl_strategies
import torch.distributed as dist

_world_size, _rank = dist.get_world_size, dist.get_rank


def _single_world_size(*args, **kwargs):
    return _world_size(*args, **kwargs) if dist.is_initialized() else 1


def _single_rank(*args, **kwargs):
    return _rank(*args, **kwargs) if dist.is_initialized() else 0


dist.get_world_size, dist.get_rank = _single_world_size, _single_rank
pl_strategies.DDPStrategy = lambda *args, **kwargs: "auto"

if __name__ == "__main__":  # Windows 上读数据的子进程会重新导入主模块，别让它们再启动一次训练
    script = os.path.join(os.getcwd(), "GPT_SoVITS", "s1_train.py")
    sys.argv = [script] + sys.argv[1:]
    runpy.run_path(script, run_name="__main__")
