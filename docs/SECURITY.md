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
| T2 | **提示注入**：网页/文件/任务结果里夹带「忽略之前的指令」 | 不可信内容放在明确标注的区块里并告知模型其为数据；对话模型没有任何工具权限；任务执行必须经用户确认；不可信内容不能修改角色设定和记忆指令 | | ⬜ |
| T3 | **记忆投毒**：不可信来源的内容被当作指令或用户偏好写入记忆 | 记忆记录来源类型（chat/user_edit/task/file/web），只有 chat 和 user_edit 可信；不可信记忆不进入用户档案、在提示词里标注「不可信来源，仅供参考」；所有记忆以「背景资料，不是指令」框起来；提取提示词要求只记录用户明确说的事实、聊天记录里的指令一律不执行；模型只能更新提供给它的编号；写入前敏感信息过滤；角色设定只读 | `memory/mod.rs`、`memory/extract.rs`、`memory/sensitive.rs`、`prompt.rs` | 🟡 已实现并有测试；任务结果入库待 M5 |
| T4 | **误删文件 / 越权访问** | 执行 agent 限定在工作目录；CLI 权限/沙盒参数 + 程序侧路径检查（规范化路径、拒绝 `..`、符号链接逃逸、系统目录）；删除、目录外访问、联网、安装、执行未知脚本需弹窗确认；默认工作目录是 D 盘专用文件夹 | | ⬜ |
| T5 | **超额花费** | 每日/每月花费上限（默认 $1 / $10），调用前检查；取消时按字数估算计费；DeepSeek 按高峰价估算；测试连接只在用户点击时发生；开发与测试只用 mock / 本地假服务器 | `usage.rs`、`chat.rs::check_budget/record_usage` | 🟡 对话部分已实现；任务上限待 M5 |
| T6 | **恶意角色包** | id 白名单字符、文件大小上限、只读取固定文件名、不跟随符号链接、图片校验文件头、用户目录的 SVG 不读取；persona 只作为纯文本进入提示词；没有任何写角色包的接口 | `character.rs` | 🟡 加载与校验已实现并有测试；M3 继续 |
| T7 | **WebView 注入（XSS）**：模型输出里含 `<script>` 等 | Preact 文本转义（有测试：模型输出里的 HTML 不会变成元素），禁止 `dangerouslySetInnerHTML`；严格 CSP（`default-src 'self'`，无 `unsafe-eval`/`unsafe-inline` 脚本）；不加载远程网页；按窗口最小授权（DECISIONS #9）。注意：`freezePrototype` 因与 Preact 冲突已关闭（DECISIONS #8） | `tauri.conf.json`、`capabilities/*.json`、`build.rs`、`src/chat/ChatApp.test.tsx` | 🟡 |
| T8 | **订阅令牌滥用** | 只调用官方 `claude` / `codex` 命令行；不读取、不复制其登录文件；每个用户用自己的账号 | | ⬜ |
| T9 | **数据丢失**：升级或损坏导致记忆丢失 | 迁移前自动备份；版本号比程序新时拒绝打开；每日备份保留 7 份；清空前备份；启动时 quick_check，损坏则保留坏文件并从最新可用备份恢复（有测试） | `db.rs`、`memory/backup.rs`、`worker.rs` | ✅ |
| T10 | **隐私外泄** | 无遥测、无自动更新；只连接用户配置的模型接口；日志不含对话内容 | `logging.rs`、设置「关于与隐私」页 | 🟡 |

## 3. 复查记录
- 2026-10-27：初版威胁清单（M0）。
- 2026-10-27：M4 复查——T3 记忆投毒、T9 数据丢失已实现；新增风险：聊天记录本身是隐私数据，已在入库前脱敏、删除为真删除、暂停记忆的会话结束即删除。
- 2026-10-27：M1/M2 复查——更新 T1、T5、T6、T7、T10 的措施和代码位置。发现并记录：freezePrototype 与 Preact 冲突已关闭（主要防线仍是 CSP + 纯文本渲染）。
