# 让千秋出声

DeskMate 里七海千秋可以出声说话（默认关闭，在「设置 → 声音」里打开）。声音有两档：

| | 预录语音 | 完整声音（GPT-SoVITS） |
|---|---|---|
| 要装什么 | 什么都不用装，安装包自带 | 要在自己电脑上装 GPT-SoVITS 和千秋的模型 |
| 她能念什么 | 事先写好的台词：开机问候、天气和今天的播报、摸头和戳一下、提醒、关心你休息、小游戏里的话 | 每一句，包括聊天里现场说的话 |
| 费用 | 免费、不联网 | 免费、不联网（有 NVIDIA 显卡最好） |

另外还能选 Fish Audio（云端）、Windows 自带语音、OpenAI 兼容接口，见设置页上的说明。

## 预录语音（推荐先用这个）

1. 右键她 →「设置…」→「声音」。
2. 「用什么声音」选**预录语音**（新装默认就是它），勾上**让她出声**，点「试听一句」。
3. 她说的是日语，气泡里照常显示中文，像看番的字幕。

聊天里她现场想的话、需要联网查的内容没有录音，所以不会出声；想要它们也出声，用下面的完整声音。

## 完整声音（GPT-SoVITS）

声音是用《超级弹丸论破 2》里七海千秋的日语原声训练出来的模型，在你自己的电脑上合成，每一句都能念。模型不在安装包里，单独放在发布页：

- 发布页：https://github.com/koara666/DeskMate/releases/tag/voice-chiaki-v1
- 文件：`chiaki-model.tar.gz`，约 306 MB
- SHA-256：`630af666d19d4521c834bbd6cb10ec8dc30a0d8e85e331a7ef18953a4fa14256`（下载后核对）

压缩包里有：

```
chiaki-model/
  chiaki_e12_s720.pth   SoVITS 模型（音色），v2ProPlus
  chiaki-e15.ckpt       GPT 模型（语气和节奏）
  ref/                  4 段参考音频（ref_game / ref_calm / ref_warm / ref_gentle.wav）
  ref/refs.txt          每段参考音频说的日语原文（「参考音频说的话」要一字不差地填）
```

### 步骤

1. 按 GPT-SoVITS 官方说明装好它（版本 v2ProPlus，需要它的预训练模型）：https://github.com/RVC-Boss/GPT-SoVITS 。这一步不是 DeskMate 的一部分。
2. 下载 `chiaki-model.tar.gz`，核对 SHA-256，解压。
3. 让 GPT-SoVITS 的 `api_v2.py` 加载这两个模型：配置文件 `tts_infer.yaml` 的 `custom` 段写
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
4. 运行 `python api_v2.py -a 127.0.0.1 -p 9880 -c <配置文件>`（只监听本机）。第一句话要等模型加载，大约 20 秒。
5. DeskMate「设置 → 声音」：引擎选 **GPT-SoVITS**，接口地址 `http://127.0.0.1:9880`；「参考音频」填 `ref/ref_game.wav` 的完整路径；「参考音频说的话」照 `ref/refs.txt` 里 ref_game 那一行填；「参考音频的语言」选日语；勾上「让她出声」，点「试听一句」。

## 版权

声音来自游戏《超级弹丸论破 2》，角色和声音的权利属于原作的版权方。模型和预录语音是为个人学习、自己使用 DeskMate 做的，**不用于商业用途**，也不要拿它冒充配音演员本人。版权方或配音演员认为不应公开时，可以联系仓库所有者，发布页上的文件会立刻删除。
