// 端到端测试：启动一个隔离的 DeskMate（测试模式，独立数据目录/凭据/WebView 数据），
// 通过 WebView2 调试端口（Chrome DevTools Protocol）驱动真实界面：点击按钮、输入、检查结果。
// 不模拟系统鼠标键盘，不影响正在使用电脑的人；不会调用任何付费接口（只用 mock 后端）。
//
// 用法：先 `npx tauri build --no-bundle`，然后 `npm run e2e`
// 注意：会短暂出现测试用的桌宠和设置窗口；运行前确认用户没有在全屏玩游戏。

import { spawn, spawnSync } from 'node:child_process';
import { mkdirSync, rmSync, writeFileSync, existsSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { tmpdir } from 'node:os';

const EXE = resolve(process.argv[2] ?? 'src-tauri/target/release/deskmate.exe');
const PORT = 9333;
const OUT = resolve('.e2e-out');
const DATA = join(tmpdir(), `deskmate-e2e-${Date.now()}`);
// 任务文件夹不能在 AppData 里（DeskMate 会拒绝系统目录），所以放到 D 盘根目录下的临时文件夹（测试后删除）
const TASK_ROOT = existsSync('D:/') ? `D:/DeskMate-E2E-${Date.now()}` : join(tmpdir(), `deskmate-e2e-work-${Date.now()}`);

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const results = [];

// ---------------- CDP ----------------

class Page {
  constructor(name, ws) {
    this.name = name;
    this.ws = ws;
    this.seq = 0;
    this.pending = new Map();
    this.errors = [];
    this.dialogs = [];
    ws.onmessage = (m) => {
      const msg = JSON.parse(m.data);
      if (msg.id && this.pending.has(msg.id)) {
        const p = this.pending.get(msg.id);
        this.pending.delete(msg.id);
        msg.error ? p.reject(new Error(msg.error.message)) : p.resolve(msg.result);
        return;
      }
      if (msg.method === 'Runtime.exceptionThrown') {
        this.errors.push(msg.params.exceptionDetails?.exception?.description ?? msg.params.exceptionDetails?.text);
      } else if (msg.method === 'Runtime.consoleAPICalled' && msg.params.type === 'error') {
        this.errors.push(msg.params.args.map((a) => a.value ?? a.description).join(' '));
      } else if (msg.method === 'Log.entryAdded' && msg.params.entry.level === 'error') {
        const t = msg.params.entry.text ?? '';
        if (!t.includes('favicon')) this.errors.push(t);
      } else if (msg.method === 'Page.javascriptDialogOpening') {
        this.dialogs.push(msg.params.message);
        this.send('Page.handleJavaScriptDialog', { accept: true }).catch(() => {});
      }
    };
  }
  static async open(name, wsUrl) {
    const ws = new WebSocket(wsUrl);
    await new Promise((res, rej) => {
      ws.onopen = res;
      ws.onerror = rej;
    });
    const p = new Page(name, ws);
    await p.send('Runtime.enable');
    await p.send('Log.enable');
    await p.send('Page.enable');
    return p;
  }
  send(method, params = {}) {
    const id = ++this.seq;
    this.ws.send(JSON.stringify({ id, method, params }));
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      setTimeout(() => {
        if (this.pending.has(id)) {
          this.pending.delete(id);
          reject(new Error(`CDP 超时：${method}`));
        }
      }, 20000);
    });
  }
  async eval(expr) {
    const r = await this.send('Runtime.evaluate', { expression: expr, awaitPromise: true, returnByValue: true });
    if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description ?? r.exceptionDetails.text);
    return r.result.value;
  }
  text() {
    return this.eval('document.body.innerText');
  }
  async waitFor(expr, what, timeout = 6000) {
    const end = Date.now() + timeout;
    while (Date.now() < end) {
      try {
        if (await this.eval(expr)) return;
      } catch {
        /* 页面可能在刷新 */
      }
      await sleep(120);
    }
    throw new Error(`等待超时：${what}`);
  }
  waitText(s, timeout) {
    return this.waitFor(`document.body.innerText.includes(${JSON.stringify(s)})`, `出现「${s}」`, timeout);
  }
  waitNoText(s, timeout) {
    return this.waitFor(`!document.body.innerText.includes(${JSON.stringify(s)})`, `「${s}」消失`, timeout);
  }
  /** 点击：优先 CSS 选择器，否则按按钮文字找 */
  async click(target, scope = 'document') {
    const ok = await this.eval(`(() => {
      const t = ${JSON.stringify(target)};
      const root = ${scope};
      let el = null;
      try { el = root.querySelector(t); } catch {}
      if (!el) el = [...root.querySelectorAll('button, [role=tab], label, summary')].find(b => b.innerText.trim() === t)
        || [...root.querySelectorAll('button')].find(b => b.innerText.trim().startsWith(t));
      if (!el) return false;
      el.click();
      return true;
    })()`);
    if (!ok) throw new Error(`找不到可以点击的「${target}」`);
    await sleep(150);
  }
  /** 给输入框 / 文本框 / 下拉框设置值并触发 Preact 监听的事件 */
  async input(selector, value) {
    const ok = await this.eval(`(() => {
      const el = document.querySelector(${JSON.stringify(selector)});
      if (!el) return false;
      const proto = el.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype : el.tagName === 'SELECT' ? HTMLSelectElement.prototype : HTMLInputElement.prototype;
      Object.getOwnPropertyDescriptor(proto, 'value').set.call(el, ${JSON.stringify(value)});
      el.dispatchEvent(new Event('input', { bubbles: true }));
      el.dispatchEvent(new Event('change', { bubbles: true }));
      return true;
    })()`);
    if (!ok) throw new Error(`找不到输入框 ${selector}`);
    await sleep(120);
  }
  async key(selector, init) {
    await this.eval(`(() => {
      const el = ${selector === 'window' ? 'window' : `document.querySelector(${JSON.stringify(selector)})`};
      el.dispatchEvent(new KeyboardEvent('keydown', Object.assign({ bubbles: true, cancelable: true }, ${JSON.stringify(init)})));
    })()`);
    await sleep(150);
  }
  async shot(file) {
    const r = await this.send('Page.captureScreenshot', { format: 'png' });
    writeFileSync(join(OUT, file), Buffer.from(r.data, 'base64'));
  }
  close() {
    try {
      this.ws.close();
    } catch {}
  }
}

async function targets() {
  try {
    const r = await fetch(`http://127.0.0.1:${PORT}/json/list`);
    return (await r.json()).filter((t) => t.type === 'page');
  } catch {
    return [];
  }
}

async function waitTarget(file, timeout = 15000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) {
    const t = (await targets()).find((x) => x.url.includes(file));
    if (t) return t;
    await sleep(250);
  }
  return null;
}

function windows(pid, close = '') {
  const args = ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', resolve('scripts/e2e/window.ps1'), '-ProcessId', String(pid)];
  if (close) args.push('-Close', close);
  const r = spawnSync('powershell', args, { encoding: 'utf8' });
  const map = {};
  for (const line of (r.stdout ?? '').split(/\r?\n/)) {
    const [w, v] = line.trim().split('|');
    if (w) map[w] = v;
  }
  return map;
}

async function waitVisible(pid, which, want, timeout = 5000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) {
    const v = windows(pid)[which];
    if ((v === '1') === want) return;
    await sleep(200);
  }
  // 诊断：列出这个进程的全部窗口（同名窗口可能不止一个）
  const r = spawnSync('powershell', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', resolve('scripts/e2e/window.ps1'), '-ProcessId', String(pid)], { encoding: 'utf8' });
  throw new Error(`${which} 窗口应该${want ? '显示' : '隐藏'}，但没有（窗口：${(r.stdout ?? '').trim().split(/\r?\n/).join(' ')}）`);
}

async function test(name, fn) {
  const t0 = Date.now();
  try {
    await fn();
    results.push({ name, ok: true, ms: Date.now() - t0 });
    console.log(`  ✓ ${name}`);
  } catch (e) {
    results.push({ name, ok: false, msg: String(e?.message ?? e) });
    console.log(`  ✗ ${name}：${e?.message ?? e}`);
  }
}

// ---------------- 主流程 ----------------

async function main() {
  if (!existsSync(EXE)) {
    console.error(`找不到 ${EXE}，请先 npx tauri build --no-bundle`);
    process.exit(2);
  }
  mkdirSync(OUT, { recursive: true });
  mkdirSync(DATA, { recursive: true });
  mkdirSync(TASK_ROOT, { recursive: true });
  console.log(`数据目录：${DATA}`);
  const child = spawn(EXE, ['--settings'], {
    env: { ...process.env, DESKMATE_DATA_DIR: DATA, DESKMATE_CDP_PORT: String(PORT) },
    stdio: 'ignore',
  });
  const pid = child.pid;
  let settings, chat, pet;
  try {
    const ts = await waitTarget('settings.html');
    const tp = await waitTarget('pet.html');
    const tc = await waitTarget('chat.html');
    if (!ts || !tp || !tc) throw new Error('窗口没有全部出现（检查是否有正式版 DeskMate 占用，或调试端口没打开）');
    settings = await Page.open('settings', ts.webSocketDebuggerUrl);
    pet = await Page.open('pet', tp.webSocketDebuggerUrl);
    chat = await Page.open('chat', tc.webSocketDebuggerUrl);
    await settings.waitFor(`!!document.querySelector('.nav-item')`, '设置页加载');

    console.log('\n设置窗口');
    const tabs = [
      ['常规', '常规'],
      ['陪伴', '陪伴'],
      ['提醒', '提醒'],
      ['活动感知', '活动感知'],
      ['声音', '声音'],
      ['角色', '角色'],
      ['日记', '日记'],
      ['记忆', '记忆'],
      ['模型', '模型'],
      ['用量与花费', '用量与花费'],
      ['高级', '高级'],
      ['关于与隐私', '关于与隐私'],
    ];
    for (const [label, h1] of tabs) {
      await test(`切换到「${label}」页`, async () => {
        await settings.click(label);
        await settings.waitFor(`document.querySelector('main h1')?.innerText === ${JSON.stringify(h1)}`, `${label} 标题`);
        await sleep(300);
        await settings.shot(`settings-${label}.png`);
      });
    }

    await settings.click('模型');
    await test('模型：测试 mock 连接', async () => {
      await settings.eval(`(() => { const c = [...document.querySelectorAll('.card.backend')].find(c => c.innerText.includes('Mock')); [...c.querySelectorAll('button')].find(b => b.innerText.includes('测试连接')).click(); })()`);
      await settings.waitText('连接成功');
    });
    await test('模型：保存 Key 后只显示末 4 位，清除 Key', async () => {
      await settings.eval(`[...document.querySelectorAll('.card-head')].find(h => h.innerText.startsWith('DeepSeek')).click()`);
      await sleep(200);
      await settings.input('input[aria-label="DeepSeek 的 API Key"]', 'e2e-test-key-1234567890');
      await settings.eval(`[...document.querySelectorAll('.card.backend')].find(c => c.innerText.startsWith('DeepSeek')).querySelectorAll('button').forEach(b => { if (b.innerText === '保存 Key') b.click(); })`);
      await settings.waitText('已保存 ••••7890');
      if ((await settings.text()).includes('e2e-test-key-1234567890')) throw new Error('完整 Key 出现在界面上');
      const before = settings.dialogs.length;
      await settings.eval(`[...document.querySelectorAll('.card.backend')].find(c => c.innerText.startsWith('DeepSeek')).querySelectorAll('button').forEach(b => { if (b.innerText === '清除') b.click(); })`);
      await sleep(300);
      if (settings.dialogs.length === before) throw new Error('点「清除」没有弹出确认框');
      await settings.waitNoText('已保存 ••••7890');
    });
    await test('模型：添加并删除一个自定义配置', async () => {
      await settings.input('select[aria-label="新配置类型"]', 'openai_compat');
      await settings.click('添加');
      await settings.waitText('OpenAI 兼容接口（自定义） 2');
      await settings.eval(`[...document.querySelectorAll('.card-head')].find(h => h.innerText.includes('OpenAI 兼容接口（自定义） 2')).click()`);
      await sleep(200);
      await settings.eval(`[...document.querySelectorAll('.card.backend')].find(c => c.innerText.includes('OpenAI 兼容接口（自定义） 2')).querySelectorAll('button').forEach(b => { if (b.innerText === '删除') b.click(); })`);
      await settings.waitNoText('OpenAI 兼容接口（自定义） 2');
    });
    await test('模型：「Claude Code（用订阅额度）」卡片显示程序位置而不是接口地址（不点测试，不花额度）', async () => {
      await settings.eval(`[...document.querySelectorAll('.card-head')].find(h => h.innerText.startsWith('Claude Code（用订阅额度）')).click()`);
      await sleep(200);
      const t = await settings.eval(`[...document.querySelectorAll('.card.backend')].find(c => c.innerText.startsWith('Claude Code（用订阅额度）')).innerText`);
      if (!t.includes('claude 程序位置') || t.includes('接口地址')) throw new Error('字段名不对');
      if (t.includes('未填 Key')) throw new Error('不需要 Key 的后端显示了「未填 Key」');
      await settings.eval(`[...document.querySelectorAll('.card-head')].find(h => h.innerText.startsWith('Claude Code（用订阅额度）')).click()`);
    });
    await test('模型：切换日常聊天模型', async () => {
      await settings.input('select[aria-label="日常聊天使用的模型"]', 'deepseek');
      await settings.waitFor(`document.querySelector('select[aria-label="日常聊天使用的模型"]').value === 'deepseek'`, '选中 deepseek');
      await settings.input('select[aria-label="日常聊天使用的模型"]', 'mock');
      await settings.waitFor(`document.querySelector('select[aria-label="日常聊天使用的模型"]').value === 'mock'`, '选回 mock');
    });

    await settings.click('常规');
    await test('常规：关闭再打开「总在最前」', async () => {
      await settings.eval(`[...document.querySelectorAll('label.row')].find(l => l.innerText.includes('总在最前')).querySelector('input').click()`);
      await settings.waitText('已保存');
      await settings.eval(`[...document.querySelectorAll('label.row')].find(l => l.innerText.includes('总在最前')).querySelector('input').click()`);
    });
    await test('高级：「显卡加速」默认关，切换后提示重启生效', async () => {
      await settings.click('高级');
      await settings.waitFor(`document.querySelector('main h1')?.innerText === '高级'`, '高级页');
      const box = `[...document.querySelectorAll('label.row')].find(l => l.innerText.includes('显卡加速')).querySelector('input')`;
      if (await settings.eval(`${box}.checked`)) throw new Error('默认应该关闭');
      await settings.eval(`${box}.click()`);
      await settings.waitText('退出 DeskMate 再打开后生效');
      await settings.eval(`${box}.click()`);
      await settings.waitNoText('退出 DeskMate 再打开后生效');
      await settings.click('常规');
      await settings.waitFor(`document.querySelector('main h1')?.innerText === '常规'`, '常规页');
    });
    await test('常规：非法快捷键会提示，合法快捷键能保存', async () => {
      await settings.input('input[aria-label="快捷键"]', 'D');
      await settings.click('应用');
      await settings.waitText('快捷键需要包含');
      await settings.input('input[aria-label="快捷键"]', 'Ctrl+Alt+F9');
      await settings.click('应用');
      await settings.waitFor(`[...document.querySelectorAll('.status')].some(s => s.innerText === '已保存')`, '保存成功');
    });

    await settings.click('角色');
    await test('角色：切换角色并保存禁词', async () => {
      await settings.waitFor(`document.querySelectorAll('.char-card').length >= 8`, '8 个角色');
      await settings.eval(`[...document.querySelectorAll('.char-card')].find(c => c.innerText.includes('奶龙')).querySelector('button.btn').click()`);
      await settings.waitFor(`[...document.querySelectorAll('.char-card')].find(c => c.innerText.includes('奶龙'))?.innerText.includes('使用中')`, '奶龙使用中');
      await settings.eval(`[...document.querySelectorAll('.char-card')].find(c => c.innerText.includes('奶龙')).querySelector('.char-main').click()`);
      await sleep(200);
      await settings.eval(`(() => { const ta = [...document.querySelectorAll('label')].find(l => l.innerText.startsWith('奶龙 的禁词')).querySelector('textarea'); Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(ta, '笨蛋'); ta.dispatchEvent(new Event('input', { bubbles: true })); })()`);
      await settings.click('保存');
      await settings.waitText('已保存');
    });
    await test('角色：放进自己的图片后，桌宠按地址读取并显示（不再整张图转文本传给界面）', async () => {
      const assets = join(DATA, 'characters', 'nailong', 'assets');
      mkdirSync(assets, { recursive: true });
      // 1×1 像素的 PNG
      const png = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=';
      writeFileSync(join(assets, 'idle.png'), Buffer.from(png, 'base64'));
      await settings.click('重新加载');
      await settings.waitText('已重新加载');
      await pet.waitFor(
        `(() => { const i = document.querySelector('.pet-img'); return !!i && i.src.startsWith('http://deskmate-img.localhost/nailong/idle?v=') && i.complete && i.naturalWidth === 1; })()`,
        '桌宠显示用户图片',
      );
      if (await pet.eval(`document.querySelector('.pet').classList.contains('placeholder')`)) throw new Error('仍是占位形象');
      const preview = await settings.eval(`[...document.querySelectorAll('.char-card')].find(c => c.innerText.includes('奶龙')).querySelector('img').src`);
      if (!preview.startsWith('http://deskmate-img.localhost/nailong/idle?v=')) throw new Error(`角色页预览地址不对：${preview.slice(0, 60)}`);
      rmSync(assets, { recursive: true, force: true });
      await settings.click('重新加载');
      await pet.waitFor(`document.querySelector('.pet-img')?.src.startsWith('data:image/svg+xml')`, '删掉图片后回到占位形象');
    });

    await settings.click('记忆');
    await test('记忆：手动添加、编辑、删除', async () => {
      await settings.input('input[aria-label="新记忆"]', '我对花生过敏');
      await settings.click('添加');
      await settings.waitText('我对花生过敏');
      await settings.click('编辑', `document.querySelector('.memory-list')`);
      await settings.input('textarea[aria-label="记忆内容"]', '用户对花生过敏，吃了会起疹子');
      await settings.click('保存', `document.querySelector('.memory-list')`);
      await settings.waitText('吃了会起疹子');
      const before = settings.dialogs.length;
      await settings.click('删除', `document.querySelector('.memory-list')`);
      await sleep(300);
      if (settings.dialogs.length === before) throw new Error('点「删除」没有弹出确认框');
      await settings.waitNoText('吃了会起疹子');
    });
    await test('记忆：输入「清空」后清空', async () => {
      await settings.input('input[aria-label="新记忆"]', '测试用的记忆');
      await settings.click('添加');
      await settings.waitText('测试用的记忆');
      await settings.input('input[aria-label="清空确认"]', '清空');
      await settings.click('清空全部');
      await settings.waitNoText('测试用的记忆');
    });
    await test('记忆：暂停记忆开关', async () => {
      await settings.eval(`[...document.querySelectorAll('label.row')].find(l => l.innerText.includes('暂停记忆')).querySelector('input').click()`);
      await settings.waitText('已更新');
      await settings.eval(`[...document.querySelectorAll('label.row')].find(l => l.innerText.includes('暂停记忆')).querySelector('input').click()`);
    });

    await settings.click('高级');
    await test('高级：委托外部 agent 默认关闭，打开后才显示任务执行设置', async () => {
      await settings.waitFor(`document.querySelector('main h1')?.innerText === '高级'`, '高级页');
      const on = await settings.eval(`[...document.querySelectorAll('label.row')].find(l => l.innerText.includes('允许她把事情交给')).querySelector('input').checked`);
      if (on) throw new Error('委托外部 agent 应该默认关闭');
      if (await settings.eval(`!!document.querySelector('input[aria-label="任务文件夹"]')`)) throw new Error('没开委托时不该显示任务设置');
      await settings.eval(`[...document.querySelectorAll('label.row')].find(l => l.innerText.includes('允许她把事情交给')).querySelector('input').click()`);
      await settings.waitText('已保存');
    });
    await test('任务执行：检测 CLI 并保存任务文件夹', async () => {
      await settings.waitFor(`!!document.querySelector('input[aria-label="任务文件夹"]')`, '检测完成', 30000);
      await settings.input('input[aria-label="任务文件夹"]', TASK_ROOT);
      await settings.click('保存');
      await settings.waitText('已保存');
    });

    await test('任务执行：能检测到本机已安装的 Claude Code / Codex', async () => {
      const has = (n) =>
        (process.env.PATH ?? '')
          .split(';')
          .concat(process.env.APPDATA ? [join(process.env.APPDATA, 'npm')] : [])
          .some((d) => d && ['exe', 'cmd'].some((e) => existsSync(join(d, `${n}.${e}`))));
      const text = await settings.text();
      if (has('claude') && !/Claude Code\s+已找到/.test(text)) throw new Error('本机装了 claude，但界面显示未找到');
      if (has('codex') && !/Codex\s+已找到/.test(text)) throw new Error('本机装了 codex，但界面显示未找到');
    });

    await settings.click('用量与花费');
    await test('用量：保存花费上限', async () => {
      await settings.waitFor(`!!document.querySelector('.meter')`, '用量加载');
      await settings.click('保存');
      await settings.waitText('已保存');
    });

    console.log('\n聊天窗口与桌宠');
    await test('点桌宠打开聊天窗口', async () => {
      await pet.eval(`(() => { const el = document.querySelector('[data-testid=pet]'); el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, screenX: 10, screenY: 10, clientX: 80, clientY: 150 })); el.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, button: 0, screenX: 10, screenY: 10, clientX: 80, clientY: 150 })); })()`);
      await waitVisible(pid, 'chat', true);
    });
    await test('发送消息并收到 mock 回复', async () => {
      await chat.input('textarea[aria-label="输入消息"]', '你好，我叫测试员');
      await chat.key('textarea[aria-label="输入消息"]', { key: 'Enter' });
      await chat.waitText('mock 模式', 10000);
      await chat.waitFor(`!document.querySelector('[aria-label="停止生成"]')`, '生成结束', 10000);
      await chat.shot('chat-reply.png');
    });
    await test('气泡高度跟着内容走：内容放得下、不留大块空白', async () => {
      // 窗口高度 ≈ 面板高度 + 上下留白 12px；气泡区不需要滚动
      await chat.waitFor(
        `(() => { const p = document.querySelector('.panel'); const a = document.querySelector('.bubble-area');
          return a.scrollHeight <= a.clientHeight + 1 && Math.abs(window.innerHeight - (p.offsetHeight + 12)) <= 3; })()`,
        '窗口贴合内容',
      );
      await sleep(200);
      await chat.shot('chat-bubble-fit.png');
      const h = await chat.eval('window.innerHeight');
      if (h < 120 || h >= 250) throw new Error(`三行回复的气泡高度应在 120~250 之间，实际 ${h}`);
    });
    await test('输入法组字时按 Enter 不发送', async () => {
      await chat.input('textarea[aria-label="输入消息"]', 'nihao');
      await chat.key('textarea[aria-label="输入消息"]', { key: 'Enter', isComposing: true });
      await sleep(400);
      const v = await chat.eval(`document.querySelector('textarea[aria-label="输入消息"]').value`);
      if (v !== 'nihao') throw new Error('组字中的 Enter 把消息发出去了');
      await chat.input('textarea[aria-label="输入消息"]', '');
    });
    await test('展开 / 收起聊天面板', async () => {
      await chat.click('[aria-label="展开"]');
      await chat.waitFor(`!!document.querySelector('.panel.mode-expanded')`, '展开');
      await chat.shot('chat-expanded.png');
      await chat.click('[aria-label="收起"]');
      await chat.waitFor(`!!document.querySelector('.panel.mode-compact')`, '收起');
    });
    await test('气泡里的「打开」按钮：只显示，不自动打开', async () => {
      // 不点按钮：点了会在用户电脑上真的打开浏览器
      await chat.input('textarea[aria-label="输入消息"]', '/mock open https://example.com');
      await chat.key('textarea[aria-label="输入消息"]', { key: 'Enter' });
      await chat.waitFor(`[...document.querySelectorAll('.action')].some(a => a.innerText.includes('https://example.com'))`, '出现打开按钮', 10000);
      const shown = await chat.eval(`document.querySelector('.bubble-area, .messages').innerText`);
      if (shown.includes('[[打开')) throw new Error('标记原文不该显示出来');
      await chat.shot('chat-action.png');
    });
    await test('提醒：聊天里吩咐 → 复述 → 点「好」保存，设置「提醒」页能看到', async () => {
      await chat.input('textarea[aria-label="输入消息"]', '半小时后提醒我收衣服');
      await chat.key('textarea[aria-label="输入消息"]', { key: 'Enter' });
      await chat.waitFor(`!!document.querySelector('.reminder-card .reminder-buttons')`, '确认卡片', 10000);
      const what = await chat.eval(`document.querySelector('.reminder-what').innerText`);
      if (!what.includes('30 分钟后') || !what.includes('收衣服')) throw new Error(`复述不对：${what}`);
      await chat.click('好');
      // 保存后角色接一句（气泡只显示最新一条，确认卡片那条就看不到了）
      await chat.waitFor(`!document.querySelector('.reminder-buttons') && !!document.querySelector('.bubble-area .msg-assistant')`, '保存后角色接话', 5000);
      await chat.shot('chat-reminder.png');
      await settings.click('提醒');
      await settings.waitFor(`document.querySelector('main h1')?.innerText === '提醒'`, '提醒页');
      await settings.waitText('收衣服');
      await settings.shot('settings-提醒-有内容.png');
    });
    await test('每日播报：设置里点「现在播报一次」，气泡里出现播报（没有模型时用台词池）', async () => {
      await settings.click('陪伴');
      await settings.waitFor(`document.querySelector('main h1')?.innerText === '陪伴'`, '陪伴页');
      const before = await chat.eval(`document.querySelector('.panel').innerText`);
      await settings.click('现在播报一次');
      await chat.waitFor(`document.querySelector('.panel').innerText !== ${JSON.stringify(before)}`, '播报出现', 15000);
      await waitVisible(pid, 'chat', true);
      await chat.shot('chat-briefing.png');
    });
    await test('小游戏：点 🎮 打开，猜拳玩一局，回到聊天', async () => {
      await chat.click('[aria-label="小游戏"]');
      await chat.waitFor(`!!document.querySelector('.game-list')`, '游戏列表', 10000);
      await chat.click('猜拳');
      await chat.waitFor(`!!document.querySelector('[aria-label="石头"]')`, '猜拳加载', 10000);
      for (let i = 0; i < 6 && !(await chat.eval(`!!document.querySelector('.rps button.btn')`)); i++) {
        await chat.click('[aria-label="石头"]');
      }
      await chat.waitFor(`!!document.querySelector('.game-line')`, '她说了话', 5000);
      await chat.shot('chat-game-rps.png');
      await chat.click('回到聊天');
      await chat.waitFor(`!!document.querySelector('textarea[aria-label="输入消息"]')`, '回到聊天');
    });
    await test('任务卡片能打开和关闭（开了委托后，回复里的任务标记弹出卡片）', async () => {
      await chat.input('textarea[aria-label="输入消息"]', '/mock task 统计字数');
      await chat.key('textarea[aria-label="输入消息"]', { key: 'Enter' });
      await chat.waitFor(`!!document.querySelector('.task-card')`, '任务卡片出现', 10000);
      await chat.waitFor(`!document.querySelector('.task-card').innerText.includes('正在检测')`, '检测完成', 30000);
      await chat.shot('task-card.png');
      await chat.click('先不做了');
      await chat.waitFor(`!document.querySelector('.task-card')`, '任务卡片关闭');
    });
    await test('聊天里说「来一局井字棋」：她回一句，直接打开井字棋', async () => {
      await chat.input('textarea[aria-label="输入消息"]', '来一局井字棋');
      await chat.key('textarea[aria-label="输入消息"]', { key: 'Enter' });
      await chat.waitFor(`!!document.querySelector('.ttt-board')`, '井字棋打开', 10000);
      await chat.shot('chat-intent-ttt.png');
      await chat.click('回到聊天');
      await chat.waitFor(`!!document.querySelector('textarea[aria-label="输入消息"]')`, '回到聊天');
    });
    await test('方块对战：打开、选模式、开始，两块棋盘在动、没有报错，再关掉', async () => {
      await chat.input('textarea[aria-label="输入消息"]', '玩方块对战');
      await chat.key('textarea[aria-label="输入消息"]', { key: 'Enter' });
      await chat.waitFor(`document.body.innerText.includes('PRESS START')`, '方块对战菜单', 10000);
      await chat.click('PRESS START');
      await chat.waitFor(`!!document.querySelector('canvas[aria-label="你的棋盘"]')`, '棋盘出现', 5000);
      await sleep(4500);
      const t = await chat.eval(`document.querySelector('.blocks-time').innerText`);
      await chat.shot('chat-game-blocks.png');
      if (!/^\d:\d\d$/.test(t)) throw new Error(`计时显示不对：${t}`);
      await chat.click('回到聊天');
      await chat.waitFor(`!!document.querySelector('textarea[aria-label="输入消息"]')`, '回到聊天');
    });
    await test('点 ✕ 关闭聊天窗口', async () => {
      await chat.click('[aria-label="关闭"]');
      await waitVisible(pid, 'chat', false);
    });
    await test('按 Esc 关闭聊天窗口', async () => {
      await pet.eval(`(() => { const el = document.querySelector('[data-testid=pet]'); el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, clientX: 80, clientY: 150 })); el.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, button: 0, clientX: 80, clientY: 150 })); })()`);
      await waitVisible(pid, 'chat', true);
      await chat.key('window', { key: 'Escape' });
      await waitVisible(pid, 'chat', false);
    });
    await test('再点一次桌宠能收起聊天窗口', async () => {
      const clickPet = `(() => { const el = document.querySelector('[data-testid=pet]'); el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, clientX: 80, clientY: 150 })); el.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, button: 0, clientX: 80, clientY: 150 })); })()`;
      await pet.eval(clickPet);
      await waitVisible(pid, 'chat', true);
      await pet.eval(clickPet);
      await waitVisible(pid, 'chat', false);
    });

    console.log('\n鼠标交互与气泡菜单');
    const petAt = (button, y) =>
      pet.eval(`(() => { const el = document.querySelector('[data-testid=pet]'); el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: ${button}, screenX: 10, screenY: 10, clientX: 80, clientY: ${y} })); el.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, button: ${button}, screenX: 10, screenY: 10, clientX: 80, clientY: ${y} })); })()`);
    await test('点头顶是摸头：只弹「她说的话」气泡（没有输入框），点一下变成聊天框', async () => {
      await waitVisible(pid, 'chat', false);
      await petAt(0, 15);
      await chat.waitFor(`!!document.querySelector('.panel.speech')`, '她说的话气泡', 6000);
      if (await chat.eval(`!!document.querySelector('textarea[aria-label="输入消息"]')`)) throw new Error('摸头不该弹出输入框');
      await chat.shot('chat-speech.png');
      await chat.click('[data-testid=speech]');
      await chat.waitFor(`!!document.querySelector('textarea[aria-label="输入消息"]')`, '变成聊天框');
      await chat.click('[aria-label="关闭"]');
      await waitVisible(pid, 'chat', false);
    });
    await test('右键：头顶浮出气泡菜单，点「一起玩」打开游戏列表', async () => {
      await petAt(2, 150);
      const tm = await waitTarget('menu.html', 8000);
      if (!tm) throw new Error('气泡菜单页面没有出现');
      const menu = await Page.open('menu', tm.webSocketDebuggerUrl);
      try {
        await menu.waitFor(`document.querySelectorAll('.bubble').length >= 7`, '气泡', 5000);
        await waitVisible(pid, 'menu', true);
        await menu.shot('menu.png');
        await menu.click('[data-id=games]');
        await waitVisible(pid, 'menu', false);
        await chat.waitFor(`!!document.querySelector('.game-list')`, '游戏列表', 8000);
        await chat.click('回到聊天');
        await chat.click('[aria-label="关闭"]');
        if (menu.errors.length) throw new Error(`菜单页面报错：${menu.errors.join(' | ')}`);
      } finally {
        menu.close();
      }
    });
    await test('声音：选 Windows 自带的语音后出现试听；桌宠页面能拿到系统声音列表（不出声）', async () => {
      await settings.click('声音');
      await settings.waitFor(`document.querySelector('main h1')?.innerText === '声音'`, '声音页');
      await settings.eval(`[...document.querySelectorAll('label.engine-choice')].find(l => l.innerText.includes('Windows')).querySelector('input').click()`);
      await settings.waitText('试听一句');
      await settings.waitText('她说话的语言');
      const ja = await settings.eval(`[...document.querySelectorAll('input[name=voice-lang]')].find(i => i.checked)?.closest('label')?.innerText.startsWith('日语') === true`);
      if (!ja) throw new Error('说话的语言默认不是日语（DECISIONS #58）');
      await settings.shot('settings-声音.png');
      const n = await pet.eval(`typeof speechSynthesis === 'undefined' ? -1 : speechSynthesis.getVoices().length`);
      const nja = await pet.eval(`typeof speechSynthesis === 'undefined' ? -1 : speechSynthesis.getVoices().filter(v => v.lang.toLowerCase().startsWith('ja')).length`);
      console.log(`    （桌宠页面的系统声音：${n} 个，其中日语 ${nja} 个）`);
      if (n < 0) throw new Error('WebView2 里没有 speechSynthesis');
    });
    await test('模型页：有联网搜索开关，默认开着（DECISIONS #59）', async () => {
      await settings.click('模型');
      await settings.waitFor(`document.querySelector('main h1')?.innerText === '模型'`, '模型页');
      await settings.waitText('允许她上网搜索');
      const on = await settings.eval(`[...document.querySelectorAll('label.row')].find(l => l.innerText.includes('允许她上网搜索'))?.querySelector('input')?.checked === true`);
      if (!on) throw new Error('联网搜索默认应该是开着的');
      await settings.shot('settings-模型.png');
    });
    await test('换成七海千秋：桌宠和聊天窗口换上掌机风皮肤', async () => {
      await settings.click('角色');
      await settings.waitFor(`document.body.innerText.includes('七海千秋')`, '角色列表');
      await settings.eval(`(() => { const c = [...document.querySelectorAll('.char-card')].find(c => c.innerText.includes('七海千秋')); const b = c && [...c.querySelectorAll('button')].find(b => b.innerText.trim() === '切换'); if (b) b.click(); })()`);
      await pet.waitFor(`document.documentElement.dataset.skin === 'arcade'`, '桌宠皮肤', 6000);
      await pet.waitFor(`(document.querySelector('.pet-img')?.getAttribute('src') ?? '').startsWith('data:image/svg+xml')`, '千秋的 Q 版形象', 6000);
      await sleep(300);
      await pet.shot('pet-chiaki.png');
      await chat.waitFor(`document.documentElement.dataset.skin === 'arcade'`, '聊天皮肤', 6000);
      await petAt(0, 15);
      await chat.waitFor(`!!document.querySelector('.panel.speech')`, '她说的话气泡', 6000);
      await sleep(300);
      await chat.shot('chat-speech-arcade.png');
      // 摸头时换成「被摸头」的图
      const src = await pet.eval(`document.querySelector('.pet-img').getAttribute('src')`);
      await pet.shot('pet-chiaki-pat.png');
      if (!src.startsWith('data:image/svg+xml')) throw new Error('摸头时应该换成反应图');
      await chat.click('[data-testid=speech]');
      await chat.waitFor(`!!document.querySelector('textarea[aria-label="输入消息"]')`, '变成聊天框');
      await chat.shot('chat-arcade.png');
      await chat.click('[aria-label="关闭"]');
    });

    console.log('\n窗口关闭');
    await test('点设置窗口标题栏的 ✕（WM_CLOSE）能关掉设置窗口', async () => {
      windows(pid, 'settings');
      const end = Date.now() + 5000;
      while (Date.now() < end && (await targets()).some((t) => t.url.includes('settings.html'))) await sleep(200);
      if ((await targets()).some((t) => t.url.includes('settings.html'))) throw new Error('设置窗口没有关掉');
    });
    await test('关掉后还能从聊天窗口重新打开设置', async () => {
      await pet.eval(`(() => { const el = document.querySelector('[data-testid=pet]'); el.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, clientX: 80, clientY: 150 })); el.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, button: 0, clientX: 80, clientY: 150 })); })()`);
      await waitVisible(pid, 'chat', true);
      await chat.click('[aria-label="设置"]');
      const t = await waitTarget('settings.html', 8000);
      if (!t) throw new Error('设置窗口没有重新打开');
      await waitVisible(pid, 'settings', true);
    });
    await test('聊天窗口收到 WM_CLOSE 时只隐藏、程序不退出', async () => {
      windows(pid, 'chat');
      await waitVisible(pid, 'chat', false);
      if (child.exitCode !== null) throw new Error('程序退出了');
    });
  } catch (e) {
    results.push({ name: '测试准备', ok: false, msg: String(e?.message ?? e) });
    console.log(`  ✗ 测试准备：${e?.message ?? e}`);
  } finally {
    for (const p of [settings, chat, pet]) {
      if (p && p.errors.length) {
        results.push({ name: `${p.name} 页面没有前端报错`, ok: false, msg: p.errors.slice(0, 5).join(' | ') });
        console.log(`  ✗ ${p.name} 页面有前端报错：${p.errors.slice(0, 5).join(' | ')}`);
      }
      p?.close();
    }
    spawnSync('taskkill', ['/PID', String(pid), '/T', '/F'], { stdio: 'ignore' });
    await sleep(500);
    try {
      rmSync(DATA, { recursive: true, force: true });
      rmSync(TASK_ROOT, { recursive: true, force: true });
    } catch {}
  }
  const failed = results.filter((r) => !r.ok);
  writeFileSync(join(OUT, 'report.json'), JSON.stringify(results, null, 2));
  console.log(`\n共 ${results.length} 项，失败 ${failed.length} 项。截图和报告在 ${OUT}`);
  process.exit(failed.length ? 1 : 0);
}

main();
