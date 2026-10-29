# 千秋 Q 版形象：生图 prompt 和放图方法

> 用户 2026-10-29 反馈：0.3.1 里用代码画的 Q 版（DECISIONS #55）身体太大太胖、脸和眼睛不像、没有发卡、不可爱，认不出是千秋。
> 改为**用户自己生图**，程序这边已经支持：用户放的 PNG 优先于内置 SVG，同名替换，缺哪张就用内置的那张。
> 生出来的图只放在用户自己电脑上，**不要提交进仓库**（公开仓库，见 AGENTS.md 第 6 节）。

## 一、用什么生图（按推荐顺序）

1. **你的 4090 + ComfyUI（或 WebUI）+ 动漫底模**（最推荐）：Illustrious-XL / NoobAI-XL / Animagine XL 4 这类模型是用 Danbooru 图库训练的，**本来就认识七海千秋**，写 `nanami chiaki` 就能出她的发型、发卡、外套，一眼能认出来。免费、可以批量、可以固定种子保证一套图风格一致。
2. NovelAI（付费）：同样认识角色名，效果接近第 1 种。
3. GPT 生图 / Nano Banana / 即梦 / Midjourney：不一定认识角色名，**一定要上传一张千秋的官方图做参考**，再用下面的「自然语言版」prompt。

## 二、整套图的统一要求（每张都一样，保证换图时不跳）

- **构图**：全身，正面略侧，人物居中，脚底在同一高度，头顶留一点空。每张人物大小一致。
- **比例**：2~2.5 头身（大头、小而纤细的身体、短手短腿），**不要胖、不要圆滚滚**。
- **背景**：纯白或纯绿背景，生成后抠成透明（ComfyUI 用 BiRefNet / RMBG 节点，或 rembg；也可以直接用 LayerDiffuse 出透明图）。
- **尺寸**：生成时 1024×1152（宽:高 = 8:9），抠图后缩到 **640×720** 的 PNG（程序推荐 320×360，给 2 倍方便高分屏），单张不超过 5 MB。
- **一致性**：先只生 `idle`（待机），挑一张最满意的；之后每张都用它做参考（IP-Adapter / img2img 重绘 0.5~0.6 / Midjourney `--cref` / GPT「同一个角色，只改表情和动作」），并固定同一个种子和底模。

## 三、基础 prompt

### 3.1 标签版（Stable Diffusion / NovelAI，复制后在末尾接第四节每张图的「动作」）

```
masterpiece, best quality, chibi, super deformed, 2.5 heads tall, full body, standing, centered, simple background, white background,
nanami chiaki, danganronpa 2,
1girl, solo, short hair, wavy bob, light pink hair, flipped hair ends, blunt bangs, pink eyes, half-closed eyes, sleepy eyes, big eyes, long eyelashes,
spaceship hair ornament, hairclip,
white collared shirt, pink ribbon, dark teal hooded jacket, cat ear hood, hood down, pink backpack straps,
dark pleated skirt, thighhighs,
holding handheld game console,
small body, slender, cute, soft smile, blush,
flat color, clean lineart, thick outline, cel shading, sticker style
```

负面：

```
fat, chubby, plump, big body, long body, realistic, 3d, extra fingers, extra limbs, deformed hands, multiple girls, text, watermark, signature, frame, cropped, cut off feet, background scenery
```

- 想让她更像，可以在 `nanami chiaki` 前后加权：`(nanami chiaki:1.2)`。
- 如果发卡没出来：`(spaceship hair ornament:1.3)`；颜色不对就加颜色词，以官方图为准。
- 如果身体还是偏大：加 `(chibi:1.2), (small body:1.2)`，负面里 `fat` 加权到 1.3。

### 3.2 自然语言版（GPT 生图 / Nano Banana / 即梦 / Midjourney，**先上传一张千秋官方图做参考**）

```
A cute chibi sticker of Chiaki Nanami from Danganronpa 2 (use the reference image for her design).
2.5-heads-tall super-deformed proportions: big head, small slender body, short limbs — not chubby.
Short wavy light-pink bob with slightly flipped ends and blunt bangs, the small pixel-art spaceship hairclip on her bangs,
big pink eyes that are half-lidded and sleepy, long lashes, soft blush.
White collared shirt with a pink ribbon, dark teal hooded jacket with cat-ear hood (hood down), pink backpack straps, dark pleated skirt.
Holding a handheld game console. Full body, standing, centered, plain white background,
clean thick outlines, flat cel shading, anime sticker style, consistent character design.
Pose and expression: <把第四节的「动作」换成英文写在这里>
```

中文版（即梦等中文模型）：

```
七海千秋（弹丸论破2）的Q版贴纸，参考上传的官方图。2.5头身，大头、小而纤细的身体、短手短腿，不要胖。
浅粉色微卷波波头，发梢外翘，齐刘海，刘海上别着像素风小飞船发卡；粉色的大眼睛，眼皮半垂、总是有点困，长睫毛，淡淡腮红。
白色衬衫配粉色领结，深青色猫耳兜帽外套（兜帽放下），粉色书包带，深色百褶裙，手里拿着掌机。
全身、站立、居中、纯白背景，干净的粗描边、平涂、日系动画贴纸风格，整套角色设计保持一致。
动作和表情：<第四节的「动作」>
```

## 四、22 张图：文件名 → 动作

先做前 6 张（最常看到），其余慢慢补；缺的那张会继续用内置的。

| 文件名 | 什么时候用 | 动作（标签版） | 动作（中文） |
|---|---|---|---|
| `idle.png` | 待机（最重要） | holding game console, looking at viewer, sleepy, slight smile | 抱着掌机看向你，有点困，浅浅的笑 |
| `talking.png` | 说话 | open mouth, talking, looking at viewer, one hand raised | 张嘴说话，看着你，一只手抬起来 |
| `thinking.png` | 想事情 | finger to chin, looking up, closed mouth, thinking | 手指抵着下巴，眼睛往上看，在想 |
| `happy.png` | 开心 | closed eyes, big smile, happy, sparkles | 眯眼大笑，开心，周围小星星 |
| `sad.png` | 难过 | sad, frown, teary eyes, looking down | 难过，皱眉，眼里有泪，低头 |
| `sleep.png` | 睡觉 | sleeping, closed eyes, sitting, hugging game console, zzz, snot bubble | 抱着掌机坐着睡着，Zzz，鼻涕泡 |
| `idle_good.png` | 心情好 | smile, swaying, musical note | 心情好，笑着轻轻摇，小音符 |
| `idle_low.png` | 心情差 | expressionless, droopy eyes, slouching | 没精神，眼皮耷拉，有点驼背 |
| `idle_night.png` | 夜里 | very sleepy, yawning, rubbing eye | 很困，打哈欠，揉眼睛 |
| `idle_rain.png` | 下雨天 | hood up, cat ear hood, holding game console | 戴上猫耳兜帽，抱着掌机 |
| `idle_winter.png` | 冬天 | hood up, scarf, blush, cold | 戴兜帽、围巾，脸冻红 |
| `idle_summer.png` | 夏天（可选） | jacket off shoulders, fanning herself, sweat drop | 外套滑到肩下，扇风，小汗滴 |
| `pat.png` | 被摸头 | closed eyes, happy, blush, headpat, leaning head | 被摸头，眯眼，脸红，头歪过去 |
| `shy.png` | 害羞 | covering face with hands, blush, embarrassed | 双手捂脸，脸很红 |
| `dizzy.png` | 被甩晕 | swirly eyes, dizzy, @_@, wobbly | 转圈圈眼，晕乎乎 |
| `pout.png` | 鼓脸 | pout, puffed cheeks, annoyed, looking away | 鼓着脸，不高兴，扭头 |
| `surprised.png` | 吃惊 | surprised, wide eyes, open mouth, exclamation mark | 吃惊，睁大眼睛，感叹号 |
| `confused.png` | 疑惑 | head tilt, question mark, confused | 歪头，问号，疑惑 |
| `hood.png` | 躲起来 / 勿扰 | hood up, pulling hood down over eyes, hiding | 拉下兜帽遮住眼睛躲起来 |
| `win.png` | 小游戏她赢了 | raising game console overhead, triumphant, smug smile, v sign | 把掌机举过头顶，得意地比 V |
| `lose.png` | 小游戏她输了 | slumped, defeated, game over, teary, hugging console | 垂头丧气，GAME OVER，抱着掌机 |
| `wink.png` | 眨眼 / 神秘 | wink, finger to lips, playful | 眨一只眼，手指放在嘴前 |
| `wake.png` | 刚睡醒 | just woke up, half-open eyes, messy hair, ahoge, rubbing eye | 刚醒，眼睛半睁，头发乱，揉眼睛 |

## 五、放进 DeskMate

1. 「设置 → 角色」→ 点七海千秋的卡片 →「打开图片文件夹」。
2. 把 PNG 按上表的文件名放进去（例如 `idle.png`）。
3. 点「重新加载」。放了的那几张会替换内置的，没放的继续用内置的。
