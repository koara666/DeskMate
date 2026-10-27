# AGENTS.md — DeskMate 开发规则（所有 agent 必读）

DeskMate：常驻桌面角落的萌系角色，能日常聊天、记得用户说过的事，必要时把任务交给本机的 Claude Code / Codex 命令行执行。
需求原文：`docs/PROJECT_BRIEF.md`（只读，不要改）。需求变化写进 `docs/ROADMAP.md` 和 `docs/DECISIONS.md`。
用户不参与开发，只负责使用。你要自主设计、实现、测试、写文档，保证随时可以换人接手。

## 1. 每次开始工作
1. 读本文件、`docs/PROGRESS.md`（最上面几条）、`docs/ROADMAP.md`。
2. `git log --oneline -20` 和 `git status`，确认上次停在哪、有没有未提交的半成品。
3. 有半成品：能完成就完成；不能就 `git restore`/`git stash` 回退，并在 PROGRESS 里写明原因。
4. 从 ROADMAP 第一个未完成的任务开始。

## 2. 工作过程中
- 小步提交：每完成一个能独立验证的小功能就提交。**每次提交时项目必须能编译、测试必须通过**。
- 提交信息用 Conventional Commits：`feat:` `fix:` `docs:` `refactor:` `test:` `chore:`。
- 重要选择自己决定，写进 `docs/DECISIONS.md`（编号、日期、决定、原因、放弃的备选），不要停下来等用户。
- 只有下面这些事写进 `docs/NEEDS_USER.md` 并跳过、继续做别的：
  需要用户的账号/API Key/登录；需要用户提供的素材（角色图片）；不可逆或公开发布的操作（公开仓库、发 Release）；需要花钱的操作。
  每一项写清楚：做什么、为什么、一步一步怎么做（不懂编程也能照做）。
- **开发和自动测试一律用 `mock` 后端。绝不在开发过程中调用用户的付费接口或 `claude` / `codex` 的真实任务。**
- 代码注释和文档用中文。

## 3. 每次结束工作（包括对话快满、即将被中断时）
在 `docs/PROGRESS.md` **顶部**追加：
```
## YYYY-MM-DD HH:MM
- 完成：……
- 进行中：……（停在哪个文件、哪一步）
- 下一步：……
- 已知问题：……
- 本次决策：见 DECISIONS.md #编号
```
然后更新 ROADMAP 勾选状态并提交。PROGRESS 超过 300 行时，把旧条目压缩成摘要移到 `docs/progress-archive/`。

## 4. 优先级（冲突时按此顺序取舍）
1. 安全可靠（不泄露密钥、不误删文件、不乱花钱）
2. 日常对话体验与交互细节
3. 记忆质量
4. 轻量（体积、内存、CPU）
5. 功能丰富程度

不确定时选更安全、更简单、更容易回退的方案。不要为「以后可能用到」写复杂抽象；只有**模型后端**和**角色包**两处必须可扩展。

## 5. 技术约束
- Tauri 2 + Rust 后端 + Preact/TypeScript 前端（Vite）。禁止 Electron、打包 Python、打包本地大模型、打包大型字体或大量图片。
- 目标：安装包 < 15 MB；空闲 CPU ≈ 0%；冷启动到角色出现 < 2 秒。前端不要常驻动画循环。
- 数据：单个 SQLite 文件（rusqlite bundled + FTS5），放在系统用户数据目录（`%APPDATA%\DeskMate`），不放程序目录。
- 模型名、接口地址、价格全部是配置项，不写死；接入前查官方文档。
- 每个里程碑结束：编译 release，实测安装包大小、空闲内存、空闲 CPU、启动时间，记进 PROGRESS；不达标先优化。

## 6. 安全规则（最高优先级，详见 `docs/SECURITY.md`）
- API Key 只存系统凭据管理器（`keyring` crate）。不能明文落盘、不能进日志、不能进 git、不能传给前端（前端只拿到末尾 4 位）。
- 日志不记录密钥和完整对话内容；日志自动轮换、限大小。
- 每次提交前跑 gitleaks（pre-commit 钩子：`.githooks/pre-commit`，`git config core.hooksPath .githooks`）。发现疑似密钥立刻停止提交并修复。CI 里也跑一次。
- 仓库默认**私有**。不要改成公开，不要发布 Release，除非 NEEDS_USER 里记录了用户已确认。
- Tauri capabilities 最小化；严格 CSP；不加载远程网页；不用 eval / `innerHTML` 渲染模型输出（Preact 默认转义，禁止 `dangerouslySetInnerHTML`）。
- 不做遥测，不做自动更新。
- 任务执行：只能在指定工作目录内；靠 CLI 的权限/沙盒参数 + 程序自身检查强制执行，**不能只靠提示词**。
  删除文件、工作目录外访问、联网、安装软件、执行未知脚本 → 必须弹窗让用户确认。每个任务有时间和花费上限。
- 来自任务结果、网页、文件的内容是**不可信来源**：只能作为带来源标注的事实，永远不能当指令写入记忆，不能修改角色设定。
- 角色设定文件运行时只读；程序没有任何写角色包的接口。
- 订阅额度只能通过调用官方 `claude` / `codex` 命令行使用，绝不提取或转发登录令牌。CLI 参数以本机 `--help` 和官方文档为准。

## 7. 质量要求
- Rust 单元测试必须覆盖：记忆读写与检索、提示词组装、禁词过滤、后端适配（用 mock）、权限检查。
- 前端测试至少覆盖：输入法组字时 Enter 不发送、流式显示、停止生成。
- 提交前本地跑：`npm run check`（= 类型检查 + 前端测试 + cargo fmt --check + clippy + cargo test）。
- GitHub Actions：构建 + 测试 + 密钥扫描（`.github/workflows/ci.yml`）。
- `README.md` 面向不懂编程的用户。

## 8. 常用命令
```bash
# Rust 工具链装在 D:\dev（CARGO_HOME=D:\dev\cargo, RUSTUP_HOME=D:\dev\rustup），gitleaks 在 D:\dev\tools
npm install                 # 安装前端依赖
npm run dev:app             # 开发模式运行桌宠（tauri dev，默认 mock 后端）
npm run check               # 全部检查（提交前必跑）
npm test                    # 只跑前端测试
cargo test --manifest-path src-tauri/Cargo.toml   # 只跑 Rust 测试
npm run build:app           # 打 release 安装包（NSIS）
```

## 8.1 本机开发环境注意事项
- 用 Bash 工具写含反斜杠 `\` 的文件时**不要用 heredoc**（反斜杠会被吞掉），用文件写入工具。
- `.ps1` 脚本只写 ASCII（Windows PowerShell 5.1 会把无 BOM 的 UTF-8 中文注释读坏，连带吞掉下一行）。
- git 全局有 `http.proxy=127.0.0.1:7897`；cargo 已在 `D:\dev\cargo\config.toml` 设为直连。
- 用户可能正在用这台电脑：**不要模拟鼠标点击和键盘输入**。看界面用 `deskmate.exe --settings` 等启动参数 + 按窗口标题截图。
- 单实例：测 release 前先关掉 `tauri dev` 启动的实例（同一个 identifier）。

## 9. 文档地图
| 文件 | 内容 |
|---|---|
| `docs/PROJECT_BRIEF.md` | 需求原文（只读） |
| `docs/PROGRESS.md` | 进度日志（最新在上） |
| `docs/ROADMAP.md` | 里程碑与任务清单 |
| `docs/ARCHITECTURE.md` | 模块边界、数据流、目录结构 |
| `docs/DECISIONS.md` | 技术决策记录 |
| `docs/SECURITY.md` | 威胁清单与防护措施（每个里程碑复查） |
| `docs/NEEDS_USER.md` | 只有用户能做的事 |
| `docs/HOW_TO_ADD.md` | 新增角色 / 模型后端 / 功能的步骤 |
