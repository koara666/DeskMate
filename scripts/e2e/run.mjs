// 端到端测试：启动一个隔离的 DeskMate（测试模式，独立数据目录/凭据/WebView 数据），
// 通过 WebView2 调试端口（Chrome DevTools Protocol）驱动真实界面：点击按钮、输入、检查结果。
// 不模拟系统鼠标键盘，不影响正在使用电脑的人；不会调用任何付费接口（只用 mock 后端）。
//
// 用法：先 `npx tauri build --no-bundle`，然后 `npm run e2e`
// 注意：会短暂出现测试用的桌宠和设置窗口；运行前确认用户没有在全屏玩游戏。

import { spawn, spawnSync } from 'node:child_process';
import { createServer } from 'node:http';
import { mkdirSync, rmSync, writeFileSync, existsSync, readFileSync } from 'node:fs';
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

// ---------------- 角色包：本机假的发布页 ----------------
// 软件本体不带任何角色，用户在设置里挑哪位就下载哪位（src-tauri/src/charpacks.rs）。
// 测试里把下载地址指到本机，由这个小服务器把 dist-packs/ 里本机打好的包发出去（地址规则和 GitHub 发布页一样）。

function startPackServer() {
  const reg = JSON.parse(readFileSync(resolve('scripts/builtin_characters.json'), 'utf8'));
  const routes = new Map();
  for (const [id, c] of Object.entries(reg)) {
    if (id.startsWith('_') || !c.pack) continue;
    const file = resolve('dist-packs', `${id}.tar.bz2`);
    if (!existsSync(file)) {
      throw new Error(`没有 ${file}：先 python scripts/chibi/build_character_pack.py --all（形象和语音要先用 fetch_art.py / fetch_voice_pack.py 取到 characters/ 里）`);
    }
    routes.set(`/character-${c.short}-v${c.pack.version}/${id}.tar.bz2`, file);
  }
  const hits = [];
  const server = createServer((req, res) => {
    const file = routes.get(req.url ?? '');
    hits.push(req.url);
    if (!file) {
      res.writeHead(404).end();
      return;
    }
    const data = readFileSync(file);
    res.writeHead(200, { 'Content-Length': data.length, 'Content-Type': 'application/octet-stream' });
    // 分几次发，进度条才看得见在动
    let sent = 0;
    const step = Math.ceil(data.length / 8);
    const tick = () => {
      if (sent >= data.length) return res.end();
      res.write(data.subarray(sent, sent + step));
      sent += step;
      setTimeout(tick, 60);
    };
    tick();
  });
  return new Promise((ok) => server.listen(0, '127.0.0.1', () => ok({ port: server.address().port, hits, close: () => server.close() })));
}

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

// ---------------- 启动时的窗口竞态 ----------------
// 后台还在预建菜单窗口时就右键（装完新版马上去点桌宠），以前会各建一个同名窗口：
// 残留一个空白透明的盖在最上面，气泡上的点击全被它吞掉。另起一个隔离实例，启动后立刻连着开菜单，数菜单窗口。

function countWindows(pid, which) {
  const r = spawnSync('powershell', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', resolve('scripts/e2e/window.ps1'), '-ProcessId', String(pid)], { encoding: 'utf8' });
  return (r.stdout ?? '').split(/\r?\n/).filter((l) => l.trim().startsWith(`${which}|`)).length;
}

async function menuStartupRace() {
  const port = PORT + 1;
  const data = join(tmpdir(), `deskmate-e2e-race-${Date.now()}`);
  mkdirSync(data, { recursive: true });
  const child = spawn(EXE, [], { env: { ...process.env, DESKMATE_DATA_DIR: data, DESKMATE_CDP_PORT: String(port) }, stdio: 'ignore' });
  let pet;
  try {
    let tp;
    for (let i = 0; i < 300 && !tp; i++) {
      try {
        tp = (await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()).find((x) => x.url.includes('pet.html'));
      } catch {}
      await sleep(30);
    }
    if (!tp) throw new Error('第二个测试实例的桌宠页面没有出现');
    pet = await Page.open('race-pet', tp.webSocketDebuggerUrl);
    // 页面刚出现时 Tauri 的接口可能还没注入：等它好了再连着开（仍然远早于启动后 800ms 的菜单预建）
    await pet.waitFor(`typeof window.__TAURI_INTERNALS__?.invoke === 'function'`, '第二个实例的 Tauri 接口', 5000);
    const calls = [];
    for (let i = 0; i < 60; i++) {
      // 每一次都自己接住错误：没接住的失败会让整个测试进程直接退出
      calls.push(pet.eval(`window.__TAURI_INTERNALS__.invoke('menu_open', { hold: false }).then(() => 'ok', (e) => 'ERR ' + e)`).catch((e) => `ERR ${e?.message ?? e}`));
      await sleep(25);
    }
    const res = await Promise.all(calls);
    if (res.some((r) => r !== 'ok')) throw new Error(`连开菜单出错：${res.find((r) => r !== 'ok')}`);
    await sleep(1500);
    const n = countWindows(child.pid, 'menu');
    if (n !== 1) throw new Error(`菜单窗口应该只有 1 个，实际 ${n} 个`);
  } finally {
    pet?.close();
    spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
    await sleep(300);
    try {
      rmSync(data, { recursive: true, force: true });
    } catch {}
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
  // 安装包不带角色；以前构建留在 target/release/characters/ 里的旧文件会让「没有角色」的情况测不到
  for (const id of ['nanami-chiaki', 'makise-kurisu', 'tachibana-sherry']) {
    const left = join(EXE, '..', 'characters', id);
    if (existsSync(left)) throw new Error(`${left} 是以前的构建留下的，安装包里已经不带角色了：删掉它再测`);
  }
  const packs = await startPackServer();
  console.log(`角色包假服务器：127.0.0.1:${packs.port}`);
  const child = spawn(EXE, ['--settings'], {
    env: { ...process.env, DESKMATE_DATA_DIR: DATA, DESKMATE_CDP_PORT: String(PORT), DESKMATE_PACK_URL_BASE: `http://127.0.0.1:${packs.port}` },
    stdio: 'ignore',
  });
  const pid = child.pid;
  let settings, chat, pet;
  /** 中途打开的窗口（计划窗口），结束时一起检查前端报错 */
  const extraPages = [];
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
    const packCard = (id) => `document.querySelector('.char-card[data-testid="pack-${id}"]')`;
    const clickIn = (card, label) =>
      settings.eval(`(() => { const b = [...${card}.querySelectorAll('button')].find(b => b.innerText.trim().startsWith(${JSON.stringify(label)})); if (!b) return false; b.click(); return true; })()`);
    await test('角色：全新安装直接带着七海千秋（安装包里的初始角色，不用下载）；别的角色列出来可以下载（带预览图和大小）', async () => {
      // 打开就是千秋：不是占位形象、不提示「还没有桌宠」、不从网上下载
      await settings.waitFor(`[...document.querySelectorAll('.char-grid:not([data-testid=pack-grid]) .char-card')].some(c => c.innerText.includes('七海千秋') && c.innerText.includes('使用中'))`, '千秋已经是使用中', 8000);
      if (await settings.eval(`!!document.querySelector('[data-testid=no-character]')`)) throw new Error('全新安装不该再提示「还没有桌宠」');
      await pet.waitFor(`document.documentElement.dataset.skin === 'arcade' && !document.querySelector('.pet')?.classList.contains('placeholder')`, '桌宠一打开就是千秋', 8000);
      if (!existsSync(join(DATA, 'packs', 'nanami-chiaki', 'voice', 'index.json'))) throw new Error('初始角色的预录语音没装到数据目录');
      if (existsSync(join(DATA, 'packs', '.bundled-nanami-chiaki'))) throw new Error('装初始角色用的临时文件夹没清掉');
      if (packs.hits.some((h) => String(h).includes('nanami-chiaki'))) throw new Error(`千秋应该是安装包里带的，不该去下载：${packs.hits.join(' ')}`);
      // 千秋在「下载角色」里显示已装好（之后也能删掉），另外两位可以下载
      await settings.waitFor(`document.querySelectorAll('.char-card[data-testid^="pack-"]').length === 3`, '三位角色都列出来');
      await settings.waitFor(`${packCard('nanami-chiaki')}.innerText.includes('已下载')`, '千秋显示已装好', 5000);
      const preview = await settings.eval(`${packCard('makise-kurisu')}.querySelector('img')?.src ?? ''`);
      if (!preview.startsWith('data:image/png;base64,')) throw new Error('下载前就该看到预览图（安装包里带的小预览）');
      const size = await settings.eval(`${packCard('makise-kurisu')}.innerText`);
      if (!/下载（\d+\.\d MB）/.test(size)) throw new Error(`没有显示下载大小：${size}`);
      await settings.shot('settings-角色-全新安装.png');
      // 再下红莉栖：当前用着的不变
      if (!(await clickIn(packCard('makise-kurisu'), '下载'))) throw new Error('找不到红莉栖的下载按钮');
      await settings.waitFor(`${packCard('makise-kurisu')}.innerText.includes('已下载')`, '红莉栖下载完成', 30000);
      await settings.waitFor(`document.querySelectorAll('.char-grid:not([data-testid=pack-grid]) .char-card').length === 2`, '上面列出两位已装好的角色');
    });
    await test('角色：切换角色并保存禁词', async () => {
      await settings.waitFor(`document.querySelectorAll('.char-card').length >= 2`, '2 个内置角色');
      await settings.eval(`[...document.querySelectorAll('.char-card')].find(c => c.innerText.includes('牧濑红莉栖')).querySelector('button.btn').click()`);
      await settings.waitFor(`[...document.querySelectorAll('.char-card')].find(c => c.innerText.includes('牧濑红莉栖'))?.innerText.includes('使用中')`, '牧濑红莉栖使用中');
      await settings.eval(`[...document.querySelectorAll('.char-card')].find(c => c.innerText.includes('牧濑红莉栖')).querySelector('.char-main').click()`);
      await sleep(200);
      await settings.eval(`(() => { const ta = [...document.querySelectorAll('label')].find(l => l.innerText.startsWith('牧濑红莉栖 的禁词')).querySelector('textarea'); Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(ta, '笨蛋'); ta.dispatchEvent(new Event('input', { bubbles: true })); })()`);
      await settings.click('保存');
      await settings.waitText('已保存');
    });
    await test('角色：放进自己的图片后，桌宠按地址读取并显示（不再整张图转文本传给界面）', async () => {
      const assets = join(DATA, 'characters', 'makise-kurisu', 'assets');
      mkdirSync(assets, { recursive: true });
      // 1×1 像素的 PNG
      const png = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=';
      // 待机图会按时间 / 季节换变体（夜里 idle_night、冬天 idle_winter），内置图里有这些变体，所以一起放，测试不看几点钟
      for (const n of ['idle', 'idle_night', 'idle_winter']) writeFileSync(join(assets, `${n}.png`), Buffer.from(png, 'base64'));
      await settings.click('重新加载');
      await settings.waitText('已重新加载');
      await pet.waitFor(
        `(() => { const i = document.querySelector('.pet-img'); return !!i && /^http:\\/\\/deskmate-img\\.localhost\\/makise-kurisu\\/idle(_night|_winter)?\\?v=/.test(i.src) && i.complete && i.naturalWidth === 1; })()`,
        '桌宠显示用户图片',
      );
      if (await pet.eval(`document.querySelector('.pet').classList.contains('placeholder')`)) throw new Error('仍是占位形象');
      const preview = await settings.eval(`[...document.querySelectorAll('.char-card')].find(c => c.innerText.includes('牧濑红莉栖')).querySelector('img').src`);
      if (!preview.startsWith('http://deskmate-img.localhost/makise-kurisu/idle?v=')) throw new Error(`角色页预览地址不对：${preview.slice(0, 60)}`);
      rmSync(assets, { recursive: true, force: true });
      await settings.click('重新加载');
      // 删掉用户图片后回到内置形象：装了形象包是安装包内置的 PNG（不是刚才那张 1×1 的），否则是内置的 SVG
      const bundledArt = true; // 下载来的角色包一定带 Q 版形象
      await pet.waitFor(
        `(() => { const i = document.querySelector('.pet-img'); const s = i?.getAttribute('src') ?? ''; return ${bundledArt} ? (s.includes('/makise-kurisu/idle') && i.complete && i.naturalWidth > 1) : s.startsWith('data:image/svg+xml'); })()`,
        '删掉图片后回到内置形象',
      );
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
    await test('启动后马上连着右键：只会有一个菜单窗口（不会叠出残留的透明窗口盖住气泡）', menuStartupRace);
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
    await test('语音输入：声音页有「语音输入」卡片，默认关闭、选项藏着（不打开，免得去下载识别模型）', async () => {
      await settings.click('声音');
      await settings.waitFor(`document.querySelector('main h1')?.innerText === '声音'`, '声音页');
      await settings.waitText('语音输入');
      const off = await settings.eval(`document.querySelector('input[aria-label="打开语音输入"]')?.checked === false`);
      if (!off) throw new Error('语音输入默认应该是关闭的');
      const hidden = await settings.eval(`!document.querySelector('input[aria-label="连续对话"]')`);
      if (!hidden) throw new Error('语音输入没打开时不应该显示它的选项');
      // 没打开语音输入时，聊天框里没有麦克风按钮
      const noMic = await chat.eval(`!document.querySelector('button[aria-label="语音输入"]')`);
      if (!noMic) throw new Error('没打开语音输入时聊天框不应该有麦克风按钮');
    });
    await test('语音输入、联网开关的命令都有授权：真实调用不会报 "not allowed by ACL"（不打开语音输入，不下载模型）', async () => {
      // 单元测试和模拟接口碰不到 Tauri 的权限检查，只有真实窗口里调用才知道
      const inv = (page, cmd, args = '') => page.eval(`window.__TAURI_INTERNALS__.invoke('${cmd}'${args})`);
      const st = await inv(settings, 'asr_status');
      if (typeof st?.model_ready !== 'boolean' || !Array.isArray(st?.devices)) throw new Error(`asr_status 返回不对：${JSON.stringify(st)}`);
      const cur = await settings.eval(`window.__TAURI_INTERNALS__.invoke('settings_get').then(v => v.asr)`);
      const saved = await settings.eval(`window.__TAURI_INTERNALS__.invoke('asr_save', { asr: ${JSON.stringify(cur)} })`);
      if (saved?.enabled !== false) throw new Error(`asr_save 之后语音输入应该还是关的：${JSON.stringify(saved)}`);
      const web = await inv(settings, 'web_save', ', { enabled: true }');
      if (web?.enabled !== true) throw new Error(`web_save 返回不对：${JSON.stringify(web)}`);
      const avail = await inv(chat, 'asr_available');
      if (avail !== false) throw new Error(`没打开语音输入时 asr_available 应该是 false：${JSON.stringify(avail)}`);
    });
    await test('看屏幕：「切到新界面时看一眼」默认开、「按时间定时看」默认关（需要先允许她看屏幕）', async () => {
      await settings.click('活动感知');
      await settings.waitFor(`document.querySelector('main h1')?.innerText.includes('活动')`, '活动感知页');
      // 看屏幕的选项在打开「允许她看屏幕」后才显示；这里只看后端给的默认值
      const d = await settings.eval(`window.__TAURI_INTERNALS__.invoke('settings_get').then(v => v.vision)`);
      if (d.on_switch !== true) throw new Error('切到新界面时看一眼应该默认开');
      if (d.timed_look !== false) throw new Error('按时间定时看应该默认关');
      if (d.enabled !== false) throw new Error('允许她看屏幕应该默认关');
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
      // 构建时放进了 Q 版形象包（scripts/chibi/fetch_art.py，DECISIONS #66）→ 必须是安装包内置的 PNG（走图片协议）；没放就是内置的 SVG
      const bundledArt = true; // 下载来的角色包一定带 Q 版形象
      await pet.waitFor(`(() => { const s = document.querySelector('.pet-img')?.getAttribute('src') ?? ''; return ${bundledArt} ? (s.includes('deskmate-img') && s.includes('/nanami-chiaki/idle')) : s.startsWith('data:image/svg+xml'); })()`, bundledArt ? '千秋的 Q 版形象（安装包内置的 PNG）' : '千秋的 Q 版形象（内置 SVG）', 6000);
      console.log(`    （这次安装包里${bundledArt ? '带' : '没带'} Q 版形象包）`);
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
      // 带形象包时，摸头图是安装包内置的 pat.png（走图片协议）；否则是内置的 pat.svg
      const patOk = bundledArt ? src.includes('/nanami-chiaki/pat') : src.startsWith('data:image/svg+xml');
      if (!patOk) throw new Error(`摸头时应该换成反应图（现在是 ${src.slice(0, 80)}）`);
      await chat.click('[data-testid=speech]');
      await chat.waitFor(`!!document.querySelector('textarea[aria-label="输入消息"]')`, '变成聊天框');
      await chat.shot('chat-arcade.png');
      await chat.click('[aria-label="关闭"]');
    });

    // 构建时放进了预录语音包（scripts/voice/fetch_voice_pack.py）才测：声音页能选，包里的录音能被 WebView2 解码（只加载、不播放，不出声）
    const packIndex = join(DATA, 'packs', 'nanami-chiaki', 'voice', 'index.json');
    if (existsSync(packIndex)) {
      await test('预录语音：声音页显示句数；包里的录音能取到并被 WebView2 解码', async () => {
        await settings.click('声音');
        await settings.waitFor(`document.querySelector('main h1')?.innerText === '声音'`, '声音页');
        await settings.eval(`[...document.querySelectorAll('label.engine-choice')].find(l => l.innerText.includes('预录语音')).querySelector('input').click()`);
        await settings.waitText('已装好');
        await settings.shot('settings-预录语音.png');
        const lines = Object.keys(JSON.parse(readFileSync(packIndex, 'utf8')).lines ?? {});
        if (lines.length < 100) throw new Error(`语音包里只有 ${lines.length} 句`);
        const sec = await pet.eval(`(async () => {
          const buf = await window.__TAURI_INTERNALS__.invoke('voice_audio', { id: '${lines[0]}' });
          const a = new Audio(URL.createObjectURL(new Blob([buf])));
          await new Promise((ok, no) => { a.onloadedmetadata = ok; a.onerror = () => no(new Error('WebView2 解码失败')); setTimeout(() => no(new Error('解码超时')), 5000); });
          return a.duration;
        })()`);
        if (!(sec > 0.3 && sec < 20)) throw new Error(`录音时长不对：${sec}`);
        console.log(`    （语音包 ${lines.length} 句，第一句 ${sec.toFixed(1)} 秒）`);
      });
    }

    await test('换成牧濑红莉栖：桌宠、聊天窗口、头顶菜单换上实验室风皮肤，带她自己的 Q 版形象', async () => {
      await settings.click('角色');
      await settings.waitFor(`document.body.innerText.includes('牧濑红莉栖')`, '角色列表');
      await settings.eval(`(() => { const c = [...document.querySelectorAll('.char-card')].find(c => c.innerText.includes('牧濑红莉栖')); const b = c && [...c.querySelectorAll('button')].find(b => b.innerText.trim() === '切换'); if (b) b.click(); })()`);
      await pet.waitFor(`document.documentElement.dataset.skin === 'lab'`, '桌宠皮肤', 6000);
      const bundledArt = true; // 下载来的角色包一定带 Q 版形象
      await pet.waitFor(`(() => { const s = document.querySelector('.pet-img')?.getAttribute('src') ?? ''; return ${bundledArt} ? (s.includes('deskmate-img') && s.includes('/makise-kurisu/idle')) : s.startsWith('data:image/svg+xml'); })()`, bundledArt ? '红莉栖的 Q 版形象（安装包内置的 PNG）' : '红莉栖的 Q 版形象（内置 SVG）', 6000);
      console.log(`    （这次安装包里${bundledArt ? '带' : '没带'}红莉栖的 Q 版形象包）`);
      await sleep(300);
      await pet.shot('pet-kurisu.png');
      await chat.waitFor(`document.documentElement.dataset.skin === 'lab'`, '聊天皮肤', 6000);
      await petAt(0, 15);
      await chat.waitFor(`!!document.querySelector('.panel.speech')`, '她说的话气泡', 6000);
      await sleep(300);
      await chat.shot('chat-speech-lab.png');
      const src = await pet.eval(`document.querySelector('.pet-img').getAttribute('src')`);
      await pet.shot('pet-kurisu-pat.png');
      const patOk = bundledArt ? src.includes('/makise-kurisu/pat') : src.startsWith('data:image/svg+xml');
      if (!patOk) throw new Error(`摸头时应该换成反应图（现在是 ${src.slice(0, 80)}）`);
      await chat.click('[data-testid=speech]');
      await chat.waitFor(`!!document.querySelector('textarea[aria-label="输入消息"]')`, '变成聊天框');
      await chat.shot('chat-lab.png');
      await chat.click('[aria-label="关闭"]');
      // 头顶气泡菜单也是实验室风
      await petAt(2, 150);
      const tm = await waitTarget('menu.html', 8000);
      if (!tm) throw new Error('气泡菜单页面没有出现');
      const menu = await Page.open('menu', tm.webSocketDebuggerUrl);
      try {
        await menu.waitFor(`document.querySelectorAll('.bubble').length >= 7 && document.documentElement.dataset.skin === 'lab'`, '实验室风气泡', 5000);
        await sleep(400);
        await menu.shot('menu-lab.png');
        await menu.click('[data-id=chat]');
        await waitVisible(pid, 'menu', false);
        await chat.waitFor(`!!document.querySelector('textarea[aria-label="输入消息"]')`, '聊天框', 6000);
        await chat.click('[aria-label="关闭"]');
        if (menu.errors.length) throw new Error(`菜单页面报错：${menu.errors.join(' | ')}`);
      } finally {
        menu.close();
      }
    });
    const kurisuPackIndex = join(DATA, 'packs', 'makise-kurisu', 'voice', 'index.json');
    if (existsSync(kurisuPackIndex)) {
      await test('红莉栖的预录语音：声音页显示句数；包里的录音能被 WebView2 解码', async () => {
        await settings.click('声音');
        await settings.waitFor(`document.querySelector('main h1')?.innerText === '声音'`, '声音页');
        await settings.eval(`[...document.querySelectorAll('label.engine-choice')].find(l => l.innerText.includes('预录语音')).querySelector('input').click()`);
        await settings.waitText('已装好');
        await settings.shot('settings-预录语音-红莉栖.png');
        const lines = Object.keys(JSON.parse(readFileSync(kurisuPackIndex, 'utf8')).lines ?? {});
        if (lines.length < 100) throw new Error(`语音包里只有 ${lines.length} 句`);
        const sec = await pet.eval(`(async () => {
          const buf = await window.__TAURI_INTERNALS__.invoke('voice_audio', { id: '${lines[0]}' });
          const a = new Audio(URL.createObjectURL(new Blob([buf])));
          await new Promise((ok, no) => { a.onloadedmetadata = ok; a.onerror = () => no(new Error('WebView2 解码失败')); setTimeout(() => no(new Error('解码超时')), 5000); });
          return a.duration;
        })()`);
        if (!(sec > 0.3 && sec < 20)) throw new Error(`录音时长不对：${sec}`);
        console.log(`    （红莉栖语音包 ${lines.length} 句，第一句 ${sec.toFixed(1)} 秒）`);
      });
    }

    await test('雪莉：下载、切换后换上侦探笔记本风皮肤，带她自己的 Q 版形象；预录语音能被 WebView2 解码；删掉后切回红莉栖', async () => {
      await settings.click('角色');
      await settings.waitFor(`!!${packCard('tachibana-sherry')}`, '雪莉的下载卡');
      if (!(await clickIn(packCard('tachibana-sherry'), '下载'))) throw new Error('找不到雪莉的下载按钮');
      await settings.waitFor(`${packCard('tachibana-sherry')}.innerText.includes('已下载')`, '雪莉下载完成', 30000);
      await settings.eval(`(() => { const c = [...document.querySelectorAll('.char-card')].find(c => c.innerText.includes('橘雪莉') && !c.dataset.testid); const b = c && [...c.querySelectorAll('button')].find(b => b.innerText.trim() === '切换'); if (b) b.click(); })()`);
      await pet.waitFor(`document.documentElement.dataset.skin === 'detective'`, '桌宠皮肤', 6000);
      await pet.waitFor(`(() => { const s = document.querySelector('.pet-img')?.getAttribute('src') ?? ''; return s.includes('deskmate-img') && s.includes('/tachibana-sherry/idle'); })()`, '雪莉的 Q 版形象（下载来的 PNG）', 6000);
      await sleep(300);
      await pet.shot('pet-sherry.png');
      await chat.waitFor(`document.documentElement.dataset.skin === 'detective'`, '聊天皮肤', 6000);
      await petAt(0, 15);
      await chat.waitFor(`!!document.querySelector('.panel.speech')`, '她说的话气泡', 6000);
      await sleep(300);
      await chat.shot('chat-speech-detective.png');
      const src = await pet.eval(`document.querySelector('.pet-img').getAttribute('src')`);
      await pet.shot('pet-sherry-pat.png');
      if (!src.includes('/tachibana-sherry/pat')) throw new Error(`摸头时应该换成反应图（现在是 ${src.slice(0, 80)}）`);
      await chat.click('[data-testid=speech]');
      await chat.waitFor(`!!document.querySelector('textarea[aria-label="输入消息"]')`, '变成聊天框');
      await chat.shot('chat-detective.png');
      await chat.click('[aria-label="关闭"]');
      const sherryPackIndex = join(DATA, 'packs', 'tachibana-sherry', 'voice', 'index.json');
      if (!existsSync(sherryPackIndex)) throw new Error('角色包里的预录语音没装到数据目录');
      await settings.click('声音');
      await settings.waitFor(`document.querySelector('main h1')?.innerText === '声音'`, '声音页');
      await settings.eval(`[...document.querySelectorAll('label.engine-choice')].find(l => l.innerText.includes('预录语音')).querySelector('input').click()`);
      await settings.waitText('已装好');
      await settings.shot('settings-预录语音-雪莉.png');
      const lines = Object.keys(JSON.parse(readFileSync(sherryPackIndex, 'utf8')).lines ?? {});
      if (lines.length < 100) throw new Error(`语音包里只有 ${lines.length} 句`);
      const sec = await pet.eval(`(async () => {
        const buf = await window.__TAURI_INTERNALS__.invoke('voice_audio', { id: '${lines[0]}' });
        const a = new Audio(URL.createObjectURL(new Blob([buf])));
        await new Promise((ok, no) => { a.onloadedmetadata = ok; a.onerror = () => no(new Error('WebView2 解码失败')); setTimeout(() => no(new Error('解码超时')), 5000); });
        return a.duration;
      })()`);
      if (!(sec > 0.3 && sec < 20)) throw new Error(`录音时长不对：${sec}`);
      console.log(`    （雪莉语音包 ${lines.length} 句，第一句 ${sec.toFixed(1)} 秒）`);
      // 切回红莉栖，删掉雪莉（后面的测试仍按「千秋 + 红莉栖」来）
      await settings.click('角色');
      await settings.eval(`(() => { const c = [...document.querySelectorAll('.char-card')].find(c => c.innerText.includes('牧濑红莉栖') && !c.dataset.testid); const b = c && [...c.querySelectorAll('button')].find(b => b.innerText.trim() === '切换'); if (b) b.click(); })()`);
      await pet.waitFor(`document.documentElement.dataset.skin === 'lab'`, '切回红莉栖', 6000);
      if (!(await clickIn(packCard('tachibana-sherry'), '删除'))) throw new Error('找不到雪莉的删除按钮');
      await settings.waitFor(`${packCard('tachibana-sherry')}.innerText.includes('下载（')`, '雪莉变回待下载', 8000);
      if (existsSync(join(DATA, 'packs', 'tachibana-sherry'))) throw new Error('雪莉的文件夹没有删掉');
    });

    await test('角色：不用的角色可以删掉（正在用的不能删），删掉后又能重新下载', async () => {
      await settings.click('角色');
      await settings.waitFor(`!!${packCard('makise-kurisu')}`, '红莉栖的下载卡');
      const cur = await settings.eval(`${packCard('makise-kurisu')}.innerText`);
      if (!cur.includes('使用中，不能删除') || (await clickIn(packCard('makise-kurisu'), '删除'))) throw new Error(`正在用的角色不该能删：${cur}`);
      if (!(await clickIn(packCard('nanami-chiaki'), '删除'))) throw new Error('找不到千秋的删除按钮');
      await settings.waitFor(`${packCard('nanami-chiaki')}.innerText.includes('下载（')`, '千秋变回待下载', 8000);
      if (existsSync(join(DATA, 'packs', 'nanami-chiaki'))) throw new Error('千秋的文件夹没有删掉');
      await settings.waitFor(`document.querySelectorAll('.char-grid:not([data-testid=pack-grid]) .char-card').length === 1`, '上面只剩红莉栖');
    });

    console.log('\n计划');
    let plans = null;
    // 她刚说了句反应时弹的是「她说的话」气泡（没有输入框）：点一下变成聊天框；聊天窗口关着就点她身体
    const chatInput = async () => {
      if (await chat.eval(`!!document.querySelector('textarea[aria-label="输入消息"]')`)) return;
      if (await chat.eval(`!!document.querySelector('[data-testid=speech]')`)) await chat.click('[data-testid=speech]');
      else await petAt(0, 150);
      await chat.waitFor(`!!document.querySelector('textarea[aria-label="输入消息"]')`, '聊天输入框', 6000);
    };
    await test('计划：设置「提醒」页的入口打开计划窗口（第一次没有计划）', async () => {
      await settings.click('提醒');
      await settings.waitFor(`document.querySelector('main h1')?.innerText === '提醒'`, '提醒页');
      await settings.click('打开计划');
      const t = await waitTarget('plans.html', 10000);
      if (!t) throw new Error('计划窗口没有打开');
      plans = await Page.open('plans', t.webSocketDebuggerUrl);
      extraPages.push(plans);
      await plans.waitFor(`!!document.querySelector('.plans-nav') && document.body.innerText.includes('今天还没有安排')`, '计划窗口加载', 10000);
      await waitVisible(pid, 'plans', true);
      await plans.shot('plans-today-empty.png');
    });
    await test('计划：写一句话 → 她来安排（mock 用本地规则理解）→ 看一看 → 开始', async () => {
      await plans.click('＋ 新建计划');
      await plans.waitFor(`!!document.querySelector('textarea[aria-label="想做什么计划"]')`, '新建页');
      await plans.input('textarea[aria-label="想做什么计划"]', '期末考试前复习线性代数第1-6章，每天1个半小时，周末2小时，晚上8点开始');
      await plans.click('让她来安排');
      await plans.waitText('看看这样排行不行', 15000);
      const n = await plans.eval(`document.querySelectorAll('.item-list .item').length`);
      if (n !== 6) throw new Error(`应该拆成 6 章，实际 ${n} 条`);
      const remind = await plans.eval(`document.querySelector('input[aria-label="每天几点开始"]').value`);
      if (remind !== '20:00') throw new Error(`开始时间没认出来：${remind}`);
      const sat = await plans.eval(`document.querySelector('input[aria-label="周六多少分钟"]').value`);
      if (sat !== '120') throw new Error(`周末的时间没认出来：${sat}`);
      await plans.shot('plans-draft.png');
      await plans.click('就这样，开始！');
      await plans.waitFor(`[...document.querySelectorAll('[role=tab]')].some(t => t.innerText === '日历')`, '计划详情', 8000);
      await plans.waitText('线性代数');
      await plans.shot('plans-detail.png');
      await plans.click('日历');
      await plans.waitFor(`!!document.querySelector('.cal-grid')`, '日历');
      await plans.shot('plans-calendar.png');
    });
    await test('计划：今天的事项能勾掉，桌宠有反应；聊天里问「今天还要做什么」直接回答', async () => {
      await plans.click('☀ 今天');
      await plans.waitFor(`document.querySelectorAll('.task-list .task').length > 0`, '今天的事项', 8000);
      await plans.shot('plans-today.png');
      const before = await chat.eval(`document.querySelector('.panel')?.innerText ?? ''`);
      await plans.click('.task-list .task button.check');
      await plans.waitFor(`document.querySelector('.task-list .task button.check')?.getAttribute('aria-checked') === 'true'`, '勾上了');
      await chat.waitFor(`(document.querySelector('.panel')?.innerText ?? '') !== ${JSON.stringify(before)}`, '她说了一句', 8000);
      await chat.shot('chat-plan-done.png');
      await chatInput();
      await chat.input('textarea[aria-label="输入消息"]', '今天还要做什么');
      await chat.key('textarea[aria-label="输入消息"]', { key: 'Enter' });
      await chat.waitFor(`[...document.querySelectorAll('.msg-assistant')].some(m => /今天.*(项|完成)/.test(m.innerText))`, '本地回答今天的计划', 8000);
    });
    await test('计划：随手加一件事、专注计时（桌宠脚下有倒计时），停下后消失', async () => {
      await plans.input('input[aria-label="加一件事"]', '取快递');
      await plans.input('input[aria-label="大概多少分钟"]', '15');
      await plans.click('加上');
      await plans.waitText('取快递');
      await plans.click('button[aria-label="开始专注：取快递"]');
      await plans.waitFor(`!!document.querySelector('.focus-bar')`, '计划窗口的专注条', 5000);
      await pet.waitFor(`!!document.querySelector('.pet-focus')`, '桌宠脚下的倒计时', 5000);
      await plans.shot('plans-focus.png');
      await pet.shot('pet-focus.png');
      await plans.click('停下');
      await plans.waitFor(`!document.querySelector('.focus-bar')`, '专注条消失', 5000);
      await pet.waitFor(`!document.querySelector('.pet-focus')`, '倒计时消失', 5000);
    });
    await test('计划：聊天气泡里的 [[计划：…]] 按钮打开计划窗口并填好那句话', async () => {
      await chatInput();
      await chat.input('textarea[aria-label="输入消息"]', '/mock plan 每天背50个单词');
      await chat.key('textarea[aria-label="输入消息"]', { key: 'Enter' });
      await chat.waitFor(`[...document.querySelectorAll('.action button')].some(b => b.innerText === '去安排')`, '「去安排」按钮', 8000);
      await chat.shot('chat-plan-button.png');
      await chat.click('去安排');
      await plans.waitFor(`document.querySelector('textarea[aria-label="想做什么计划"]')?.value === '每天背50个单词'`, '新建页填好了那句话', 8000);
      // 收起聊天窗口，后面的测试从「聊天窗口关着」开始
      await chat.click('[aria-label="关闭"]');
      await waitVisible(pid, 'chat', false);
    });
    await test('计划：她怎么陪你：切换监督强度马上保存', async () => {
      await plans.click('.plans-nav > .nav-item:last-child');
      await plans.waitFor(`document.querySelector('main h1')?.innerText.endsWith('怎么陪你')`, '监督设置页');
      await plans.click('严格');
      await plans.waitText('已保存');
      await plans.shot('plans-settings.png');
      const s = await plans.eval(`window.__TAURI_INTERNALS__.invoke('plan_boot').then(v => v.settings.strictness)`);
      if (s !== 'strict') throw new Error(`设置里没存上「严格」：${s}`);
    });
    // 计划书（复杂的长期计划）：mock 用本地规则——复杂的先问，回答完出路线图，最近三周按要点拆开
    const svgClick = (sel) => plans.eval(`(() => { const el = document.querySelector(${JSON.stringify(sel)}); if (!el) return false; el.dispatchEvent(new MouseEvent('click', { bubbles: true })); return true; })()`);
    const LONG_PLAN = [
      '帮我做一份考证准备计划。',
      '【我的背景】',
      '- 某某大学 机械专业，明年7月毕业',
      '- 英语四级，做过两个课程设计',
      '【约束】',
      '- 每天只能投入1〜2小时，周末可多一些；要准备期末考试',
      '- 实习作为备选：每周最多1小时用于联系公司，2027年2月底前没有回应就放弃',
      '【准备内容（按优先级）】',
      '1. 吃透专业课，能讲清楚三个核心概念（最高优先级）',
      '2. 刷历年真题：选择、计算、案例',
      '3. 做一个小作品集：选题、实现、整理',
      '【输出要求】',
      '- 时间范围：2026年11月9日〜2027年6月30日',
      '- 按阶段划分，有关键节点，最后附一个每周日自查清单；如有需要补充的信息，请先向我提问',
    ].join('\n');
    await test('计划书：写一份很长的计划 → 她先问几句 → 回答后出计划书（时间线）', async () => {
      await plans.click('＋ 新建计划');
      await plans.waitFor(`!!document.querySelector('textarea[aria-label="想做什么计划"]')`, '新建页');
      await plans.input('textarea[aria-label="想做什么计划"]', LONG_PLAN);
      await plans.click('让她来安排');
      // 新建页的说明里也有「先问你几句」，等问题本身出来
      await plans.waitFor(`document.querySelectorAll('.question').length > 0`, '她先问的问题', 15000);
      const qs = await plans.eval(`document.querySelectorAll('.question').length`);
      if (qs < 1 || qs > 3) throw new Error(`应该问 1~3 个问题，实际 ${qs} 个`);
      await plans.click('1月中旬期末考试');
      await plans.shot('plans-goal-questions.png');
      await plans.click('回答好了，开始排');
      await plans.waitText('计划书：看看这样排行不行', 20000);
      await plans.waitFor(`!!document.querySelector('svg.viz.timeline')`, '时间线');
      const lanes = await plans.eval(`[...document.querySelectorAll('.lane-label')].map(g => g.getAttribute('aria-label'))`);
      for (const want of ['线：吃透专业课', '线：刷历年真题', '线：做一个小作品集', '线：联系公司']) {
        if (!lanes.includes(want)) throw new Error(`时间线上没有「${want}」：${lanes.join('、')}`);
      }
      const phases = await plans.eval(`document.querySelectorAll('.phase-band').length`);
      if (phases !== 3) throw new Error(`应该分 3 个阶段，实际 ${phases}`);
      const buffers = await plans.eval(`document.querySelectorAll('.buffer').length`);
      if (buffers !== 1) throw new Error(`回答里的期末考试应该变成一段缓冲，实际 ${buffers}`);
      await plans.eval(`document.querySelector('.viz-card')?.scrollIntoView({ block: 'start' })`);
      await plans.shot('plans-goal-draft.png');
      // 点第一个阶段：下面出来阶段的编辑
      if (!(await svgClick('.phase-band'))) throw new Error('点不到阶段');
      await plans.waitFor(`[...document.querySelectorAll('.node-panel h2')].some(h => h.innerText.startsWith('阶段：打基础'))`, '阶段的编辑面板');
      await plans.shot('plans-goal-phase.png');
      await plans.click('就这样，开始！');
      await plans.waitFor(`!!document.querySelector('.goal-stats') && [...document.querySelectorAll('[role=tab]')].some(t => t.innerText === '计划书')`, '计划书页', 10000);
      await plans.waitFor(`!!document.querySelector('svg.viz.timeline')`, '计划书页的时间线');
      await plans.eval(`document.querySelector('.viz-card')?.scrollIntoView({ block: 'start' })`);
      await plans.shot('plans-goal.png');
    });
    await test('计划书：今天排了它的线；树状图、流程图；点节点能改', async () => {
      const today = await plans.eval(`window.__TAURI_INTERNALS__.invoke('plan_today')`);
      const mine = today.tasks.filter((t) => t.label.includes('·'));
      if (!mine.length) throw new Error(`今天没有排计划书的线：${JSON.stringify(today.tasks.map((t) => t.label))}`);
      await plans.click('树状图');
      await plans.waitFor(`!!document.querySelector('svg.viz.tree')`, '树状图');
      await plans.eval(`document.querySelector('.viz-card')?.scrollIntoView({ block: 'start' })`);
      await plans.shot('plans-goal-tree.png');
      await plans.click('流程图');
      await plans.waitFor(`!!document.querySelector('svg.viz.flow')`, '流程图');
      await plans.eval(`document.querySelector('.viz-card')?.scrollIntoView({ block: 'start' })`);
      await plans.shot('plans-goal-flow.png');
      if (!(await svgClick('.v-fnode.ms'))) throw new Error('流程图上没有节点');
      await plans.waitFor(`[...document.querySelectorAll('.node-panel h2')].some(h => h.innerText.startsWith('节点：'))`, '节点的编辑面板');
      await plans.click('时间线');
      await plans.waitFor(`!!document.querySelector('svg.viz.timeline')`, '切回时间线');
    });
    await test('计划书：这周 → 调整下周（mock 按要点细化）→ 按这个改；原文是生成好的', async () => {
      await plans.click('这周');
      await plans.click('写一句要求');
      await plans.input('textarea[aria-label="想怎么调整下周"]', '真题多练一点');
      await plans.click('让她调整');
      await plans.waitText('调整下周：', 15000);
      await plans.shot('plans-goal-adjust.png');
      await plans.click('按这个改');
      await plans.waitText('已更新计划书', 8000);
      await plans.click('原文');
      const doc = await plans.eval(`document.querySelector('textarea[aria-label="计划书原文"]').value`);
      if (!doc.includes('## 阶段1：打基础') || !doc.includes('## 每周固定的事')) throw new Error(`原文不对：${doc.slice(0, 200)}`);
      await plans.shot('plans-goal-doc.png');
      await plans.click('计划书');
    });
    await test('计划书：深色模式、窄窗口也能看', async () => {
      await plans.send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: 'dark' }] });
      await sleep(300);
      await plans.eval(`document.querySelector('.viz-card')?.scrollIntoView({ block: 'start' })`);
      await plans.shot('plans-goal-dark.png');
      await plans.send('Emulation.setEmulatedMedia', { features: [] });
      await plans.send('Emulation.setDeviceMetricsOverride', { width: 720, height: 600, deviceScaleFactor: 1, mobile: false });
      await sleep(400);
      const over = await plans.eval(`document.documentElement.scrollWidth - document.documentElement.clientWidth`);
      await plans.shot('plans-goal-narrow.png');
      await plans.send('Emulation.clearDeviceMetricsOverride');
      if (over > 2) throw new Error(`窄窗口时整页横着溢出了 ${over}px（图应该在自己的框里横向滚动）`);
    });
    await test('计划：统计页（热力图、8 周柱状图、每个计划一行；深色、窄窗口；点计划名字跳过去）', async () => {
      await plans.click('📊 统计');
      await plans.waitFor(`document.querySelectorAll('.stat-heat [aria-label]').length > 0 && document.querySelectorAll('.bar-col').length === 8`, '热力图和 8 根柱子', 8000);
      await plans.waitFor(`document.querySelectorAll('.stat-plans li').length > 0`, '每个计划一行', 5000);
      await plans.shot('plans-stats.png');
      await plans.send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: 'dark' }] });
      await sleep(300);
      await plans.shot('plans-stats-dark.png');
      await plans.send('Emulation.setEmulatedMedia', { features: [] });
      await plans.send('Emulation.setDeviceMetricsOverride', { width: 720, height: 600, deviceScaleFactor: 1, mobile: false });
      await sleep(400);
      const over = await plans.eval(`document.documentElement.scrollWidth - document.documentElement.clientWidth`);
      await plans.shot('plans-stats-narrow.png');
      await plans.send('Emulation.clearDeviceMetricsOverride');
      if (over > 2) throw new Error(`统计页在窄窗口时横着溢出了 ${over}px`);
      const stats = await plans.eval(`window.__TAURI_INTERNALS__.invoke('plan_stats')`);
      if (!stats || !Array.isArray(stats.plans)) throw new Error(`plan_stats 返回不对：${JSON.stringify(stats)?.slice(0, 200)}`);
      // 点单独那个计划的名字：跳到计划页
      await plans.eval(`[...document.querySelectorAll('.stat-plans button, .stat-plans a')].find(b => b.innerText.includes('线性代数'))?.click()`);
      await plans.waitFor(`[...document.querySelectorAll('[role=tab]')].some(t => t.innerText === '日历')`, '跳到了计划页', 5000);
    });
    await test('日历：点某一天写下要做的事 → 格子里有、能勾「搞定了」；明天的事出现在「今天」页', async () => {
      const today = (await plans.eval(`window.__TAURI_INTERNALS__.invoke('plan_boot')`)).today;
      await plans.click('📅 日历');
      await plans.waitFor(`document.querySelectorAll('.agenda-grid .agenda-cell').length === 42`, '一个月 42 格', 8000);
      // 下个月 25 号（一定在 7 天以外：提示里有 4 个提醒的日子）
      await plans.click('button[aria-label="下个月"]');
      const [y, m] = today.split('-').map(Number);
      const next = m === 12 ? `${y + 1}-01-25` : `${y}-${String(m + 1).padStart(2, '0')}-25`;
      await plans.waitFor(`!!document.querySelector('[data-day="${next}"]:not(.other)')`, '翻到下个月', 5000);
      await plans.click(`[data-day="${next}"]`);
      await plans.waitFor(`document.querySelector('[data-day="${next}"]')?.getAttribute('aria-pressed') === 'true'`, '选中那一天', 5000);
      await plans.input('input[aria-label="这天有什么事"]', '数据结构作业截止');
      await plans.click('记下');
      await plans.waitFor(`document.querySelector('[data-day="${next}"]')?.innerText.includes('数据结构作业截止')`, '格子里有这一条', 8000);
      const hint = await plans.eval(`document.querySelector('.agenda-day')?.innerText ?? ''`);
      if (!/各提醒你一次/.test(hint) || (hint.match(/月\d+日/g) ?? []).length < 3 || !hint.includes('和当天')) throw new Error(`提醒的日子提示不对：${hint.slice(-120)}`);
      await plans.shot('plans-calendar-month.png');
      await plans.click('button[role=checkbox][aria-label="搞定了：数据结构作业截止"]');
      await plans.waitFor(`document.querySelector('button[role=checkbox][aria-label="改回没搞定：数据结构作业截止"]')?.getAttribute('aria-checked') === 'true'`, '勾上了搞定了', 5000);
      // 明天的事：「今天」页上有一张日历小卡片
      await plans.click('回到今天');
      const tomorrow = await plans.eval(`(() => { const d = new Date(${JSON.stringify(today)} + 'T00:00:00'); d.setDate(d.getDate() + 1); const p = (n) => String(n).padStart(2, '0'); return d.getFullYear() + '-' + p(d.getMonth() + 1) + '-' + p(d.getDate()); })()`);
      await plans.click(`[data-day="${tomorrow}"]`);
      await plans.input('input[aria-label="这天有什么事"]', '线代课停一次');
      await plans.click('记下');
      await plans.waitFor(`document.querySelector('[data-day="${tomorrow}"]')?.innerText.includes('线代课停一次')`, '明天的格子里有', 8000);
      const rows = await plans.eval(`window.__TAURI_INTERNALS__.invoke('agenda_range', { from: ${JSON.stringify(today)}, to: ${JSON.stringify(next)} })`);
      if (!Array.isArray(rows) || rows.length !== 2 || !rows.find((r) => r.title === '数据结构作业截止')?.done) throw new Error(`agenda_range 返回不对：${JSON.stringify(rows)?.slice(0, 300)}`);
      await plans.send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: 'dark' }] });
      await sleep(300);
      await plans.shot('plans-calendar-dark.png');
      await plans.send('Emulation.setEmulatedMedia', { features: [] });
      await plans.send('Emulation.setDeviceMetricsOverride', { width: 720, height: 600, deviceScaleFactor: 1, mobile: false });
      await sleep(400);
      const over = await plans.eval(`document.documentElement.scrollWidth - document.documentElement.clientWidth`);
      await plans.shot('plans-calendar-narrow.png');
      await plans.send('Emulation.clearDeviceMetricsOverride');
      if (over > 2) throw new Error(`日历在窄窗口时横着溢出了 ${over}px`);
      await plans.click('☀ 今天');
      await plans.waitFor(`document.querySelector('.agenda-soon')?.innerText.includes('线代课停一次')`, '「今天」页的日历小卡片', 8000);
      await plans.shot('plans-today-agenda.png');
    });
    await test('计划：计划窗口的命令都有授权，别的窗口的命令调不了', async () => {
      const inv = (cmd, args = '') => plans.eval(`window.__TAURI_INTERNALS__.invoke('${cmd}'${args})`);
      const list = await inv('plan_list');
      // 单独的计划一个，计划书的线另外算
      if (!Array.isArray(list) || list.filter((p) => !p.plan.goal_id).length !== 1) throw new Error(`plan_list 返回不对：${JSON.stringify(list)?.slice(0, 200)}`);
      const goals = await inv('goal_list');
      if (!Array.isArray(goals) || goals.length !== 1) throw new Error(`goal_list 返回不对：${JSON.stringify(goals)?.slice(0, 200)}`);
      const gv = await inv('goal_get', `, { id: ${goals[0].goal.id} }`);
      if (gv?.full?.phases?.length !== 3) throw new Error('goal_get 返回不对');
      const today = await inv('plan_today');
      if (!Array.isArray(today?.tasks)) throw new Error('plan_today 返回不对');
      const ag = await inv('agenda_range', `, { from: '2026-01-01', to: '2026-02-01' }`);
      if (!Array.isArray(ag)) throw new Error('agenda_range 返回不对');
      const st = await inv('plan_focus_state');
      if (st !== null) throw new Error(`没在专注时 plan_focus_state 应该是 null：${JSON.stringify(st)}`);
      const denied = await plans.eval(`window.__TAURI_INTERNALS__.invoke('settings_get').then(() => 'ok', e => String(e))`);
      if (!/not allowed/i.test(denied)) throw new Error(`计划窗口不该能读设置：${denied}`);
    });
    await test('计划：关掉计划窗口（WM_CLOSE）', async () => {
      windows(pid, 'plans');
      const end = Date.now() + 5000;
      while (Date.now() < end && (await targets()).some((t) => t.url.includes('plans.html'))) await sleep(200);
      if ((await targets()).some((t) => t.url.includes('plans.html'))) throw new Error('计划窗口没有关掉');
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
    for (const p of [settings, chat, pet, ...extraPages]) {
      if (p && p.errors.length) {
        results.push({ name: `${p.name} 页面没有前端报错`, ok: false, msg: p.errors.slice(0, 5).join(' | ') });
        console.log(`  ✗ ${p.name} 页面有前端报错：${p.errors.slice(0, 5).join(' | ')}`);
      }
      p?.close();
    }
    spawnSync('taskkill', ['/PID', String(pid), '/T', '/F'], { stdio: 'ignore' });
    packs.close();
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
