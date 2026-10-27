# 进度日志

> 最新条目在最上面。超过 300 行时把旧条目压缩成摘要移到 `docs/progress-archive/`。

## 2026-10-27 16:45（用户要求暂停，等用户说「继续」再接着做）
- 完成：
  - M0 骨架：Tauri 2 + Preact + Vite 项目、gitleaks pre-commit 钩子（已验证能拦截假 Key）、CI 配置、`npm run check`。
  - M1 桌宠窗口核心：透明 pet 窗口（默认右下角）、拖动后吸附边缘并保存位置、四角停靠、托盘与右键菜单、聊天气泡/面板、mock 流式输出、停止/重新生成/复制、角色口吻错误提示 + 查看详情、断网提示、单实例。
  - M2 模型接入：后端注册表；DeepSeek / Gemini / Claude API / OpenAI 兼容四个后端（默认值查自官方文档，见各文件开头）；Key 存 Windows 凭据管理器；用量统计、每日/每月花费上限；设置界面（常规、模型、用量、关于）；`--settings` 启动参数。
  - 测试：Rust 91 个（含本地假服务器测三家接口）、前端 24 个，全部通过。
  - 在真实 WebView2 里手动验证过：桌宠透明显示、点击打开聊天气泡、微软拼音组字时 Enter 不发送、拖动吸附到左边缘后聊天窗口自动换到右侧、设置窗口显示正常。
- 实测（release，`scripts/measure.ps1`，数据目录用临时目录）：
  - 安装包 2.62 MiB（目标 < 15 MB ✅）；exe 8.7 MB。
  - 冷启动到桌宠窗口出现：0.46 ~ 0.73 秒（目标 < 2 秒 ✅）。
  - 空闲内存：总私有内存约 185 ~ 215 MB（主进程 7 MB，其余是 WebView2：GPU 进程约 95 MB、浏览器进程 38 MB、pet 与 chat 两个渲染进程各约 25 MB）。
  - 空闲 CPU：关掉待机动画时 0.26% 单核（≈0.01% 整机）✅；旧版连续 CSS 待机动画时 1.7 ~ 3.4% 单核 ❌ → 已改成「离散姿势切换」（DECISIONS #12），**改完还没重新实测**。
- 进行中：M2 收尾——重新实测离散姿势版本的空闲 CPU。release 构建被中途停止，需要重新构建。
- 下一步：
  1. `npx tauri build --no-bundle`（或 `npm run build:app`）→ 用 `scripts/measure.ps1` 测空闲 CPU（开启待机动画），目标 < 0.5% 单核；不达标就把间隔再拉长或默认关闭动画。
  2. 勾掉 ROADMAP 里 M1/M2 剩余项，开始 M3 角色系统（8 个角色设定 + 占位 SVG + 禁词/自定义要求设置界面 + 用户图片说明）。
  3. M5 可以考虑加「用 Claude Code / Codex 当聊天后端」的可选模式（用户问过，见 DECISIONS #14）。
- 已知问题：
  - 开发时自动化测试界面会动用户的鼠标键盘：用户在用电脑时**不要**做模拟点击/输入（一次模拟按键可能落到了用户的 VS Code 窗口）。要看界面就用 `--settings` 启动参数 + 按窗口标题截图（PrintWindow），不抢焦点。
  - 本机 git 全局配置了 `http.proxy=127.0.0.1:7897`，代理没开时 cargo 会卡住；已在 `D:\dev\cargo\config.toml` 设置 cargo 直连。将来 `git push` 如果失败，先检查这个代理。
  - 「打开数据目录」只在 Windows 上实现；keyring 只启用了 Windows/macOS 后端（Linux 以后再说）。
- 本次决策：见 DECISIONS.md #8 ~ #14

## 2026-10-27 15:30
- 完成：项目第一次启动。git 初始化、.gitignore、文档体系（AGENTS.md、CLAUDE.md、docs/*）。开发环境：安装 Rust 1.99.1（D:\dev）、gitleaks 8.30.1（D:\dev\tools）。
- 进行中：M0 —— 准备搭建 Tauri 2 + Preact 项目骨架。
- 下一步：Tauri 项目骨架、测试框架、pre-commit 钩子、CI。
- 已知问题：无。
- 本次决策：见 DECISIONS.md #1 ~ #7
