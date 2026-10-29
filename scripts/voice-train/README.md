# 千秋声音训练包

用你自己的《超级弹丸论破 2》里七海千秋的日语原声（取出 711 条，挑出能用的 635 条、约 51 分钟），在你的 4090 服务器上训练一个 GPT-SoVITS 声音模型，训练完拿回来给 DeskMate 用。

> ⚠️ 训练包里的原声和训练出来的模型版权属于原作，只给你自己用：**不要上传到公开的地方**（网盘公开分享、GitHub 等）。DeskMate 的仓库已经把它们排除在外。

## 包里有什么

| 文件 | 是什么 |
|---|---|
| `dataset/wavs/` | 千秋的原声（从游戏里取的，单声道 wav） |
| `dataset/chiaki.list` | 每条原声说的日语（本机用 Whisper large-v3 识别，去掉了太短、识别不可靠、拖长音的 76 条，人名纠错过） |
| `ref/` | 4 条推荐的参考音频和原文（`refs.txt`），以后让她说话时用来定语气 |
| `GPT-SoVITS/` | GPT-SoVITS 源码（固定版本 b4570b8，MIT 许可） |
| `setup.sh` | 在服务器上装环境（只运行一次） |
| `train.sh` / `train.py` | 训练（不用打开网页界面，一条命令跑完） |
| `s1_single_gpu.py` | 只在 Windows 上用（PyTorch 2.9 的 Windows 版跑不了官方 GPT 训练的分布式启动）；Linux 服务器用不到 |

## 服务器要求

- Linux + NVIDIA 显卡（4090 就很好），能用 SSH 登录。
- 硬盘剩余 20 GB 以上；能上网（下载 PyTorch 和模型，国内服务器也行，默认走 hf-mirror.com 镜像）。
- 不需要 root，不需要事先装 conda（没有会自动装一个）。
- **所有东西都装在 `chiaki-voice-train` 这一个文件夹里**：Python 环境、conda（如果要装）、pip / 模型下载缓存、训练结果。不碰你的 `~/.cache`、`~/.conda` 和已有的 conda 环境，训练完删掉这个文件夹就清干净了。

## 一步一步做

「服务器上」的命令在你 SSH 登录服务器以后运行。

### 1. 上传

Claude 已经把下面这些放进了 Q 盘（对应服务器上的 `~/feng`）里单独的文件夹 `chiaki-voice-train`，你的科研项目文件一个都没动：

- `chiaki-voice.tar.gz`（241 MB）：训练包
- `chiaki-voice-scripts.tar.gz`（几十 KB）：脚本的新版本（大包传到一半时脚本又改过，用它覆盖大包里的旧脚本）
- 两个 `.sha256`：核对文件没传坏
- `README.md`：就是这份说明

### 2. 解压、装环境（服务器上）

```bash
cd ~/feng/chiaki-voice-train
sha256sum -c chiaki-voice.tar.gz.sha256 chiaki-voice-scripts.tar.gz.sha256   # 两行都显示「OK」才继续
tar -xzf chiaki-voice.tar.gz
tar -xzf chiaki-voice-scripts.tar.gz       # 一定在上一行之后：用新版脚本覆盖旧的
cd chiaki-voice
bash setup.sh
```

（Q 盘 `\\sshfs\crossai@100.108.18.39!2024` 对应的是服务器上的 `/home/crossai/feng`，不是主目录。）

`setup.sh` 大约 10~30 分钟（看网速；要下载约 3 GB 的 PyTorch 和 1.4 GB 的模型，都是服务器自己下，不经过 Q 盘），最后看到「装好了」就行。海外服务器用 `SOURCE=HF bash setup.sh`。中途断了（比如 SSH 掉线）重新运行一次，会接着下载。

### 3. 试跑几分钟（服务器上）

```bash
bash train.sh --quick
```

只用 40 条、各训练 1 轮，确认环境没问题，大约 5 分钟。开头会列出每张显卡的空闲显存，并自动用**空闲显存最多的那张**（显卡编号和 `nvidia-smi` 一样）。最后出现「完成！」就说明一切正常（试跑的模型不用拿回来）。

### 4. 正式训练（服务器上）

默认自动用空闲显存最多的那张卡。想指定（比如科研任务在用的卡别抢），先 `nvidia-smi` 看编号，再加 `--gpu 编号`：

```bash
nohup bash train.sh > train.log 2>&1 &          # 指定 1 号卡：nohup bash train.sh --gpu 1 > train.log 2>&1 &
tail -f train.log
```

- 第一行让训练在后台跑，**关掉 SSH 窗口也不会停**；第二行看进度（按 Ctrl+C 只是不看了，不会停止训练）。
- 4090 上大约 20~40 分钟。最后一行出现「完成！」和 `chiaki-model.tar.gz` 的路径。
- 中途想再看进度：重新登录后 `cd ~/feng/chiaki-voice-train/chiaki-voice && tail -f train.log`。

### 5. 拿回来

训练完告诉 Claude「训练完了」。Claude 会从 Q 盘把 `chiaki-voice-train/chiaki-voice/output/chiaki-model.tar.gz`（约 300 MB）安全地拷回你电脑，再在你电脑上配好语音服务、接进 DeskMate。Q 盘这条网络实测只有 0.3~0.4 MB/s，拷回来大约要 15~20 分钟，期间别在资源管理器里打开 Q 盘。

### 6. 清理（服务器上，拿回来以后）

```bash
rm -rf ~/feng/chiaki-voice-train
```

在服务器上删比在 Q 盘里删快得多（Q 盘是网络盘，删上万个小文件会很慢，资源管理器可能卡住）。

## 训练出来的是什么

`chiaki-model.tar.gz`（约 300 MB）里只有最后一轮的两个模型，因为 Q 盘太慢，全部 6 个约 1 GB 要拷 40 分钟：

- `chiaki_e12_s….pth`：SoVITS 模型（音色），第 12 轮
- `chiaki-e15.ckpt`：GPT 模型（语气、节奏），第 15 轮
- `ref/`：参考音频

中间几轮（SoVITS 第 4、8 轮，GPT 第 5、10 轮）留在服务器的 `chiaki-voice/GPT-SoVITS/SoVITS_weights_v2ProPlus/` 和 `GPT_weights_v2ProPlus/` 里。如果最后一轮听起来怪（吞字、发音飘），告诉 Claude，单独拿前一轮的回来试（所以**确认声音满意之前先别删服务器上的文件夹**）。

## 出问题了

| 现象 | 怎么办 |
|---|---|
| `CUDA out of memory`（显存不够） | `bash train.sh --batch 6`（服务器上别的程序也在用显卡时） |
| `setup.sh` 下载失败 | 重新运行 `bash setup.sh`，会接着下；还不行就换 `SOURCE=ModelScope bash setup.sh` |
| `nvidia-smi` 找不到 | 这台服务器没装显卡驱动，换一台或联系服务器提供商 |
| GPT 训练那一步报错，报错里有 `NCCL` | `NCCL_P2P_DISABLE=1 NCCL_IB_DISABLE=1 bash train.sh --quick` 再试 |
| 想用第 2 张显卡 | `bash train.sh --gpu 1`（编号看 `nvidia-smi`） |
| 其他报错 | 把 `train.log` 最后几十行（`tail -50 train.log`）截图发给 Claude |

## 这个包是怎么做出来的（给接手的人）

- 取原声：`scripts/voice-train/extract_chiaki.py`（只读游戏的 WAD 文件；语音序号怎么对应到千秋，见脚本开头和 DECISIONS #56）。
- 识别文字：`scripts/voice-train/transcribe.py`（faster-whisper large-v3）。
- 打包：`scripts/voice-train/pack.py`。
- 训练：`train.py` 照 GPT-SoVITS `webui.py` 里 `open1abc` / `open1Ba` / `open1Bb` 的参数写，版本 v2ProPlus，默认 SoVITS 12 轮、GPT 15 轮、batch = 空闲显存 GB / 2（最多 12）。PyTorch 固定 2.9.1（`setup.sh` 在官方 `install.sh` 之前装好）。
- 本机测试：`D:\dev\chiaki-gsv-venv`（Python 3.11 + torch 2.9.1 cu126）里 `python train.py --quick` 全流程通过（RTX 4060 Laptop，约 5 分钟）；GPT 训练在 Windows 上走 `s1_single_gpu.py`。
- 推理：`scripts/voice-train/serve_local.py`（在用户电脑上跑 `api_v2.py`，只监听 127.0.0.1:9880）。本机用试跑模型测过：合成日语、中文各一句，再用 Whisper 识别回来和原文一致。
- 用户说「训练完了」以后：
  1. `python -I -X utf8 scripts/voice-train/safe_copy.py "Q:\chiaki-voice-train\chiaki-voice\output\chiaki-model.tar.gz" chiaki-voice --mbps 0.25`（约 300 MB，20 分钟左右；中途断了再运行一次会续传）
  2. 在 `chiaki-voice\` 里解压 → `chiaki-voice\chiaki-model\`（两个模型 + `ref\`）
  3. `D:\dev\chiaki-gsv-venv\Scripts\python.exe scripts/voice-train/serve_local.py`（默认就找 `chiaki-voice\chiaki-model\`；启动日志里 `Loading Text2Semantic weights from` / `Loading VITS weights from` 后面必须是这个文件夹里的文件，出现 `fall back to default` 就是没找到模型、退回了底模）
  4. DeskMate「设置 → 声音」：引擎选 GPT-SoVITS，接口地址 `http://127.0.0.1:9880`，参考音频填 `chiaki-model\ref\` 里某个 wav 的**完整路径**，「参考音频说的话」照 `refs.txt` 一字不差地填，语言选日语；点「试听一句」。
  5. 现在 DeskMate 发给 GPT-SoVITS 的 `text_lang` 写死 `zh`（她用千秋的音色说中文）；要说日语得先做 ROADMAP v2.2 的「声音语言」。
