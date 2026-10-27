// 开发脚本共用：确保能找到 cargo（本机 Rust 装在 D:/dev，见 DECISIONS #3）
import { existsSync } from 'node:fs';
import { delimiter, join } from 'node:path';

export function devEnv(extra = {}) {
  const env = { ...process.env, ...extra };
  const cargoHome = env.CARGO_HOME || 'D:/dev/cargo';
  const cargoBin = join(cargoHome, 'bin');
  const pathKey = Object.keys(env).find((k) => k.toUpperCase() === 'PATH') ?? 'PATH';
  const current = env[pathKey] || '';
  if (existsSync(cargoBin) && !current.toLowerCase().includes(cargoBin.toLowerCase())) {
    env[pathKey] = cargoBin + delimiter + current;
    env.CARGO_HOME = cargoHome;
    env.RUSTUP_HOME = env.RUSTUP_HOME || 'D:/dev/rustup';
  }
  return env;
}
