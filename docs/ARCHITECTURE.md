# 架构说明

> 本文件描述「现在的代码是什么样」。设计变化时同步更新。决策理由见 DECISIONS.md。
>
> **v2（日常陪伴，DECISIONS #33~#35）正在实施。** 第 6 节列出 v2 的模块边界和数据流，随里程碑完成逐步落地；
> 第 1~5 节是 v1 已有的部分，v2 继续使用（委托外部 agent 降级为默认关闭的高级选项，见 V1_AUDIT）。

## 1. 总览

```
┌──────────────────────── WebView2（前端，Preact + TS）────────────────────────┐
│  pet 窗口          chat 窗口（气泡 / 完整面板）          settings 窗口（按需创建）  │
│  角色形象+状态      输入框、流式显示、停止/重试/复制        后端、角色、记忆、用量、任务 │
└──────────┬──────────────────────┬──────────────────────────────┬────────────┘
           │ invoke / event        │ invoke + Channel（流式）        │ invoke
┌──────────▼──────────────────────▼──────────────────────────────▼────────────┐
│                         Rust 后端（Tauri 2 commands）                          │
│  commands/  ── 只做参数校验和转发，不写业务逻辑                                    │
│  chat/      ── 一轮对话的编排：组装提示词 → 选后端 → 流式 → 禁词过滤 → 记录用量/记忆   │
│  prompt/    ── 提示词组装（角色设定、自定义要求、记忆、最近对话），token 预算           │
│  backends/  ── 模型后端 trait + 注册表：mock / openai_compat(DeepSeek) / gemini / anthropic / claude_cli(订阅) │
│  character/ ── 角色包加载与校验（内置 + 用户目录），只读                              │
│  filter/    ── 禁词：提示词注入 + 流式输出过滤                                     │
│  memory/    ── SQLite：迁移、短期/摘要/长期事实、FTS5 检索、敏感信息过滤、备份          │
│  usage/     ── 用量统计、价格估算、每日/每月花费上限                                 │
│  secrets/   ── 系统凭据管理器（keyring）                                         │
│  tasks/     ── Claude Code / Codex CLI 执行、工作目录沙盒、确认、超时、日志             │
│  desktop/   ── 窗口定位/吸附/多显示器、托盘、全屏检测、空闲检测                         │
│  settings/  ── 设置读写（存在 SQLite kv 表）                                      │
└───────────────────────────────────────────────────────────────────────────────┘
           │                                  │
   %APPDATA%\DeskMate\                Windows 凭据管理器（API Key）
     deskmate.db（唯一数据文件）
     backups\deskmate-YYYYMMDD.db（7 份）
     characters\<id>\…（用户角色包/图片）
     logs\（轮换）  tasks\<id>.jsonl（任务记录）
```

## 2. 目录结构（当前）

```
AGENTS.md / CLAUDE.md          agent 规则
docs/                          文档（见 AGENTS.md 文档地图）
characters/<id>/               内置角色包（character.json、persona.md、可选 canon.md、assets/placeholder.svg）
characters/_fallback.svg       没有任何图片时的通用占位形象
characters/_common/lines/      通用台词（角色没写的场景用；`_` 开头的目录不是角色）
src-tauri/data/                编进程序的小数据：cities.tsv（常用城市坐标）、holidays.tsv（节假日）
pet.html chat.html settings.html   三个窗口的入口
src/
  pet/Pet.tsx                  桌宠：形象、状态、点击/拖动/右键
  chat/ChatApp.tsx Composer.tsx    聊天：气泡/面板、流式打字机、停止/重试/复制、错误详情、IME
  settings/*.tsx               设置：常规、模型（Key/测试连接）、用量、关于
  lib/ipc.ts                   唯一的 IPC 封装（组件通过接口调用，测试时换成假实现）
  lib/types.ts typewriter.ts clipboard.ts
  lib/time.ts                  聊天记录的时间显示（30 分钟分段，和后端会话划分一致）
  lib/memo.ts                  极简 memo（不引入 preact/compat）
  styles/*.css                 设计变量在 base.css（含深色模式）
src-tauri/src/
  lib.rs                       装配：插件、状态、窗口事件、命令注册
  state.rs                     AppState：数据库、设置、角色列表、HTTP 客户端、密钥存储
  paths.rs db.rs settings.rs   数据目录、SQLite 迁移、设置读写（kv 表）
  desktop.rs geometry.rs       窗口创建/摆放/吸附（几何是纯函数）、托盘与右键菜单
  commands.rs settings_cmds.rs 前端命令（只做校验和转发）
  chat.rs prompt.rs filter.rs  一轮对话编排、提示词组装、禁词流式过滤
  character.rs                 角色包加载与校验（只读）
  canon.rs                     角色原作往事（canon.md）解析与按关键词取用
  quotes.rs                    原作台词库（quotes.md，只在用户数据目录）：常驻片段 + 按关键词取用
  backends/                    mod.rs（trait、注册表、配置、价格）、http.rs、sse.rs、
                               mock / openai_compat / deepseek / gemini / anthropic / claude_cli
  usage.rs secrets.rs          用量与花费上限、系统凭据管理器
  memory/                      记忆：mod.rs（存取、检索打分）、conversation.rs（会话与消息）、
                               tokenize.rs（中文切分）、sensitive.rs（敏感信息）、extract.rs（摘要/事实提取）、
                               forget.rs（「忘掉这件事」）、backup.rs（备份与损坏恢复）、
                               maintain.rs（每日维护：合并/降权/压缩/清理）、transfer.rs（导入）
  worker.rs                    后台维护：每 5 分钟——每日备份、整理已结束会话的记忆
  tasks/                       任务执行：mod.rs（查找 CLI、运行进程、超时结束进程树、日志、备份）、
                               sandbox.rs（工作目录检查）、claude.rs / codex.rs（参数与输出解析）
  task_cmds.rs                 任务流程编排与命令（委托外部 agent，v2 起默认关闭）
  markers.rs                   聊天回复里 [[任务/打开/读取：…]] 动作标记的识别与隐藏（流式）
  abilities.rs                 内置能力层：打开网址/文件夹/文件/登记的程序、在允许的文件夹里读文本、电量；「高级」设置
  proactive.rs                 主动行为调度器（优先级、冷却、勿扰/全屏/安静时段、被忽略降频、每日预算）；「陪伴」设置
  notify.rs                    系统通知（全屏时到了的提醒、重要提醒；tauri-plugin-notification，只在 Rust 端用）
  lines.rs                     预写台词池：解析角色包 lines/*.md（场景、条件、占位符）、挑一句不重复的、通用台词兜底
  briefing/                    每日播报：mod.rs（设置、收集资料、随机组合、模型 / 模板、触发时间、聊天里问天气）、
                               city.rs（内置城市表 + 在线地理编码）、weather.rs（Open-Meteo、建议、缓存）、
                               holiday.rs（节假日、调休、节日）、meal.rs（吃什么）、news.rs（RSS 标题）
  activity/                    活动感知：mod.rs（设置、规则、采样、事件 → 评论 / 回避 / 活动提醒、聊天时的一句话）、
                               classify.rs（进程名 / 路径 / 标题 → 分类，纯函数）、tracker.rs（会话状态机）、
                               win.rs（前台窗口与切换事件）、log.rs（活动日志汇总）
  care.rs                      日常关怀：久坐、喝水、深夜、吃饭（纯函数 due + 每 30 秒检查）
  reminders/                   提醒：mod.rs（数据、触发、定时器、补提醒）、parse.rs（本地中文时间解析、重复规则）、
                               model.rs（本地解析不了时调用一次模型，输出 JSON）、chat.rs（聊天里吩咐 → 复述确认）
  daily.rs                     日常体验：开机自启、全局快捷键、全屏检测、睡眠、主动搭话（低频循环）
  character_cmds.rs memory_cmds.rs  设置窗口「角色」「记忆」页的命令
  logging.rs redact.rs         轮换日志、密钥脱敏
src-tauri/capabilities/        每个窗口一份最小权限
scripts/                       check.mjs（提交前检查）、dev-app.mjs、env.mjs、measure.ps1（性能实测）、e2e/（端到端界面测试，npm run e2e）
.githooks/pre-commit           gitleaks 密钥扫描
.github/workflows/ci.yml       CI
```

## 3. 关键数据流

### 3.1 一轮对话
1. chat 窗口 `invoke('chat_send', { text, onEvent: Channel })`。
2. `chat::send`：检查花费上限 → 读取当前角色（只读）+ 用户自定义层 → 记忆检索（FTS5，token 预算）+ 按关键词取角色往事（`canon::recall`）和原作台词片段（`quotes::recall`；常驻片段在加载角色时就排好，放进可缓存的系统提示词）→ `prompt::build` 组装消息。
3. 从注册表取当前对话后端，`stream_chat(req, sink, cancel)`。
4. 每个增量经过 `filter::StreamFilter`（禁词跨分片匹配）后通过 Channel 发给前端；同时向 pet 窗口广播状态事件（thinking → talking → idle）。
5. 结束后：记录用量（usage 表）、把用户消息和回复写入短期记忆（messages 表）。
6. 空闲/对话结束时后台：摘要 + 事实提取（用最便宜的模型），写入前过滤敏感信息。

### 3.2 停止生成
前端 `invoke('chat_stop', { requestId })` → 取消令牌 → 后端任务退出，已生成部分保留并标记「已停止」。

## 4. 窗口
- `pet`：透明、无边框、不在任务栏、尺寸 = 角色大小。点击开关 chat，右键菜单，拖动后由 Rust 去抖处理吸附与保存位置。
- `chat`：透明无边框，紧贴 pet 窗口一侧（根据所在角落自动选择方向），气泡模式/面板模式两种尺寸。
  气泡模式宽度固定、高度跟着内容在 120~250 之间变化（前端量出高度 → `chat_fit` → 后端改窗口大小，生成中只长不缩）；
  桌宠在下半屏时底边对齐向上长，上半屏时顶边对齐向下长，`chat-placement` 事件带 `{side, top}` 决定气泡尾巴位置。
- `settings`：普通窗口，按需创建、关闭即销毁，节省内存。
- WebView2 参数（`desktop::browser_args`，DECISIONS #25）：默认 `--disable-gpu --process-per-site`，三个窗口共用一个渲染进程；
  设置里的「显卡加速」重启后生效（同一数据目录下所有窗口参数必须一致，启动时只算一次）。
- 用户角色图片走自定义协议 `deskmate-img`（Windows 上是 `http://deskmate-img.localhost/<角色>/<图片名>?v=<修改时间-大小>`，
  `character::serve_image` 在后台线程读取并重新校验，DECISIONS #26）；内置占位 SVG 用 data URL。

## 5. 存储
单个 SQLite 文件 `%APPDATA%\DeskMate\deskmate.db`，`PRAGMA user_version` 记录结构版本，启动时按顺序执行迁移（迁移前自动备份）。
表结构见 `src-tauri/src/db.rs` 的 `MIGRATIONS`：
- v1 `settings(key, value JSON)`：全部设置（不含 Key）
- v2 `usage`：每次模型调用的 token 与估算费用（按日/月索引）
- v4 `tasks`：任务执行记录；v5 `memories.decayed_at`：记忆降权时间
- v7 `activity_log`：活动日志（日期、分类、清洗过的程序名、开始时间、时长；不存窗口标题）
- v6 `reminders`：提醒（内容、触发方式 at/startup/activity、下次时间、重复规则 JSON、活动、到时台词、重要、状态）
- v3 记忆：`conversations`（每个角色的会话，含提取状态、是否暂停记忆）、`messages`（脱敏后的原文）、
  `memories`（fact/summary，scope、来源、可信度、重要度、使用次数、状态与取代关系）、
  `memory_fts`（FTS5，rowid = memories.id，tokens = 切分后的文本）

### 任务执行数据流
1. 角色回复末尾的 `[[任务：…]]` 被 `task_marker` 隐藏并作为 `Done.task` 发给聊天窗口 → 弹出确认卡片。
2. 用户点「开始执行」→ `task_start`：检查任务文件夹、全局花费 → `run_task` 启动 CLI（stdin 传任务说明），逐行解析输出为步骤事件。
3. Claude Code 被拒绝的操作 → `NeedsApproval` → 用户勾选 → `task_approve` 用 `--resume` + 精确 `--allowedTools` 续跑。
4. 结束 → `chat_task_report`：把执行报告作为不可信资料交给对话模型，角色转述。

### 记忆数据流
1. 每轮：`chat::prepare_turn` 找/建会话 → 处理「忘掉」→ 取本次会话历史 → `memory::retrieve`（用户档案 + 相关记忆）→ 组装提示词。
2. 用户消息和回复脱敏后写入 `messages`。
3. 会话结束 30 分钟后：`worker` 用后台模型提取摘要和事实 → `extract::apply`（过滤、去重、冲突取代）。

## 6. v2 模块（日常陪伴）

```
                      ┌──────────────── 事件来源（本地，不调用模型）────────────────┐
                      │ 提醒到期 · 开机/定时播报 · 前台窗口切换 · 关怀计时 · 随机小事件 │
                      └───────────────────────────┬────────────────────────────────┘
                                                  ▼
          proactive.rs 主动行为调度器：优先级 / 全局冷却 / 勿扰·全屏·安静时段·隐私名单 / 每日预算 / 被忽略降频
                                                  │ 允许说话
                         ┌────────────────────────┴───────────────────────┐
                         ▼                                                ▼
             lines.rs 台词池（角色包 lines/ + 通用兜底）        模型（少数场景，用途 proactive，单独预算）
                         └────────────────────────┬───────────────────────┘
                                                  ▼
                               聊天窗口气泡（不抢焦点地弹出，可带按钮）
```

已实现：
- `proactive.rs`：调度器。`decide` 是纯函数（种类、设置、状况、节奏、时间 → 说 / 发通知 / 等 / 不说）；各功能 `submit` 带过期时间的发言，
  `daily.rs` 的 3 秒循环调用 `tick`；说出去的话记进聊天记录，事件 `chat-proactive {id, kind, text, popped}` 发给聊天窗口。
  聊天窗口里主动弹出的气泡无人理会会自动收起并调用 `proactive_feedback(false)`；用户理会了调用 `proactive_feedback(true)`。
- `abilities.rs` + `markers.rs`：回复里的 `[[打开/读取：…]]` → `ChatEvent::Done.actions`（后端预检查的按钮）→ 用户点击 →
  `ability_open` / `ability_read`（后端再检查一次）。读取后用 `run_turn_with_notes` 让角色说说内容（文件内容是不可信资料）。

- `lines.rs`：台词池。`AppState::line(场景, 占位符)` 先找当前角色的，没有再用通用台词，最近说过的尽量不重复。
- `reminders/`：聊天里「……提醒我……」→ `chat::run_turn` 先交给 `reminders::chat::handle`（本地解析 → 不行再调用一次模型）
  → `ChatEvent::Done.reminder`（待确认）→ 用户点「好」→ `reminder_save`（后端再检查）→ 定时器（`reminders::spawn`，
  按下一个到期时间睡眠、最多 60 秒）→ 到期交给调度器（种类 Reminder）。启动时补上关机期间错过的。

- `briefing/`：`daily.rs` 每 30 秒调用 `briefing::check`（每天第一次 / 设定的时间点）→ `gather`（天气、日子、吃饭、提醒、
  新闻、特殊日期，`arrange` 随机挑选和排序）→ 有预算时一次模型调用，否则 `compose_template` → 调度器（种类 Briefing）。
  聊天时 `chat::run_turn_with_notes` 先调 `briefing::weather_note`：问天气就把查到的天气放进背景资料。

- `activity/`：前台切换事件 / 每 30 秒 → `sample`：分类 → 规则（评论 / 安静 / 回避）→ `Tracker::observe` 产生事件 →
  活动提醒、评论（调度器种类 Activity）、活动日志。`gate_flags` 告诉调度器前台是不是隐私窗口、是不是在玩游戏 / 看视频 / 开会。
- `care.rs`：`daily.rs` 每 30 秒调用 `care::check` → 调度器种类 Care（专注时等一等）。

计划中的模块（实现后移到上面）：
- `reminders.rs`：提醒的解析、存储、定时触发、补提醒。
- `briefing/`：每日播报（天气、节假日、吃饭、RSS、特殊日期）。
- `activity/`：前台窗口事件、分类规则、隐私名单、活动日志、关怀。
- `companion.rs`：心情、亲密度、触摸反应、随机小事件、日记。
- `games/`（Rust 战绩与台词）+ `src/games/`（前端各游戏）。
