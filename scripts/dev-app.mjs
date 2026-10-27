// 开发模式运行：数据放在项目内 .devdata（已被 .gitignore 排除），不碰真实用户数据
import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import { devEnv } from './env.mjs';

const env = devEnv({ DESKMATE_DATA_DIR: resolve('.devdata') });
const r = spawnSync('npx', ['tauri', 'dev', ...process.argv.slice(2)], {
  env,
  stdio: 'inherit',
  shell: process.platform === 'win32',
});
process.exit(r.status ?? 1);
