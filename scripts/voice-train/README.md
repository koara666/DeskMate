# 千秋声音训练包

用你自己的《超级弹丸论破 2》里七海千秋的日语原声（711 条、约 52 分钟），在你的 4090 服务器上训练一个 GPT-SoVITS 声音模型，训练完拿回来给 DeskMate 用。

> ⚠️ 训练包里的原声和训练出来的模型版权属于原作，只给你自己用：**不要上传到公开的地方**（网盘公开分享、GitHub 等）。DeskMate 的仓库已经把它们排除在外。

## 包里有什么

| 文件 | 是什么 |
|---|---|
| `dataset/wavs/` | 千秋的原声（从游戏里取的，单声道 wav） |
| `dataset/chiaki.list` | 每条原声说的日语（本机用 Whisper 识别的，已经去掉不可靠的） |
| `ref/` | 推荐的参考音频（以后让她说话时用） |
| `GPT-SoVITS/` | GPT-SoVITS 源码（固定版本 b4570b8，MIT 许可） |
| `setup.sh` | 在服务器上装环境（只运行一次） |
| `train.sh` / `train.py` | 训练（不用打开网页界面，一条命令跑完） |

## 服务器要求

- Linux + NVIDIA 显卡（4090 就很好），能用 SSH 登录。
- 硬盘剩余 20 GB 以上；能上网（下载 PyTorch 和模型，国内服务器也行，默认走 hf-mirror.com 镜像）。
- 不需要 root，不需要事先装 conda（没有会自动装一个）。
- **所有东西都装在 `chiaki-voice-train` 这一个文件夹里**：Python 环境、conda（如果要装）、pip / 模型下载缓存、训练结果。不碰你的 `~/.cache`、`~/.conda` 和已有的 conda 环境，训练完删掉这个文件夹就清干净了。

## 一步一步做

「服务器上」的命令在你 SSH 登录服务器以后运行。

### 1. 上传

Claude 已经把 `chiaki-voice.tar.gz` 放进了 Q 盘（就是服务器上你的主目录）里单独的文件夹 `chiaki-voice-train`，旁边的 `chiaki-voice.tar.gz.sha256` 用来核对文件没传坏。你的科研项目文件一个都没动。

### 2. 解压、装环境（服务器上）

```bash
cd ~/chiaki-voice-train
sha256sum -c chiaki-voice.tar.gz.sha256      # 显示「OK」才继续
tar -xzf chiaki-voice.tar.gz
cd chiaki-voice
bash setup.sh
```

（如果 `cd ~/chiaki-voice-train` 说找不到，用 `ls ~` 看看；Q 盘对应的是 SSH 登录后的主目录。）

`setup.sh` 大约 10~30 分钟（看网速），最后看到「装好了」就行。海外服务器用 `SOURCE=HF bash setup.sh`。中途断了重新运行一次，会接着下载。

### 3. 试跑几分钟（服务器上）

```bash
bash train.sh --quick
```

只用 40 条、各训练 1 轮，确认环境没问题。最后出现「完成！」就说明一切正常（试跑的模型不用拿回来）。

### 4. 正式训练（服务器上）

先 `nvidia-smi` 看一眼哪张显卡空着（科研任务在用的卡别抢）。默认用 0 号卡，想用别的加 `--gpu 编号`：

```bash
nohup bash train.sh > train.log 2>&1 &          # 用 1 号卡：nohup bash train.sh --gpu 1 > train.log 2>&1 &
tail -f train.log
```

- 第一行让训练在后台跑，**关掉 SSH 窗口也不会停**；第二行看进度（按 Ctrl+C 只是不看了，不会停止训练）。
- 4090 上大约 20~40 分钟。最后一行出现「完成！」和 `chiaki-model.tar.gz` 的路径。
- 中途想再看进度：重新登录后 `cd ~/chiaki-voice-train/chiaki-voice && tail -f train.log`。

### 5. 拿回来

训练完告诉 Claude「训练完了」。Claude 会从 Q 盘把 `chiaki-voice-train/chiaki-voice/output/chiaki-model.tar.gz`（几百 MB）安全地拷回你电脑，再在你电脑上配好语音服务、接进 DeskMate。

### 6. 清理（服务器上，拿回来以后）

```bash
rm -rf ~/chiaki-voice-train
```

在服务器上删比在 Q 盘里删快得多（Q 盘是网络盘，删上万个小文件会很慢，资源管理器可能卡住）。

## 训练出来的是什么

`chiaki-model.tar.gz` 里有：

- `chiaki_e4_s….pth`、`chiaki_e8_s….pth`、`chiaki_e12_s….pth`：SoVITS 模型（音色），分别是第 4、8、12 轮存的
- `chiaki-e5.ckpt`、`chiaki-e10.ckpt`、`chiaki-e15.ckpt`：GPT 模型（语气、节奏），第 5、10、15 轮
- `ref/`：参考音频

一般用最后一轮的就好；如果听起来怪（吞字、发音飘），换前一轮的试试。一共几百 MB。

## 出问题了

| 现象 | 怎么办 |
|---|---|
| `CUDA out of memory`（显存不够） | `bash train.sh --batch 6`（服务器上别的程序也在用显卡时） |
| `setup.sh` 下载失败 | 重新运行 `bash setup.sh`，会接着下；还不行就换 `SOURCE=ModelScope bash setup.sh` |
| `nvidia-smi` 找不到 | 这台服务器没装显卡驱动，换一台或联系服务器提供商 |
| 想用第 2 张显卡 | `bash train.sh --gpu 1` |
| 其他报错 | 把 `train.log` 最后几十行（`tail -50 train.log`）截图发给 Claude |

## 这个包是怎么做出来的（给接手的人）

- 取原声：`scripts/voice-train/extract_chiaki.py`（只读游戏的 WAD 文件；语音序号怎么对应到千秋，见脚本开头和 DECISIONS #56）。
- 识别文字：`scripts/voice-train/transcribe.py`（faster-whisper large-v3）。
- 打包：`scripts/voice-train/pack.py`。
- 训练：`train.py` 照 GPT-SoVITS `webui.py` 里 `open1abc` / `open1Ba` / `open1Bb` 的参数写，版本 v2ProPlus，默认 SoVITS 12 轮、GPT 15 轮、batch = 显存 GB / 2。
