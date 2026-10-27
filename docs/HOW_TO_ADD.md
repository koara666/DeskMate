# 如何新增角色 / 模型后端 / 功能

> 本文件随实现更新。新增东西时，先读对应小节，按步骤做。

## 1. 新增角色（不需要改代码）

（M3 实现角色包加载后补全具体字段说明）

角色包结构：
```
characters/<角色id>/
  character.json   # 名字、语言、表情映射、作者、版本
  persona.md       # 性格设定 + 5~10 段原创示范对话
  assets/          # 仓库里只放原创占位 SVG；用户图片放用户数据目录
```

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
