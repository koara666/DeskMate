# 进度日志

> 最新条目在最上面。超过 300 行时把旧条目压缩成摘要移到 `docs/progress-archive/`。

## 2026-10-30 00:30（0.4.0 已发布，声音模型待用户上传）
- 背景：用户试听后说音色挺像、气口不像（中文原因）、先要听日语——0.3.2 已发布（端到端 51/51；启动 0.58 秒、空闲 CPU 0.02~0.05%、内存 136 MB、安装包 3.56 MiB）。随后给了一批任务：声音适配所有场景、修 ROADMAP 里记着的问题、全仓库扫冗余、汇成新版、声音模型也传 GitHub。
- 完成（版本号已改 0.4.0，`npm run check` 通过，Rust 约 350 个测试 + 前端 101 个）：
  - **声音适配**（DECISIONS #62）：审计发现出口基本都汇到 `proactive::deliver`，缺的是日语版——模型现场生成的话（主动发言 / 每日播报 / 很久没聊天的招呼 / 胜负反应）现在让模型顺带写 `[[日语：]]`；日语对照表落盘（迁移 v10，重启后提醒到点也能念日语）；海龟汤 / 二十问主持人的「是 / 不是……」用本地日语配音。
  - **v2.3 四项**：表情（#61，拖动不再笑）、方块对战速度（降到人手范围）、海龟汤 / 二十问（#60，现场出题 + 事实表 + 本地校验）、联网搜索（#59，新闻 / 最新消息 / 搜一下；设置 → 模型可关）。
  - **代码清理**：四处复制的「记进聊天记录」抽成 `chat::record_exchange`；删掉无人使用的 `Rect::contains_point`；仅测试用的 `set_status` / `local_draft` / `MockBackend::instant` / `start_with` 加 `#[cfg(test)]`。依赖（Rust 的 24 项、npm 的 8 个包）和 CSS 类都核对过，没有多余的。
- 完成：**0.4.0 端到端 52/52 通过**（新增「模型页有联网开关」）；实测 安装包 3.60 MiB、启动 0.70 秒、空闲 CPU 0.01%、私有内存 132.5 MB（0.3.2：3.56 MiB / 0.58 秒 / 0.02~0.05% / 136 MB），都在目标内；gitleaks 扫全部历史无泄露，推送 `v0.4.0` 标签由 `release.yml` 自动发布。
- 待用户：NEEDS_USER #14——声音模型传到 GitHub 的单独发布页（**必须标预发布**，否则检查更新会收不到以后的版本，DECISIONS #63）。自动模式拦下了「下载并运行 gh」，所以要用户在网页上点一下，或自己装 gh 并登录。
- 下一步：端到端 + 发布 0.4.0；用户上传模型后核对 SHA-256、删掉 `docs/voice/CHIAKI_MODEL.md` 开头的「还没上传」提示，README 补一句下载说明；气口不像的问题——用中文念时 GPT-SoVITS 是跨语言，之后可以考虑把中文原声也识别、做中日双语训练（用户说先不纠结）。
- 已知问题：联网搜索在中国大陆可能连不上 DuckDuckGo / Google（连不上时会如实说没查到）；现场出的海龟汤质量取决于聊天模型，出题过检查（长度、事实清单、不抄汤底）但不保证好玩，不好就换一局。
- 本次决策：DECISIONS #59~#63

## 2026-10-29 21:30
- 完成：**千秋的声音训练完了并拉回本机**。chain.sh 全自动：装环境 6 分钟 → 试跑 1 分钟 → 正式训练 5 分钟（SoVITS 12 轮 + GPT 15 轮，4090）；`scp` 拉回 306 MB 只要 157 秒（SSH 通道 ~1.5 MB/s，比 Q 盘快 5 倍），sha256 与服务器一致，解压到 `chiaki-voice/chiaki-model/`。本机 `serve_local.py` 加载的是 e15 / e12（日志确认没有退回底模），按 DeskMate 的请求格式合成 4 句日语，Whisper 识别回来 3 句一字不差、第 4 句开头的「うーん」被识别成「ふぅ」。合成速度：第一句 20 秒（预热），之后每句约 2 秒。`npm run check` 全部通过（0.3.2）。
- 进行中：无（本机语音服务已停；服务器上 `~/feng/chiaki-voice-train/` 先留着，中间几轮模型还要用）。
- 没做（原因）：**0.3.2 的端到端和发布**——用户在全屏（`SHQueryUserNotificationState` = 2），按规定不启动可见窗口；发布要等端到端过（AGENTS §6）。
- 下一步：① 用户试听（NEEDS_USER #13：`chiaki-voice\start-voice.bat`，声音页填参考音频），不像就用 `scp` 把服务器上 `GPT-SoVITS/SoVITS_weights_v2ProPlus/`、`GPT_weights_v2ProPlus/` 里的中间几轮拉回来换着试；② 用户不在全屏时：`npx tauri build --no-bundle && npm run e2e` → `scripts/measure.ps1` → 推送 `v0.3.2` 标签发布；③ ROADMAP 的 v2.3 待办。
- 已知问题：音色像不像只有用户的耳朵能判断，Whisper 只能证明「能听懂、读对了」；`serve_local.py` 第一次合成要预热约 20 秒（DeskMate 对 GPT-SoVITS 的合成超时是 90 秒，`voice.rs` 782 行，够用）。
- 本次决策：DECISIONS #56 补充 5

## 2026-10-29 21:15（进行中；下面已被 21:30 条目取代）
- 完成：**服务器装环境「一直卡在下载」彻底解决**（DECISIONS #56 补充 5）。真正原因不是 pip：官方 `install.sh` 用 conda 从 conda-forge 装 gcc 11 / ffmpeg / cmake / make / unzip，conda 的软件包本体被跳转到国外对象存储，国内下不动（之前九次重试都栽在这）。`setup.sh` 现在：只用国内 conda 镜像装 ffmpeg（不到 1 分钟），install.sh 里的 conda 步骤全部跳过（服务器自带 gcc/cmake 等），wget 换成 curl 小脚本，`TERM=xterm`（官方脚本下载成功后的 `tput` 在后台会报错、被当成失败整个重跑，G2PW 520 MB 因此白下两遍）。装环境 6 分钟走完，自检：RTX 4090 可用、日语词典 OK、读音频 OK。
- 进行中：服务器 `chain.sh` 无人值守——`train.sh --quick`（21:09 开始，已在 SoVITS 训练）→ 通过后自动 `train.sh` 正式训练。结果看 `~/feng/chiaki-voice-train/chiaki-voice/chain.log` 最后一行（exit 0 完成 / 2 试跑失败看 quick.log / 3 训练失败看 train.log）。
- 下一步：chain.log 出现 `exit 0` → `scp` / `sftp reget` 拉回 `output/chiaki-model.tar.gz` → 本机解压到 `chiaki-voice/chiaki-model/` → `serve_local.py` 合成几句、Whisper 识别回来对照 → 跑端到端、实测、发布 0.3.2。
- 已知问题：停进程时别漏了孤儿——停掉 install.sh 后，它启动的 wget 小脚本（父进程变成 1）还会继续下载；用 `ps -o pid,ppid,cmd` 看 PPID 为 1 且 cwd 在训练文件夹里的残留，按进程号停。hf-mirror 的大文件仍时快时慢，curl 小脚本会断开续传。

## 2026-10-29 20:20（进行中）
- 背景：用户重连了 Q 盘；决定千秋的声音主要说日语（中文以后再说）；让 agent 自己在服务器上训练（用户本机 `~/.ssh/config` 已配好密钥登录）。
- 完成：
  - 训练包传完：Q 盘 = 服务器 `/home/crossai/feng`（不是主目录），在 `~/feng/chiaki-voice-train/` 里 sha256 核对通过并解压。以后传文件、拉模型都走 SSH（`scp` / `sftp reget`），不再走 Q 盘。
  - **0.3.2 千秋说日语**（DECISIONS #58）：设置「她说话的语言」（默认日语）；247 句台词全部写了日语版；聊天回复附 `[[日语：]]`；兜底用聊天模型翻译；GPT-SoVITS 的 `text_lang` 跟着语言走；系统语音按语言挑声音。版本号已改 0.3.2、更新说明已写（`docs/release-notes/v0.3.2.md`），**还没跑端到端、没发布**。
  - 服务器装环境踩的坑都写进了 `setup.sh` 和 DECISIONS #56 补充 4：wget 偶尔连到不通的地址一直等（下载改用 curl、install.sh 的 wget 用文件夹里的 wgetrc 加超时）；hf-mirror 偶尔 429 / 小文件不响应（重试；卡住的小配置文件从本机 scp）；`pypi.nvidia.com` 国内读超时（torch 本体走官方源 `--no-deps`，依赖走阿里云 PyPI）。
- 进行中（20:50）：服务器上 `setup.sh`（第九次）在跑最后一步官方 install.sh；预训练模型、torch 2.9.1+cu126 和依赖都已装好。服务器上 `chain.sh` 无人值守：等 setup 结束 → `train.sh --quick` → `train.sh` 正式训练，结果在 `~/feng/chiaki-voice-train/chiaki-voice/chain.log` 最后一行（exit 0 = 完成；1 装环境失败看 setup.log；2 试跑失败看 quick.log；3 训练失败看 train.log）。用户嫌盯得太勤、额度烧得多：以后长任务一律服务器端串起来跑，本机只低频（5 分钟）查一次结果。
- 下一步：装好 → `bash train.sh --quick` → `nohup bash train.sh > train.log` 正式训练 → 用 sftp reget 拉回 `output/chiaki-model.tar.gz` → 本机解压到 `chiaki-voice/chiaki-model/` → `chiaki-voice/start-voice.bat`（或 `serve_local.py`）→ 合成几句、Whisper 识别回来检查 → 跑端到端、实测、发布 0.3.2。
- 工具（在本次会话的临时目录里，换人接手要重写）：SSH 一律用 `C:\Windows\System32\OpenSSH\ssh.exe`（Git Bash 自带的 ssh 读不到配置）；远程长任务用 `setsid -f` 后台跑、写日志，本机每 30 秒看一次。

## 2026-10-29 19:10
- 完成：
  - **本机全流程跑通**（DECISIONS #56 补充 3）：`train.py --quick` 从头到尾通过（预处理四步 40/40 → SoVITS → GPT → 打包，RTX 4060 Laptop 6 分 40 秒）。GPT 训练在 Windows 上起不来的真正原因是 PyTorch 2.9.1 的 Windows 版 gloo 根本不可用（和主机名、代理无关），新增 `s1_single_gpu.py`（单卡启动器，不改官方源码，只在 Windows 上用）；去掉了和官方 DataLoader 冲突的「Windows 上 0 个读数据进程」。
  - **推理验证**：`serve_local.py` 加载试跑模型，按 DeskMate 的请求格式合成日语两句、中文一句，再用 Whisper 识别回来，和原文一字不差。修了 `serve_local.py` 的相对路径：api_v2.py 找不到模型时会悄悄退回底模、还改写配置文件。
  - **服务器上更稳**：PyTorch 固定 2.9.1 / torchcodec 0.8.1（官方 install.sh 不写版本）；显卡编号和 nvidia-smi 一致、不填 `--gpu` 时自动用空闲显存最多的卡、查显存改用 nvidia-smi（不在别人的卡上建 CUDA 上下文）；已经训完的步骤重跑时跳过；GPT 中途断了从头训（PyTorch 2.6+ 续训读不了检查点里的路径对象，本机实测）；只打包最后一轮的两个模型（约 300 MB）。
  - `safe_copy.py`：续传（逐块核对已写部分）、1 MB 一块、默认 0.25 MB/s、卡住提示；`pack.py --scripts-only`（几十 KB 的脚本小包）。
  - ROADMAP 新增「v2.3 待办」：用户反馈的表情对不上、方块对战不公平、海龟汤 / 猜东西答错、联网——用户说先记着、声音训练做完再做。
  - 删掉误提交的 `__pycache__/*.pyc`，`.gitignore` 加 Python 缓存。
- 进行中：**往 Q 盘传训练包又卡住了**——前 210 MB 读回核对全对，补写剩下 31 MB 时 sshfs 卡死（18:46 起没进展）。原因见 DECISIONS #56 的「Q 盘卡死」：Tailscale 走香港中转，sshfs 卡死后访问 Q 盘的进程都停在内核里、杀不掉（本机现在有两个这样的 python 进程：上传 21656、查空间 25012，重连 Q 盘后会自己退出）。要用户重连 Q 盘。
- 下一步：用户重连 Q 盘后——
  1. `python -I -X utf8 scripts/voice-train/safe_copy.py chiaki-voice.tar.gz "Q:\chiaki-voice-train"`（续传，只剩 31 MB），后台跑 + Monitor 盯「卡住了」；
  2. 同样传 `chiaki-voice-scripts.tar.gz`（已打好，在仓库根目录）和 `scripts/voice-train/README.md`；
  3. 如果又卡在 210 MB：可能是服务器硬盘满了或有配额，请用户在服务器上 `df -h ~`；
  4. 告诉用户服务器上的命令（NEEDS_USER #13），等训练完拷回模型（README「给接手的人」里有步骤）。
- 已知问题：本机试跑模型在 `chiaki-voice/output/chiaki_quick/`（只训练 1 轮，只用来测链路）。DeskMate 发给 GPT-SoVITS 的 `text_lang` 还写死 `zh`。
- 本次决策：DECISIONS #56 补充 3、Q 盘卡死

## 2026-10-29 16:45
- 完成：
  - **Q 版形象**（DECISIONS #57）：用户自己生成的 23 格表情拼图 `nanami.png`（根目录，已 .gitignore）→ `scripts/chibi/slice_sheet.py` 切成 23 张透明背景 PNG（253×223，底边对齐），放进 `%APPDATA%\DeskMate\characters\nanami-chiaki\assets\`。23 个名字和 CHIAKI_PROMPTS 第四节完全对上，够用。第一次抠图把脸颊抠穿了（皮肤暖白被当成背景、腮红被当成标签），已修好。
  - **声音训练包**（DECISIONS #56）：Whisper large-v3（`D:\dev\models\faster-whisper-large-v3`）识别 711 条 → 去掉幻听（「ご視聴ありがとうございました」）、太短、拖长音的 76 条 → 635 条 / 51.2 分钟；人名纠错（七道明→ななみちあき，七海→ななみ，日向→ひなた：pyopenjtalk 会把日向读成ひゅうが）。验证了序号对应：(1,1)「じゃあ、その謎からいってみよっか」= “Then...let's start with that mystery”，(99,101)「この教室には、私たちみたいな新入生が集められてる」= “freshmen like us have been gathered in this classroom”。4 条参考音频放 `ref/`。
  - **本机实测**（`D:\dev\chiaki-gsv-venv`，Python 3.11 + torch 2.9.1 cu126 + GPT-SoVITS 依赖，pyopenjtalk 现场编译成功）：`train.py --quick` 预处理 1A / 1B / 声纹 / 1C 都是 40/40，SoVITS 训练出模型。发现并修了：声纹那步缺 torchcodec 时**每条都失败但脚本照样往下走**——`train.py` 现在每一步数产出，不到 90% 就停下并说明原因；服务器上 `setup.sh` 最后会实际读一条音频、`train.sh` 把 conda 环境的 FFmpeg 库加进 `LD_LIBRARY_PATH`。GPT 训练在 Windows 上卡在 gloo 分布式初始化（官方代码写死 `USE_LIBUV=0`、主机名解析失败），Linux 走 NCCL 不受影响，由用户在服务器上的 `--quick` 验证。
  - 服务器上所有东西装在 `chiaki-voice-train/` 里（`env.sh`：conda 环境、需要时的 Miniforge、pip / HF / conda 缓存），只用 conda-forge，batch 按空闲显存算。
  - 训练包 `chiaki-voice.tar.gz`（241 MB，1074 个文件，不含预训练模型）用 `safe_copy.py` 传到 `Q:\chiaki-voice-train\`（Q 盘 = 用户服务器主目录、科研项目目录；只新建这一个文件夹）。
- 进行中：等用户在服务器上训练（NEEDS_USER #13）。
- 下一步：用户说训练完 → `safe_copy.py` 从 `Q:\chiaki-voice-train\chiaki-voice\output\chiaki-model.tar.gz` 拷回 → 本机 `api_v2.py`（tts_infer.yaml 的 custom 段指向训练好的模型）→ DeskMate 声音页接上、试听；再做 ROADMAP v2.2 的「声音语言」（日语输出）。
- 已知问题：本机的 FFmpeg 动态库在 `D:\dev\ffmpeg-shared`，靠 venv 里的 `zz_ffmpeg_dll.pth` 加载（只影响本机 venv）。
- 本次决策：DECISIONS #56（补充）、#57

## 2026-10-29 12:40（用户要关电脑，在这里停下）
- 停在：本机测试环境下载中被中断（PyTorch 2.9.1 cu126 没装完）。Whisper large-v3 模型已下完，放在 `D:\dev\models\faster-whisper-large-v3`（不用再下）。
- 已经在硬盘上、下次直接用：`chiaki-voice/dataset/wavs/`（711 条原声）、`chiaki-voice/GPT-SoVITS/`（b4570b8 源码）、`chiaki-voice/GPT-SoVITS/GPT_SoVITS/pretrained_models/`（训练用的 1.4 GB 预训练模型）。
- 下次接着做：
  1. 建 venv（Python 3.11：`C:\Users\京东\AppData\Local\Programs\Python\Python311\python.exe`；建议放 `D:\dev\chiaki-gsv-venv`，以后本机推理服务也能用）→ `pip install torch==2.9.1 torchaudio==2.9.1 --index-url https://download.pytorch.org/whl/cu126` → `pip install -r chiaki-voice/GPT-SoVITS/extra-req.txt --no-deps` 和 `requirements.txt`（Windows 上 opencc / jieba_fast 可能要编译，装不上就跳过，只影响中文）。
  2. `WHISPER_MODEL=D:\dev\models\faster-whisper-large-v3 python scripts/voice-train/transcribe.py chiaki-voice` → 抽查 `dataset/asr.json`。
  3. 挑 3 条 3~10 秒、语气平稳的放 `chiaki-voice/ref/`（附日语原文）。
  4. 本机 4060：`python chiaki-voice/train.py --quick`（先 `python scripts/voice-train/pack.py chiaki-voice` 把脚本复制进去）跑通。
  5. `pack.py` 打包 → 把 README 里的命令告诉用户。

## 2026-10-29 12:35（进行中）
- 背景：用户要用自己的 4090 服务器训练千秋的声音，没用过 GPT-SoVITS，要我把游戏原声和训练代码打包好、给命令（DECISIONS #56）。
- 完成：
  - 找到千秋的语音：`dr2_data.wad` 序号 4920~5630（711 条、约 52 分钟），方法见 DECISIONS #56；`scripts/voice-train/extract_chiaki.py` 已把它们转成 wav 放进 `chiaki-voice/dataset/wavs/`（`/chiaki-voice/` 已加进 .gitignore）。
  - `scripts/voice-train/`：`wad.py`（只读 WAD）、`extract_chiaki.py`、`transcribe.py`（Whisper 识别 → 清单）、`train.py`（无界面训练，照 webui.py）、`setup.sh` / `train.sh`（服务器上）、`pack.py`（打包）、`README.md`（给用户的一步步说明）。
  - GPT-SoVITS 源码（b4570b8）已放进 `chiaki-voice/GPT-SoVITS/`，训练用的预训练模型已下到本机（打包时不带）。
- 进行中：本机测试环境（临时文件夹里的 Python 3.11 venv，装 PyTorch 2.9.1 cu126 + GPT-SoVITS 依赖）和 Whisper large-v3 模型在下载；下载完 → 跑 `transcribe.py` 生成 `chiaki.list` 并抽查 → 选参考音频放 `chiaki-voice/ref/` → 本机 4060 上 `train.py --quick` 冒烟 → `pack.py` 打包 → 告诉用户命令。
- 下一步：用户把模型拿回来后，在本机配 GPT-SoVITS 推理服务（api_v2.py，127.0.0.1:9880）接进 DeskMate；做 ROADMAP v2.2 的「声音语言」（日语输出）。
- 本次决策：DECISIONS #56

## 2026-10-29 11:56
- 背景：用户试用 0.3.1 后反馈——① 代码画的 Q 版认不出是千秋（身体太大太胖、脸和眼睛不像、没有发卡、不可爱），想自己生图，要 prompt；② 语音完全不像、也不是日语，想要中文 / 日语两种输出，问能不能用自己的 4090 服务器训练声音、还能不能保持轻量。用户要求东京时间 12:20 前停下并保存进度，这次只做文档和规划，没有改代码。
- 完成：
  - `docs/art/CHIAKI_PROMPTS.md`：生图工具推荐（4090 + ComfyUI + 认识千秋的动漫底模最好）、整套图的统一要求（2~2.5 头身、不要胖、构图一致、透明背景、640×720）、标签版 / 英文自然语言版 / 中文版 prompt、负面词、22+1 张图的文件名 → 动作对照表、放进 DeskMate 的步骤。用户放的 PNG 本来就会替换内置 SVG，不用改代码。
  - `docs/voice/SOVITS_TRAINING.md`：用自己的 4090 服务器训练千秋声音的步骤（取原声 → GPT-SoVITS 网页界面切分 / 识别 / 校对 / 训练 → 服务器上 `api_v2.py` 只监听本机 → 用户电脑 `ssh -L 9880:127.0.0.1:9880` 转发 → 现在的 DeskMate 直接能连）。
  - ROADMAP 新增 v2.2（声音语言中文 / 日语、GPT-SoVITS 连局域网服务器、训练指南、用户出图后的调整）；NEEDS_USER #12（生图）、#13（训练声音）。
- 声音不像的原因（写给下一个接手的人）：默认的 Fish Audio 声音是别人上传的社区模型，拿日语素材克隆的声音去念中文，音色和口音都会跑；GPT-SoVITS 的 `text_lang` 写死 `zh`（`voice.rs` 约 541 行）；系统语音本来就不像。
- 进行中：无（工作区干净）。
- 下一步：按 ROADMAP v2.2 顺序——先做「声音语言」（日语 = 气泡中文、声音日语），再做 GPT-SoVITS 局域网地址；用户把图放进来后看效果调整。做完改版本号 0.3.2、跑 `npm run check` 和端到端、发布。
- 已知问题：同上一条。
- 本次决策：无新编号（v2.2 的方案写在 ROADMAP 里，动手实现时再记进 DECISIONS）。

## 2026-10-29 11:20
- 完成：
  - **0.3.1 千秋的 Q 版形象**（DECISIONS #55）：用户觉得游戏立绘太少，想按特征自己生成 Q 版形象和对应动作。`scripts/chibi/chiaki.mjs` 用代码画原创 SVG，22 张（状态 6、待机变体 5、反应 11），在 `POSES` 里加一行就多一个动作；内置整套形象，反应时短暂换图，勿扰时戴兜帽。端到端加了桌宠截图检查。
  - **CI 修复**：v0.3.0 的 Release 在 GitHub 上测试失败没有发布。GitHub 不给非管理员看日志，改了 CI：测试失败时把报错写成公开可读的注解。查到是 `abilities::tests::read_dir_validation`：CI 临时文件夹是 8.3 短名，数据目录没换成完整路径就比较，会被放行——这是真实的小漏洞，修了（读取文件夹、任务文件夹两处，见 SECURITY）。
  - 本机端到端 51/51（第一次跑时刚编译完机器忙，出现过一次 CDP 超时，重跑通过）。
- **发布**：CI 变绿，`v0.3.1` 已发布（https://github.com/koara666/DeskMate/releases/tag/v0.3.1 ，安装包 3.54 MiB + SHA256SUMS.txt，GitHub 也给附件算了 digest）。`v0.3.0` 标签留着，但没有对应的 Release（当时测试没过）。
- 注意：GitHub 公开接口每个网络每小时只能查 60 次。不要高频轮询 Actions 状态（这次轮询太勤，把用户这个网络的额度用完了一小时），查发布结果每 1~2 分钟一次就够；「检查更新」遇到限流现在会提示过一会儿再试（下个版本带上）。
- 下一步：用户在 0.3.0 里点「设置 → 关于 → 检查更新」试一次自动更新到 0.3.1；想出声就按聊天里给的步骤拿 Fish Audio 的 Key；想要新的 Q 版动作就在 `scripts/chibi/chiaki.mjs` 的 `POSES` 里加。
- 本次决策：DECISIONS #54、#55

## 2026-10-29 10:35
- 完成：用户退出全屏后补跑——**端到端 51/51 通过**（新增的聊天开游戏、方块对战、摸头只弹气泡、右键气泡菜单、声音页、千秋皮肤都过了；桌宠页面能拿到 10 个系统声音）。
- 实测（release 0.3.0，和 0.2.0 对比）：安装包 **3.54 MiB**（3.41）；冷启动 **0.56~0.64 秒**（0.62）；空闲 CPU 整机 **0.018~0.045%**（0.044~0.052）；空闲私有内存 **134~137 MB**（125，多出来的约 10 MB 主要是启动后预先建好的气泡菜单窗口）。都在目标内。
- 发布：用户授权「每次更新版本之后都直接发布」（DECISIONS #54，NEEDS_USER #10 已完成）；gitleaks 扫全部历史没有密钥，推送代码和 `v0.3.0` 标签。
- 下一步：用户要「根据特征自己生成 Q 版形象，按需求生成对应动作」——自己画千秋的 Q 版 SVG 一整套（待机、说话、开心、摸头、晕、睡觉、躲起来……），并做「反应 → 图片」。

## 2026-10-29 05:10
- 背景：用户试用 0.2.0 后给了 7 条反馈（看屏幕的隐私和主动看、检查更新、聊天驱动和鼠标交互、贴图、角色专属界面、语音、和千秋对战的方块游戏）。按用户给的顺序排成 ROADMAP 的 P9~P16（v2.1 / 0.3.0），全部做完（端到端和性能实测除外，见下）。
- 完成：
  - **P9 看屏幕的隐私护栏**（DECISIONS #47）：聊天 / 邮件 / 支付证券 / 远程桌面 / 系统设置不看，标题有登录 / 密码 / 邮箱 / 支付 / 订单 / 账户 / 身份证等不看，焦点在密码框不看（UI Automation，本机实测 31 毫秒）；只截前台那一个窗口；发送前用 Windows 自带的文字识别（本机可用）涂黑邮箱、手机号、卡号、金额、密钥和带「密码」的行，主动看遇到高风险就不看；气泡下注明「看了 XX · 遮住 N 处」；设置里能预览、能看本次运行的记录。打开后玩游戏时默认主动看（不用再一个个勾）；JPEG、先说「我看看」更快。
  - **P10 检查更新**（#48）：启动 1 分钟后查 GitHub Releases（每天一次、可关）；更新卡片；只下载查到的安装包、校验大小和 SHA-256、安静安装后重启；发布流程 `.github/workflows/release.yml`（推送 v* 标签触发）。AGENTS.md「不做自动更新」改为「不做静默的自动更新」。
  - **P11 对话驱动 + 鼠标交互**（#49）：聊天里说「来一局井字棋 / 别打扰我 / 你先躲一下 / 看看我的屏幕 / 我有哪些提醒 / 取消买菜的提醒」直接做，模型 `[[玩：]]` `[[提醒：]]` 兜底；「叫我 / 喊我」带时间也算提醒；「她说的话」气泡（没有输入框，点一下变聊天框）；点头 / 蹭头 = 摸头，拖动不弹聊天框，按住不放、连戳、甩晕、搬家；右键（可按住）头顶气泡菜单（新窗口 `menu`）；一堆彩蛋台词。
  - **P12 贴图**（#50）：网上 Wiki 有反爬验证不绕过，改从用户自己的《超级弹丸论破 2》游戏文件取出千秋的半身 24 张、全身 25 张，放在 `stickers/`（不进仓库），附 `stickers/说明.md`（每张的表情和建议用途）。
  - **P13 角色专属界面**（#51）：角色包 `theme`（皮肤白名单 + 颜色白名单）；千秋的掌机 / 像素风皮肤（聊天框、对话框式气泡、头顶菜单、深色模式）；小游戏比分牌、结果横幅、棋子颜色。
  - **P14 语音**（#52）：Fish Audio（有现成的千秋声音、默认免费档）/ Windows 自带语音 / 本机 GPT-SoVITS / OpenAI 兼容；只读精简后的一两句；分场景开关、勿扰和全屏规则、缓存、每天字数上限；设置「声音」页；NEEDS_USER #11 写了怎么拿 Fish 的 Key。
  - **P15 方块对战**（#53）：原创的下落方块对战（8×16、8 种原创方块、塞垃圾行、限时 / 生存、三档电脑对手），不是 Tetris 的复刻。
  - **P16**：升级后她介绍一次新功能；版本 0.3.0；`docs/release-notes/v0.3.0.md`；README 0.3.0；端到端测试新增 8 项（聊天开游戏、方块对战、摸头气泡、右键菜单、声音页、千秋皮肤……）。
  - 测试：Rust 316（+7 个默认忽略的本机冒烟）、前端 93，`npm run check` 通过。
- 进行中 / 没做：
  - **端到端测试和性能实测没跑**：用户整段时间都在全屏（`SHQueryUserNotificationState` = 2），按规定不启动可见窗口。用户退出全屏后跑：`npx tauri build --no-bundle && npm run e2e`，`scripts/measure.ps1`。新的鼠标交互（尤其是按住右键选气泡）只有单元测试，真实鼠标手感要用户试。
  - 0.3.0 安装包：`src-tauri/target/release/bundle/nsis/DeskMate_0.3.0_x64-setup.exe`，**3.54 MiB**（0.2.0 是 3.41 MiB；主程序 12.2 MiB，0.2.0 是 11.6 MiB，多出来的是语音、更新、文字识别和菜单窗口的代码）。都在目标内（< 15 MB）。内存、CPU、启动时间要等用户退出全屏后用 `scripts/measure.ps1` 实测。
- 下一步：
  1. 用户退出全屏后补跑端到端和实测，记进这里；有失败先修。
  2. 用户试用 0.3.0（换装前先退出旧版）；想出声就按 NEEDS_USER #11 拿 Fish Audio 的 Key。
  3. 用户确认后发布第一个 GitHub Release（NEEDS_USER #10，推送 `v0.3.0` 标签；仓库里还有 40 多个提交没推送）——发布以后「检查更新」才有东西可查。
  4. 用户挑好贴图后，按 `stickers/说明.md` 把摸头、甩晕、躲起来等反应对应上不同的图（要加「反应 → 图片」的代码）。
  5. 可选：从游戏里取千秋的日语原声做 GPT-SoVITS 参考音频 / Fish 私有声音。
- 已知问题：
  - 看屏幕的文字识别认不出的字（很小、艺术字）不会被涂黑；新的聊天软件可能不在名单里（用户可以加进隐私名单）。
  - Fish Audio 的社区声音模型是别人上传的，可能被下架；免费合成模型高峰期可能排队。
  - 按住右键选气泡靠后端读鼠标和右键状态，多显示器不同缩放下的坐标只在单元测试里验证过。
- 本次决策：见 DECISIONS.md #47~#53

## 2026-10-29 00:20
- 完成：**v2 的 P0~P8 全部完成**（ROADMAP 已勾选）。本次在 23:55 那条之后：
  - **P6 需要模型的小游戏**（DECISIONS #44）：成语接龙、海龟汤（6 道原创题，汤底只在后端）、二十问；模型只输出 JSON、规则在本地检查。
  - **P7 看一眼屏幕**（DECISIONS #45，默认关闭）：快捷键 Alt+Shift+L、允许的游戏偶尔看；截图只在内存；三种模型接口带图片；本机截屏冒烟通过（248 KB PNG、70 毫秒）。
  - **P8 打磨**（DECISIONS #46）：说话 / 思考动画改成离散帧；README 0.2.0 功能总览；NEEDS_USER #8 试用、#9 真实模型验证；安全总复查。
  - 修了一个跨午夜就失败的测试（播报的「今天」按早上 5 点分天）。
  - 测试：Rust 289（+4 个默认忽略的真实接口 / 本机冒烟）、前端 70、**端到端 44/44**。
- 实测（最终 release 0.2.0，和 V1_AUDIT 对比）：安装包 **3.41 MiB**（v1 2.95）；主程序 11.6 MiB（v1 10.0）；冷启动 **0.62 秒**（v1 0.57~0.77）；空闲 CPU 整机 **0.044~0.052%**（v1 0.026~0.044，都在 0.05% 左右的测量噪声内）；空闲私有内存 **124.5~125 MB**（v1 117.5~119.5）。都在目标内（< 15 MB、≈ 0% CPU、< 2 秒）。前端：公共运行时 34 KB、聊天 14 KB、设置 59 KB，小游戏和任务卡片按需加载。
- 进行中：无。
- 下一步：
  1. 用户试用 0.2.0（NEEDS_USER #8；安装包 `src-tauri/target/release/bundle/nsis/DeskMate_0.2.0_x64-setup.exe`，换装前要先退出正在运行的旧版），按反馈调整台词、频率、界面。
  2. 用户愿意的话用真实模型验证模型版播报、问天气、成语接龙等（NEEDS_USER #9，花一点钱）。
  3. 每年 11 月更新节假日数据（HOW_TO_ADD 5.1）；2027 年的中国放假安排公布后补上。
  4. 以后要给别的角色补内容：按 HOW_TO_ADD 1.6 写 `lines/`，不用改代码。
- 已知问题：
  - 真实模型下的播报、小游戏、看画面没有实测（按规定不花用户的钱），台词和提示词的实际效果要靠用户反馈。
  - 全屏游戏时第三层只能用快捷键看（偶尔看画面在全屏时不看）。
  - 内存比 v1 多约 6 MB。
- 本次决策：见 DECISIONS.md #44~#46

## 2026-10-28 23:55
- 完成：
  - **P4 角色感**（DECISIONS #41）：心情、亲密度（只增不减）、摸头 / 点身体 / 拖动、随机小事件（含选项）、日记（迁移 v8）、聊天里记下生日纪念日、待机图片变体、健康边界写进基本规则；七海千秋的关系称号、日记风格、游戏性格和台词。
  - **P5 小游戏**（DECISIONS #42）：前端注册表框架、猜拳、井字棋（三档难度）、21 点、每日抽签；战绩（迁移 v9），影响心情亲密度并写进日记；🎮 和右键「一起玩…」。
  - **修复**（DECISIONS #43）：窗口显示 / 隐藏同时走系统调用，解决「播报弹出后聊天窗口关不掉」（v1 就潜伏的问题）。
  - 用户退出全屏后补跑：**端到端 44/44 通过**（新增提醒、播报、小游戏、日记页、活动感知页等）。
  - 测试：Rust 281（+3 忽略）、前端 67。
- 实测（release，和 V1_AUDIT 对比）：安装包 3.33 MiB（v1 2.95）；主程序 11.3 MiB（v1 10.0）；冷启动 0.52~0.58 秒（v1 0.57~0.77）；空闲 CPU 整机 0.023~0.034%（v1 0.026~0.044%）；空闲私有内存 124~127 MB（v1 117.5~119.5，+约 6 MB，主要是聊天和设置页的脚本变大）。打开活动感知和全部主动行为（`DESKMATE_TEST_PROACTIVE=1`）时空闲 CPU 0.043%，DeskMate 主进程 40 秒只用了 16 毫秒。都在目标内。
- 进行中：无。
- 下一步：P6（成语接龙、海龟汤、二十问——需要模型），P7（截屏，默认关），P8 打磨。
- 已知问题：内存比 v1 多约 6 MB，P8 看能不能把设置页按页面拆分加载。
- 本次决策：见 DECISIONS.md #41~#43

## 2026-10-28 23:20
- 完成：
  - **P2 每日播报**（DECISIONS #39）：每天第一次 / 自定义时间点（晚上的是睡前问候）；天气（Open-Meteo，内置 135 个常用城市中文名表；聊天里问天气直接查，取代 #32 的任务流程）、节假日（中国 2026 官方安排 + 日本 + 美国规则 + 常见节日）、吃饭（只说菜式）、今天的提醒、RSS 新闻（默认关）、生日纪念日和角色生日；随机组合和顺序；有预算时一次模型，否则模板 + 台词池。真实天气接口冒烟通过。版本 0.2.0，安装包 3.21 MiB（`src-tauri/target/release/bundle/nsis/DeskMate_0.2.0_x64-setup.exe`）。
  - **P3 活动感知 + 日常关怀**（DECISIONS #40）：前台切换事件 + 30 秒采样、本地分类、隐私名单回避、用户规则、状态变化评论、活动日志（迁移 v7）、活动触发的提醒、久坐 / 喝水 / 深夜 / 吃饭关怀。本机冒烟：正在玩的 Steam 游戏被识别为「玩游戏（Slay the Spire 2）」。
  - 测试实例默认不做自动主动行为（播报、活动感知、关怀），`DESKMATE_TEST_PROACTIVE=1` 打开。
  - 测试：Rust 269（+3 个默认忽略的冒烟 / 体检测试）、前端 54，`npm run check` 通过。
- 进行中：P4 角色感（还没开始写代码）。
- 未做 / 延后：0.2.0 和 P3 的端到端界面测试、性能实测——用户一直在全屏玩游戏（SHQueryUserNotificationState = 2），按规定不启动可见窗口；用户退出全屏后补跑（`npx tauri build --no-bundle && npm run e2e`，`scripts/measure.ps1`）。
- 下一步：P4（心情、亲密度、触摸、随机小事件、日记、特殊日期从聊天里记下、天气季节换图），然后 P5 小游戏。
- 本次决策：见 DECISIONS.md #39、#40

## 2026-10-28 21:00
- 背景：用户决定 v2 转向「日常陪伴」（`docs/briefs/DIRECTION_V2.md`），**角色内容只做七海千秋**，全程由 agent 自主开发，用户只验收（DECISIONS #33、#34）。
- 完成：
  - **方向调整文档**：方向说明移到 `docs/briefs/`；`docs/V1_AUDIT.md`（v1 模块去留 + 实测基线）；DECISIONS #33~#38；ROADMAP 重写为 P0~P8；AGENTS.md 定位与优先级（工作规则没动）、ARCHITECTURE、HOW_TO_ADD、SECURITY、README 同步。
  - **P0**：委托外部 agent 降为默认关闭的高级选项（去掉聊天框 🛠，任务卡片 / 任务设置按需加载，提示词只在开启时带任务规则，8 个角色设定去掉「帮你干活」的定位）；内置能力层（`[[打开：…]]` / `[[读取：…]]` → 气泡按钮，用户点了才做，后端两次检查；电量）；主动行为调度器（优先级、冷却、勿扰 / 全屏 / 安静时段、被忽略降频、无人理会自动收起气泡）；主动行为每日预算；设置新增「陪伴」「高级」页。
  - **P1 提醒**：聊天里吩咐 → 本地中文时间解析（相对时间、某天某时、每天 / 工作日 / 每周 / 每月、开机时、开始玩游戏时），不行再用一次模型 → 角色复述 → 点「好」保存；到时台词创建时定好；定时器 + 关机错过的补提醒；全屏 / 重要提醒发系统通知（tauri-plugin-notification）；设置「提醒」页、右键菜单「提醒…」。
  - **台词池** `lines.rs`（`lines/*.md`，条件 + 占位符 + 通用台词兜底 + 体检测试），七海千秋的提醒台词。
  - 测试：Rust 230（+1 忽略）、前端 52、端到端 40 项全部通过。
- 实测（release，`scripts/measure.ps1`，和 V1_AUDIT 对比）：安装包 3.12 MiB（v1 2.95，+0.17 主要是通知插件）；主程序 10.5 MiB（v1 10.0）；冷启动 0.59~0.64 秒（v1 0.57~0.77）；空闲 CPU 整机 0.026~0.039%（v1 0.026~0.044%）；空闲私有内存 117.7~118.8 MB（v1 117.5~119.5）。没有变差。
- 进行中：P2 每日播报——已下载并核对节假日数据（`src-tauri/data/holidays.tsv`，中国 2026 官方安排 + 日本内阁府 2026~2027），查了 Open-Meteo 文档和条款（免费、非商用、需署名 CC BY 4.0；地名搜索只认英文，中文要靠内置城市表）。代码还没写。
- 下一步：P2（天气、节假日、吃饭建议、RSS、特殊日期、随机组合、离线兜底、聊天里问天气直接回答），然后 P3 活动感知。
- 已知问题：版本号还是 0.1.6，交付给用户试用前改成 0.2.0；e2e 里「气泡贴合内容」偶尔超时（单独复现正常，第二次全量通过）。
- 本次决策：见 DECISIONS.md #33~#38

> v1 时期（2026-10-27 ~ 10-28 19:30）的条目已移到 `docs/progress-archive/2026-10-v1.md`。
