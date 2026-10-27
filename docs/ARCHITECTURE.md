# 架构说明

> 本文件描述「现在的代码是什么样」。设计变化时同步更新。决策理由见 DECISIONS.md。

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
│  backends/  ── 模型后端 trait + 注册表：mock / openai_compat(DeepSeek) / gemini / anthropic │
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

## 2. 目录结构（规划，随实现更新）

```
AGENTS.md / CLAUDE.md       agent 规则
docs/                       文档（见 AGENTS.md 文档地图）
characters/<id>/            内置角色包（character.json、persona.md、assets/placeholder.svg）
src/                        前端
  pet/                      桌宠窗口
  chat/                     聊天窗口（气泡 + 面板）
  settings/                 设置窗口
  lib/                      IPC 封装、类型、工具函数
src-tauri/                  Rust
  src/lib.rs                应用装配（插件、窗口、托盘、命令注册）
  src/<模块>/               见上图
  capabilities/             Tauri 权限（最小化）
.githooks/pre-commit        gitleaks 密钥扫描
.github/workflows/ci.yml    CI
```

## 3. 关键数据流

### 3.1 一轮对话
1. chat 窗口 `invoke('chat_send', { text, onEvent: Channel })`。
2. `chat::send`：检查花费上限 → 读取当前角色（只读）+ 用户自定义层 → 记忆检索（FTS5，token 预算）→ `prompt::build` 组装消息。
3. 从注册表取当前对话后端，`stream_chat(req, sink, cancel)`。
4. 每个增量经过 `filter::StreamFilter`（禁词跨分片匹配）后通过 Channel 发给前端；同时向 pet 窗口广播状态事件（thinking → talking → idle）。
5. 结束后：记录用量（usage 表）、把用户消息和回复写入短期记忆（messages 表）。
6. 空闲/对话结束时后台：摘要 + 事实提取（用最便宜的模型），写入前过滤敏感信息。

### 3.2 停止生成
前端 `invoke('chat_stop', { requestId })` → 取消令牌 → 后端任务退出，已生成部分保留并标记「已停止」。

## 4. 窗口
- `pet`：透明、无边框、不在任务栏、尺寸 = 角色大小。点击开关 chat，右键菜单，拖动后由 Rust 去抖处理吸附与保存位置。
- `chat`：透明无边框，紧贴 pet 窗口一侧（根据所在角落自动选择方向），气泡模式/面板模式两种尺寸。
- `settings`：普通窗口，按需创建、关闭即销毁，节省内存。

## 5. 存储
单个 SQLite 文件 `%APPDATA%\DeskMate\deskmate.db`，`PRAGMA user_version` 记录结构版本，启动时按顺序执行迁移（迁移前自动备份）。
表结构见 `src-tauri/src/memory/migrations.rs`（实现后在此补充摘要）。
