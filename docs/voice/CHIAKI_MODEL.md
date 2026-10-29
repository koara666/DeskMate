# 七海千秋的声音模型（GPT-SoVITS）

> **状态：发布页还没上传**（需要仓库所有者操作，步骤见 `docs/NEEDS_USER.md` 第 14 项）。上传后删掉这一行。

用《超级弹丸论破 2》里七海千秋的日语原声训练出来的 GPT-SoVITS 模型，给 DeskMate 的「声音」功能用。模型文件**不放在代码仓库里，也不在安装包里**，单独放在 GitHub 的一个发布页（Release）：

- 发布页标签：`voice-chiaki-v1`（https://github.com/koara666/DeskMate/releases/tag/voice-chiaki-v1）
- 文件：`chiaki-model.tar.gz`，约 306 MB
- SHA-256：`630af666d19d4521c834bbd6cb10ec8dc30a0d8e85e331a7ef18953a4fa14256`（下载后一定要核对）

## 里面有什么

```
chiaki-model/
  chiaki_e12_s720.pth   SoVITS 模型（音色），v2ProPlus，训练 12 轮
  chiaki-e15.ckpt       GPT 模型（语气和节奏），训练 15 轮
  ref/                  4 段参考音频（ref_game / ref_calm / ref_warm / ref_gentle.wav）
  ref/refs.txt          每段参考音频说的日语原文和语气（「参考音频说的话」要一字不差地填）
```

训练数据：从游戏文件里取出的 635 条日语语音（约 51 分钟），用 Whisper large-v3 识别、校对、去掉幻听和太短的。训练过程和踩过的坑见 `scripts/voice-train/README.md` 和 `docs/DECISIONS.md` #56。

## 怎么用

需要先在你自己的电脑上装好 **GPT-SoVITS**（版本 v2ProPlus，要有它的预训练模型；有 NVIDIA 显卡最好）。这一步不是 DeskMate 的一部分，按 GPT-SoVITS 官方说明装。

1. 下载 `chiaki-model.tar.gz`，核对 SHA-256，解压。
2. 让 GPT-SoVITS 的 `api_v2.py` 加载这两个模型：配置文件 `tts_infer.yaml` 里 `custom` 段写
   ```yaml
   custom:
     version: v2ProPlus
     t2s_weights_path: <解压位置>/chiaki-model/chiaki-e15.ckpt
     vits_weights_path: <解压位置>/chiaki-model/chiaki_e12_s720.pth
     bert_base_path: GPT_SoVITS/pretrained_models/chinese-roberta-wwm-ext-large
     cnhuhbert_base_path: GPT_SoVITS/pretrained_models/chinese-hubert-base
     device: cuda
     is_half: true
   ```
   路径要写**绝对路径**——写错时 `api_v2.py` 不会报错，而是悄悄退回底模，听起来就不是她了。启动日志里 `Loading Text2Semantic weights from` 和 `Loading VITS weights from` 后面必须是这两个文件。
   在 DeskMate 的开发环境里，把解压出来的 `chiaki-model` 放到 `chiaki-voice/` 下，再运行 `scripts/voice-train/serve_local.py`（或 `chiaki-voice/start-voice.bat`）就会自动配好，只监听本机 `127.0.0.1:9880`。
3. DeskMate「设置 → 声音」：引擎选 **GPT-SoVITS**，接口地址 `http://127.0.0.1:9880`；「参考音频」填 `ref/ref_game.wav` 的完整路径；「参考音频说的话」照 `ref/refs.txt` 里 ref_game 那一行填；「参考音频的语言」选日语；「她说话的语言」选日语；点「试听一句」。

## 版权与删除

- 声音来自游戏《超级弹丸论破 2》，角色和声音的权利属于原作的版权方。这个模型是为**个人学习和自己使用 DeskMate** 做的，**不用于商业用途**，也不要拿它冒充配音演员本人去做会让人误解的事。
- 如果版权方或配音演员认为不应该公开，可以随时联系仓库所有者，发布页会立刻删除（它只是一个发布页附件，不在代码历史里，删除就彻底没了）。
- 没有放进来的：游戏里取出来的 711 条原始语音和识别文本（只在训练用的电脑上，从不上传）。
