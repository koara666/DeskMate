# 技术决策记录

> 格式：编号、日期、决定、原因、放弃的备选。新的追加在最后。

## #1 2026-10-27 前端用 Preact + TypeScript + Vite
- 决定：前端框架选 Preact（约 4 KB），构建用 Vite，测试用 Vitest + happy-dom + @testing-library/preact。
- 原因：设置界面表单很多（后端、角色、记忆管理、用量、任务权限），纯 TS 手写 DOM 维护成本高；Preact 体积接近原生，JSX 默认转义文本，天然防止模型输出注入 HTML。
- 放弃：原生 TS（设置界面代码量大、易出 XSS）；Svelte（编译器链更重，团队/agent 熟悉度不如 Preact）。

## #2 2026-10-27 三个独立窗口，而不是一个大透明窗口
- 决定：pet（只有角色）、chat（气泡/面板）、settings（按需创建）三个窗口。
- 原因：Windows 上透明区域也会拦截鼠标点击。一个大透明窗口会挡住桌面；若用轮询鼠标位置切换穿透，会让空闲 CPU 上涨。分窗口后每个窗口只占实际内容大小。
- 放弃：单窗口 + `set_ignore_cursor_events` 轮询（CPU 成本）；单窗口动态改尺寸（Windows 上先改尺寸再改位置会闪一帧）。

## #3 2026-10-27 开发工具链安装在 D 盘
- 决定：Rust 装在 `D:\dev`（`CARGO_HOME=D:\dev\cargo`、`RUSTUP_HOME=D:\dev\rustup`，已写入用户环境变量），gitleaks 放在 `D:\dev\tools`。
- 原因：C 盘剩余空间不足 9 GB；用户目录含中文字符，部分构建工具对非 ASCII 路径不友好。
- 放弃：默认安装到 `%USERPROFILE%\.cargo`。

## #4 2026-10-27 中文全文检索：自己做「单字 + 二元」切分再交给 FTS5
- 决定：写入 FTS5 前在 Rust 里把中日韩字符切成单字和相邻二字组合（「喜欢猫」→「喜 欢 猫 喜欢 欢猫」），查询时同样处理；拉丁字母按单词。FTS5 用默认 unicode61 分词器。
- 原因：unicode61 不会切分中文（整句会变成一个词，几乎搜不到）；trigram 分词器要求查询至少 3 个字，中文常见词是 2 个字；jieba 等分词库要带几 MB 词典，违反轻量目标。
- 放弃：trigram 分词器、jieba-rs、嵌入向量（需求明确第一版不做）。

## #5 2026-10-27 设置存在 SQLite 的 kv 表里
- 决定：所有设置（不含密钥）以 JSON 存在 `deskmate.db` 的 `settings` 表。
- 原因：需求要求单个 SQLite 文件；设置随数据库一起备份和迁移。
- 放弃：单独的 settings.json 文件。

## #6 2026-10-27 用户图片通过 IPC 读成 data URL，不开 asset 协议
- 决定：Rust 读取用户数据目录里的角色图片（校验扩展名、文件头、大小 ≤ 5 MB），以 data URL 返回给前端。
- 原因：不需要给 WebView 开放文件系统访问（asset protocol scope），CSP 只需允许 `img-src 'self' data: blob:`，攻击面更小。
- 放弃：Tauri asset 协议 + `convertFileSrc`。

## #7 2026-10-27 待机动画用低频触发的一次性 CSS 动画
- 决定：每隔几秒（带随机抖动）给角色加一个类，播放一次约 0.6 秒的呼吸/眨眼动画，播完移除；其余时间没有任何动画在跑。设置里可以完全关闭。
- 原因：持续运行的 CSS 动画（即使只动 transform）也会让 Chromium 每帧合成，空闲 CPU/GPU 不为 0。
- 放弃：`animation: infinite` 的呼吸动画；requestAnimationFrame 循环。

## #8 2026-10-27 关闭 Tauri 的 freezePrototype
- 决定：`tauri.conf.json` 里 `freezePrototype` 设为 false。
- 原因：开启后 `Object.prototype` 被冻结，Preact 创建虚拟节点时给对象赋值 `constructor` 会抛错，界面直接白屏（实测）。防 XSS 的主要防线是严格 CSP（禁止内联脚本和 eval）+ 模型输出只按纯文本渲染，冻结原型只是额外加固。
- 放弃：换用不给 constructor 赋值的框架（成本高、收益小）。

## #9 2026-10-27 应用命令按窗口最小授权
- 决定：`build.rs` 用 `AppManifest::commands` 登记全部命令，每个窗口的 capability（`capabilities/pet.json`、`chat.json`、`settings.json`）只授权它需要的命令；前端事件只开放 listen/unlisten，不开放 emit。
- 原因：需求要求 capabilities 最小化；即使某个窗口被注入脚本，也调用不了别的窗口的命令。
- 放弃：所有窗口共用一个 capability（简单但权限过大）。

## #10 2026-10-27 自写轮换文件日志 + 脱敏，不用 tauri-plugin-log
- 决定：`logging.rs` 实现一个约 100 行的文件日志：单文件 1 MB、保留 3 份，每行输出前经过 `redact::redact_secrets`。
- 原因：插件会额外引入依赖和前端可调用的日志接口；我们只需要后端写日志。脱敏是兜底：正常代码路径本来就不记录 Key、请求头、请求体和对话内容。
- 放弃：tauri-plugin-log。

## #11 2026-10-27 拖动结束判定：去抖 + 检查鼠标左键是否仍按住
- 决定：桌宠窗口每次 Moved 事件后 250 ms 内没有新的移动、且鼠标左键已松开（Windows 上用 GetAsyncKeyState），才执行吸附和保存位置。
- 原因：原生拖动期间前端收不到 mouseup；只靠去抖的话，用户拖到一半停住不动时会被提前吸附。
