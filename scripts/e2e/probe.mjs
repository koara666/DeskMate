// 排查用：启动测试实例 → 关掉设置窗口 → 从聊天页直接调用 open_settings，打印结果和日志
import { spawn, spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { tmpdir } from 'node:os';

const EXE = resolve('src-tauri/target/release/deskmate.exe');
const PORT = 9334;
const DATA = join(tmpdir(), `deskmate-probe-${Date.now()}`);
mkdirSync(DATA, { recursive: true });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const child = spawn(EXE, ['--settings'], { env: { ...process.env, DESKMATE_DATA_DIR: DATA, DESKMATE_CDP_PORT: String(PORT) }, stdio: 'ignore' });
const list = async () => (await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json()).filter((t) => t.type === 'page');
async function evalIn(url, expr) {
  const t = (await list()).find((x) => x.url.includes(url));
  const ws = new WebSocket(t.webSocketDebuggerUrl);
  await new Promise((r) => (ws.onopen = r));
  const res = await new Promise((r) => {
    ws.onmessage = (m) => r(JSON.parse(m.data));
    ws.send(JSON.stringify({ id: 1, method: 'Runtime.evaluate', params: { expression: expr, awaitPromise: true, returnByValue: true } }));
  });
  ws.close();
  return res.result;
}
try {
  for (let i = 0; i < 60 && (await list().catch(() => [])).length < 3; i++) await sleep(300);
  console.log('pages:', (await list()).map((t) => t.url.split('/').pop()));
  spawnSync('powershell', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', resolve('scripts/e2e/window.ps1'), '-ProcessId', String(child.pid), '-Close', 'settings']);
  await sleep(1500);
  console.log('after close:', (await list()).map((t) => t.url.split('/').pop()));
  const r = await evalIn('chat.html', `window.__TAURI_INTERNALS__.invoke('open_settings').then(() => 'ok', e => 'ERR ' + e)`);
  console.log('invoke open_settings:', JSON.stringify(r));
  await sleep(2000);
  console.log('after reopen:', (await list()).map((t) => t.url.split('/').pop()));
} finally {
  await sleep(500);
  console.log('--- log ---');
  try {
    console.log(readFileSync(join(DATA, 'logs', 'deskmate.log'), 'utf8').split('\n').slice(-15).join('\n'));
  } catch (e) {
    console.log('no log', e.message);
  }
  spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
}
