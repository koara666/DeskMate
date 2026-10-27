// 与 Rust 端（src-tauri/src/commands.rs、chat.rs）保持一致的类型

export type PetState = 'idle' | 'thinking' | 'talking' | 'happy' | 'sad' | 'sleep';
export type ChatMode = 'compact' | 'expanded';
/** 聊天窗口在桌宠的哪一边 */
export type Side = 'left' | 'right';
export type Role = 'user' | 'assistant';
export type FinishReason = 'stop' | 'length' | 'cancelled';

export interface CharacterView {
  id: string;
  name: string;
  color: string;
}

export interface Appearance {
  images: Record<string, string>;
  placeholder: boolean;
}

export interface PetInit {
  character: CharacterView;
  appearance: Appearance | null;
  idle_animation: boolean;
  dnd: boolean;
}

export interface ChatMessage {
  role: Role;
  content: string;
}

export interface ChatInit {
  character: CharacterView;
  mode: ChatMode;
  side: Side;
  history: ChatMessage[];
}

export interface ErrorInfo {
  kind: string;
  message: string;
  detail: string;
  hint: string;
}

export type ChatEvent =
  | { type: 'delta'; text: string }
  | { type: 'done'; text: string; finish: FinishReason }
  | ({ type: 'error' } & ErrorInfo);

// ---------------- 设置窗口 ----------------

export interface General {
  always_on_top: boolean;
  pet_scale: number;
  idle_animation: boolean;
  dnd: boolean;
}

export interface Price {
  input: number;
  cached_input: number;
  cache_write: number;
  output: number;
}

export interface BackendConfig {
  id: string;
  kind: string;
  name: string;
  base_url: string;
  model: string;
  reasoning: string;
  price: Price;
}

export interface BackendView {
  config: BackendConfig;
  kind_name: string;
  needs_key: boolean;
  key_url: string;
  note: string;
  key_mask: string | null;
  removable: boolean;
}

export interface Budget {
  daily_usd: number;
  monthly_usd: number;
}

export interface SettingsView {
  general: General;
  backends: BackendView[];
  kinds: { kind: string; display_name: string }[];
  chat_backend: string;
  background_backend: string;
  budget: Budget;
  data_dir: string;
  version: string;
}

export interface TestResult {
  ok: boolean;
  latency_ms: number;
  reply: string;
  error_kind: string;
  error_detail: string;
  hint: string;
}

export interface UsageRow {
  backend_id: string;
  model: string;
  requests: number;
  input_tokens: number;
  cached_input_tokens: number;
  output_tokens: number;
  cost_usd: number;
}

export interface UsageView {
  today: string;
  month: string;
  spent_today: number;
  spent_month: number;
  budget: Budget;
  rows_today: UsageRow[];
  rows_month: UsageRow[];
  rows_all: UsageRow[];
}
