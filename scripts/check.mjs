// 提交前一键检查：类型检查 → 前端测试 → cargo fmt → clippy → cargo test
import { spawnSync } from 'node:child_process';
import { devEnv } from './env.mjs';

const env = devEnv();
const steps = [
  ['类型检查', 'npx', ['tsc', '--noEmit'], '.'],
  ['前端测试', 'npx', ['vitest', 'run'], '.'],
  ['Rust 格式', 'cargo', ['fmt', '--check'], 'src-tauri'],
  ['Rust lint', 'cargo', ['clippy', '--all-targets', '--', '-D', 'warnings'], 'src-tauri'],
  ['Rust 测试', 'cargo', ['test'], 'src-tauri'],
];

for (const [name, cmd, args, cwd] of steps) {
  console.log(`\n=== ${name} ===`);
  const r = spawnSync(cmd, args, { cwd, env, stdio: 'inherit', shell: process.platform === 'win32' });
  if (r.status !== 0) {
    console.error(`\n✗ ${name} 失败`);
    process.exit(r.status ?? 1);
  }
}
console.log('\n✓ 全部检查通过');
