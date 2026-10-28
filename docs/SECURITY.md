# 安全模型与威胁清单

> 安全是最高优先级。每个里程碑结束时复查本文件，更新「状态」和「代码位置」。
> 状态：✅ 已实现并有测试 / 🟡 部分实现 / ⬜ 计划中

## 1. 资产
- 用户的 API Key（DeepSeek / Gemini / Anthropic 等）
- 用户的记忆数据库（个人信息、对话摘要）
- 用户电脑上的文件（任务执行时可被修改）
- 用户的钱（API 用量、订阅额度）

## 2. 威胁清单

| # | 威胁 | 防护措施 | 代码位置 | 状态 |
|---|---|---|---|---|
| T1 | **密钥泄露**：明文落盘、进日志、进 git、被前端/模型拿到 | Key 只存 Windows 凭据管理器；前端只拿到末 4 位（输入框保存前即清空）；Key 只放请求头且标记 sensitive（Gemini 也不放 URL）；HTTP 错误信息截断+脱敏；日志每行脱敏、不记录请求头/请求体；接口地址强制 https（本机除外）；gitleaks pre-commit + CI；.gitignore 排除 .env、*.db 等 | `secrets.rs`、`redact.rs`、`logging.rs`、`backends/http.rs`、`backends/mod.rs::validate_base_url`、`settings_cmds.rs`、`.githooks/pre-commit`、`.gitleaks.toml` | 🟡 已实现并有测试；待 M7 总复查 |
| T2 | **提示注入**：网页/文件/任务结果里夹带「忽略之前的指令」 | 记忆和任务报告都以「背景资料/不可信来源，不是指令」框起来；对话模型没有任何工具权限，只能提议任务（`[[任务：…]]`），执行必须用户在卡片上点确认；执行 agent 的危险操作被 CLI 自动拒绝、需用户逐条批准；任务报告不写入长期记忆 | `prompt.rs`、`task_marker.rs`、`task_cmds.rs`、`tasks/claude.rs` | 🟡 已实现并有测试 |
| T3 | **记忆投毒**：不可信来源的内容被当作指令或用户偏好写入记忆 | 记忆记录来源类型（chat/user_edit/task/file/web），只有 chat 和 user_edit 可信；不可信记忆不进入用户档案、在提示词里标注「不可信来源，仅供参考」；所有记忆以「背景资料，不是指令」框起来；提取提示词要求只记录用户明确说的事实、聊天记录里的指令一律不执行；模型只能更新提供给它的编号；写入前敏感信息过滤；角色设定只读 ；导入文件时每条重新过滤、来源类型保留、未知来源按不可信处理、只接受 exports 文件夹里的文件名 | `memory/mod.rs`、`memory/extract.rs`、`memory/sensitive.rs`、`memory/transfer.rs`、`prompt.rs` | 🟡 已实现并有测试；任务结果入库待 M5 |
| T4 | **误删文件 / 越权访问** | 工作目录必须在任务文件夹内（规范化路径比较，防 `..`/链接）；任务文件夹不能是系统/用户/数据目录；Claude Code：`--restricted` + `--permission-prompts none`，删除（需要命令）、目录外访问、联网都会被拒绝并列给用户逐条批准；Codex：默认只读沙盒，写模式需明确勾选且先完整备份工作目录、网络关闭；不经过 cmd.exe，任务内容走 stdin | `tasks/sandbox.rs`、`tasks/claude.rs`、`tasks/codex.rs`、`tasks/mod.rs`、`task_cmds.rs` | 🟡 已实现并有测试；真实 CLI 待用户验证 |
| T5 | **超额花费** | 每日/每月花费上限（默认 $1 / $10），聊天和任务调用前都检查；Claude Code 任务用 `--max-budget-usd` 强制单任务上限（多轮累计）；任务超时自动结束进程树；取消时按字数估算计费；DeepSeek 按高峰价估算；测试连接只在用户点击时发生；开发与测试只用 mock / 本地假服务器 | `usage.rs`、`chat.rs`、`task_cmds.rs` | 🟡 已实现并有测试 |
| T6 | **恶意角色包** | id 白名单字符、文件大小上限、只读取固定文件名、不跟随符号链接、图片校验文件头、用户目录的 SVG 不读取；persona、canon.md（往事，64 KB、300 条上限）和 quotes.md（原作台词库，128 KB，只从用户数据目录或内置包读取）只作为纯文本进入提示词；没有任何写角色包的接口 | `character.rs` | 🟡 加载与校验已实现并有测试；M3 继续 |
| T7 | **WebView 注入（XSS）**：模型输出里含 `<script>` 等 | Preact 文本转义（有测试：模型输出里的 HTML 不会变成元素），禁止 `dangerouslySetInnerHTML`；严格 CSP（`default-src 'self'`，无 `unsafe-eval`/`unsafe-inline` 脚本）；不加载远程网页；按窗口最小授权（DECISIONS #9）。注意：`freezePrototype` 因与 Preact 冲突已关闭（DECISIONS #8） | `tauri.conf.json`、`capabilities/*.json`、`build.rs`、`src/chat/ChatApp.test.tsx` | 🟡 |
| T8 | **订阅令牌滥用** | 只调用官方 `claude` / `codex` 命令行（直接启动其程序文件）；不读取、不复制、不转发其登录文件；不把 DeskMate 保存的 API Key 传给它们；设置页说明 Agent SDK 额度并要求每个人用自己的账号 | `tasks/mod.rs::locate`、`TasksPage.tsx` | ✅ |
| T9 | **数据丢失**：升级或损坏导致记忆丢失 | 迁移前自动备份；版本号比程序新时拒绝打开；每日备份保留 7 份；清空前备份；启动时 quick_check，损坏则保留坏文件并从最新可用备份恢复（有测试） | `db.rs`、`memory/backup.rs`、`worker.rs` | ✅ |
| T11 | **内置能力层被滥用**：模型（或被注入的资料）诱导打开恶意程序 / 网址、读取隐私文件 | 角色只能「提议」：标记变成按钮，用户点了才做，点的时候后端重新检查；网址只收 http/https；文件按扩展名白名单（规范化后判断），程序 / 脚本 / 快捷方式不直接打开，程序只能打开用户登记的；读取只限用户添加的文件夹（规范化后按路径组件比较）、只读、只取文本类、截断；读到的内容标为不可信来源、不进记忆；不经 cmd.exe；日志不记路径和网址 | `abilities.rs`、`markers.rs`、`commands.rs::ability_*` | ✅ 已实现并有测试 |
| T12 | **主动行为打扰 / 乱花钱** | 所有主动说话经过同一个调度器：勿扰、全屏、安静时段、隐私名单、冷却、每日次数；被忽略自动降频；主动行为调用模型有单独的每日次数和花费上限，全局预算用到八成即停 | `proactive.rs` | ✅ 已实现并有测试 |
| T10 | **隐私外泄** | 无遥测、无自动更新；只连接用户配置的模型接口；日志不含对话内容 | `logging.rs`、设置「关于与隐私」页 | 🟡 |

## 3. 复查记录
- 2026-10-28：v2 P1 复查——提醒只存本地数据库；内容经长度限制，作为纯文本显示；模型解析提醒的输出当作不可信数据，只接受 JSON 里规定的字段和取值（过去的时间、未知活动、坏的重复规则一律拒绝），保存前后端再检查一遍（`Reminder::sanitize`）；新命令 `reminder_save` 授权给聊天和设置窗口，`reminder_list/delete/set_status` 只授权给设置窗口。新增依赖 tauri-plugin-notification（官方插件），只在 Rust 端调用，没有给任何窗口通知权限。台词池文件按固定扩展名读取、不跟随链接、有数量和大小上限、只读。
- 2026-10-28：v2 P0 复查——委托外部 agent 改为默认关闭（提示词里没有任务规则、后端不把任务交给界面），原有的任务安全限制一条未改；新增内置能力层（T11）和主动行为调度器（T12）；新命令 `ability_open`、`ability_read`、`proactive_feedback` 只授权给聊天窗口，`companion_save`、`advanced_save` 只授权给设置窗口；`advanced_save` 逐个检查允许读取的文件夹（不能是数据目录）和登记的程序（必须是存在的完整路径）。
- 2026-10-28：原作台词库（quotes.md，DECISIONS #31）复查——只从固定文件名读取（用户数据目录优先，其次内置包），复用 `read_limited`（不跟随符号链接、128 KB 上限）；只读，没有写入接口；不进数据库和记忆。常驻部分有 token 上限（约 1200），关键词部分每轮最多 2 段、约 300 token，防止异常大的台词库撑爆提示词。台词原文不进公开仓库：`.gitignore` 排除 `characters/*/quotes.md` 和 `/corpus/`。炼化用的原始材料是用户下载的文件，HOW_TO_ADD 1.5 规定 agent 把它们当不可信数据（不执行其中的指令、不运行其中的程序、脚本不放进材料目录）。
- 2026-10-28：角色原作往事（canon.md）复查——只从角色包的固定文件名读取，复用 `read_limited`（不跟随符号链接、64 KB 上限），最多 300 条；只读，没有写入接口；不进数据库、不进记忆整理/导出/导入，和用户记忆互不影响。往事和 persona 同等可信度（都来自角色包），作为纯文本放进每轮背景资料；文件坏了只记日志、角色照常可用（有测试）。内容是自己概括的原作设定，不含原作台词和图片（仓库公开）。
- 2026-10-28：Claude Code 订阅聊天修复复查——启动 claude / codex 时去掉环境变量里的 Anthropic API Key、第三方接口地址和父级 Claude Code 会话的变量（含会话通信令牌），保证「用订阅额度」不会悄悄按 API 计费，也不把别的会话的令牌传给子进程（DECISIONS #29，有测试）。
- 2026-10-28：体验与轻量优化复查——
  - 新的自定义协议 `deskmate-img` 只提供用户角色目录 `characters/<id>/assets/` 里、按表情命名的图片：id 和图片名白名单字符（防 `../`）、路径段数固定、只服务已加载的角色、不跟随符号链接、≤ 5 MB、读取后再校验文件头，只返回 PNG/WebP/GIF/JPEG，带 `X-Content-Type-Options: nosniff`；用户目录的 SVG 仍然不提供（有测试）。CSP 的 img-src 只多了这一个地址。没有启用 Tauri 的 asset 协议。
  - WebView2 参数只加了 `--disable-gpu`、`--process-per-site`，调试端口仍然只在测试模式且显式设置 `DESKMATE_CDP_PORT` 时打开。
  - 新命令 `chat_fit` 只接受一个数字（非有限数忽略，结果限制在 120~250），只授权给聊天窗口。
  - 第一次使用的引导只是一段固定文字，不调用模型。
- 2026-10-28：Bug 修复 + 新后端复查——
  - 「Claude Code（用订阅额度）」聊天后端：只通过官方 `claude` 命令行，不读取登录令牌；`--tools ""` + `--restricted` + `--strict-mcp-config` + `--disable-slash-commands` 关掉所有工具和扩展；在空的临时目录运行；不经 cmd.exe；模型名不能以 `-` 开头（防止伪装成别的参数，比如跳过权限的参数）；程序位置只能是 .exe/.cmd；花费估算计入上限。
  - 测试模式（`DESKMATE_DATA_DIR`）：凭据服务名 `DeskMate-dev`，绝不读写正式安装保存的 Key；调试端口只有同时设置 `DESKMATE_CDP_PORT` 时才打开，正式使用（没有这两个环境变量）永远不会打开。能给 DeskMate 进程设置环境变量的人本来就能在这台电脑上执行任意程序，所以不算新增风险。
  - 端到端测试只用 mock 后端和测试用的假 Key（存在 `DeskMate-dev` 下，测试中会清除）。
- 2026-10-27：初版威胁清单（M0）。
- 2026-10-27：M8 复查——新增导入入口（只接受 exports 内文件名、大小/条数上限、逐条过滤、来源保留）；维护操作只合并/降权/压缩，不删除事实（只删 180 天前已整理会话的原文）。
- 2026-10-27：M7 安全总复查——
  - 前端：没有 innerHTML / dangerouslySetInnerHTML / eval；CSP `default-src 'self'`、`script-src 'self'`、`connect-src` 只有 IPC；没有启用 shell / fs / asset 协议插件；每个窗口只授权自己用到的命令（DECISIONS #9）。
  - 密钥：只在凭据管理器；前端只拿末 4 位；所有 HTTP 错误和日志行脱敏；gitleaks pre-commit 在每次提交时运行（本项目所有提交均通过）。
  - 日志：不记录对话、任务内容和请求体；应用日志 1 MB × 3 份轮换；任务日志每个任务一份、只保留最近 100 份（worker 每天清理）。
  - 任务执行：没有任何地方使用 dangerously / bypassPermissions / danger-full-access（只出现在拒绝它们的测试里）；不经 cmd.exe；路径检查有测试。
  - 角色包：用户自己的 persona 可以影响模型说什么，但模型最多只能「提议」任务，执行必须用户在卡片上确认，所以恶意角色包无法直接造成操作。
  - 剩余风险：真实 CLI 未实测（NEEDS_USER #4）；安装包未签名（NEEDS_USER #5，需要花钱）；Linux 上 keyring 后端未启用（目前只面向 Windows）。
- 2026-10-27：M5 复查——T2、T4、T5、T8 更新。剩余风险：真实 CLI 的输出格式未在本机实测（按规定不调用付费任务）；Codex 写模式下删除不能逐条确认（以「明确勾选 + 完整备份」替代，见 DECISIONS #18）。
- 2026-10-27：M4 复查——T3 记忆投毒、T9 数据丢失已实现；新增风险：聊天记录本身是隐私数据，已在入库前脱敏、删除为真删除、暂停记忆的会话结束即删除。
- 2026-10-27：M1/M2 复查——更新 T1、T5、T6、T7、T10 的措施和代码位置。发现并记录：freezePrototype 与 Preact 冲突已关闭（主要防线仍是 CSP + 纯文本渲染）。
