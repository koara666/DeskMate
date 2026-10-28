# 如何新增角色 / 模型后端 / 功能

> 本文件随实现更新。新增东西时，先读对应小节，按步骤做。

## 1. 新增角色（不需要改代码）

### 1.1 角色包结构
```
<角色id>/
  character.json   # 元信息、表情映射、角色口吻的固定台词
  persona.md       # 性格设定 + 5~10 段原创示范对话（每轮都放进提示词）
  canon.md         # 可选：原作往事，按关键词取用（不聊到不占 token）
  assets/
    placeholder.svg  # 仅内置角色：原创的简单占位形象（160×180 viewBox）
```
- 角色 id 只能用小写字母、数字、`-`、`_`，最长 40 个字符，**必须和文件夹名一致**。
- 放在哪里：
  - 随程序发布的内置角色：仓库的 `characters/<id>/`（只放原创占位 SVG，**绝不放官方或他人绘制的图片**）。
  - 用户自己加的角色：`%APPDATA%\DeskMate\characters\<id>\`（有 character.json + persona.md 才算新角色；同 id 会覆盖内置设定）。
- 加好以后：设置 →「角色」→「重新加载」，或重启程序。

### 1.2 character.json 字段
| 字段 | 必填 | 说明 |
|---|---|---|
| `id` | 是 | 同文件夹名 |
| `name` | 是 | 显示名 |
| `order` | 否 | 列表排序，越小越靠前（默认 1000） |
| `version` / `author` / `language` | 否 | `language` 默认 `zh-CN` |
| `description` | 否 | 一句话介绍（设置界面显示） |
| `source` | 否 | 原作/原型出处，同人角色要写明「与官方无关」 |
| `color` | 否 | 主题色 `#rrggbb`（气泡边框、按钮） |
| `expressions` | 否 | 状态 → 图片文件名（不含扩展名），如 `{"sad": "confused"}`；不写就用同名图片 |
| `lines` | 否 | 固定台词（不调用模型，省钱）：`greeting`、`error`、`offline`、`budget`、`not_configured`，每项是字符串数组，随机挑一句 |

表情状态：`idle` 待机、`talking` 说话、`thinking` 思考、`happy` 开心、`sad` 难过/困惑、`sleep` 睡觉。

### 1.3 persona.md 写法
按这几节写（参考 `characters/makise-kurisu/persona.md`）：身份背景（简短）、性格关键词、说话方式与语气、称呼用户的方式、口癖倾向、会做的事、不会做的事、示范对话（5~10 段，**必须原创，不能摘抄原作台词**，标题里写「原创」）。
- 「会做的事」里写上：用户想让角色动手做事时，先确认要做什么、在哪个文件夹。
- 角色设定在运行时只读，程序和模型都不能改。

### 1.4 canon.md（可选：原作往事）
有原作的角色（参考 `characters/makise-kurisu/`）可以把经历分成两层，既像本人又省 token（DECISIONS #30）：
- **persona.md** 写「你是谁」（就是原作里的那个人、记忆停在哪、现在以 AI 的形式待在桌面上、用户是新认识的人）+ 档案 + 最核心的几段经历 + 性格和说话方式。它每轮都带上（可缓存），所以要精炼。
- **canon.md** 写细节往事，每条格式如下；只有本轮用户的话（或角色上一句回复）里出现关键词时才放进提示词，每轮最多 3 条、约 500 token：
```
## 父亲
关键词：你爸、你父亲、中钵、章一
正文：用自己的话概括，每条尽量 200 字以内。
```
- 关键词要**具体**：用人名、专有名词，或「你爸」「你小时候」这种只有在问角色时才会出现的说法。不要用「绝望」「希望」「毕业」「老师」「我爸爸」这类日常词，否则用户聊自己的事时也会把往事带进来，白花 token。单字关键词会被忽略；英文关键词不区分大小写、按整词匹配。
- 第一个 `## ` 之前是给维护者看的说明，程序忽略。没有关键词或正文的条目会被丢掉。文件上限 64 KB、最多 300 条。
- 往事和用户记忆完全分开：只读、不进数据库、记忆管理界面里看不到，也不会被记忆整理改动。
- 内容用自己的话概括原作设定，**不摘抄原作台词或百科原文**（仓库是公开的）；拿不准的细节宁可不写。
- 写完用 `cargo test character` 检查：每条往事都要能用它的第一个关键词想起来。

### 1.5 测试
`cargo test character` 里的 `all_builtin_packs_are_valid` 会检查所有内置角色包；新增内置角色时把 id 加进去。

## 2. 新增模型后端

**先判断能不能直接用「OpenAI 兼容接口」**：很多服务（国产模型、Ollama、LM Studio）兼容 OpenAI 格式，
用户在「设置 → 模型 → 添加一个配置 → OpenAI 兼容接口」填地址、模型名、价格即可，**不用写代码**。

需要专门支持时：
1. 查官方文档：接口地址、鉴权方式、流式格式、用量字段、错误码、价格。把查阅日期和链接写在新文件开头的注释里。
2. 新建 `src-tauri/src/backends/<名字>.rs`：
   - `pub fn kind() -> BackendKind`：显示名、是否需要 Key、默认地址/模型/思考强度/价格（美元/百万 token）、申请 Key 的网址、说明、能力声明、`create` 工厂函数。
   - 实现 `ChatBackend::stream_chat`：用 `http::post_stream` 发请求（Key 只放请求头并 `set_sensitive(true)`，**绝不放 URL**），
     用 `http::read_sse` 读流，每段正文调用 `on_delta`，把用量填进 `Usage`（`input_tokens` 是全部输入，含缓存部分）。
   - 用 `req.messages_for_api()` 取消息（它会把每轮变化的背景资料附在最后一条用户消息前）；系统提示词用 `req.system`。
   - 思考/推理内容不要显示给用户。
3. 在 `backends/mod.rs`：`pub mod <名字>;` 并在 `kinds()` 列表里加一行 `<名字>::kind()`。
4. 测试（必须，不联网）：参照 `anthropic.rs` / `gemini.rs` 的测试——请求体构造、流式解析、用 `http::fake_server` 起本地假服务器验证请求头和完整流程、错误码归类。
5. `npm run check` 通过后提交；在 DECISIONS 里记录默认值的来源。

## 3. 新增前端可调用的命令
1. 在 `src-tauri/src/commands.rs`（或 `settings_cmds.rs`）写 `#[tauri::command]`，只做参数校验和转发。
2. 在 `src-tauri/src/lib.rs` 的 `generate_handler!` 里登记。
3. 在 `src-tauri/build.rs` 的 `COMMANDS` 里登记。
4. 只给需要它的窗口授权：`src-tauri/capabilities/<窗口>.json` 加 `allow-<命令名，下划线换成短横线>`。
5. 前端在 `src/lib/ipc.ts` 加封装，组件只通过它调用（方便测试替换）。

## 4. 新增功能的一般步骤
1. 在 ROADMAP 里加任务。
2. 有重要取舍时写 DECISIONS。
3. 先写测试（Rust 用 mock 后端），再实现。
4. 涉及安全的改动同步更新 SECURITY.md。
5. 小步提交，提交前 `npm run check`。
