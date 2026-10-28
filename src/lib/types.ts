// 与 Rust 端（src-tauri/src/commands.rs、chat.rs）保持一致的类型

export type PetState = 'idle' | 'thinking' | 'talking' | 'happy' | 'sad' | 'sleep';
export type ChatMode = 'compact' | 'expanded';
/** 聊天窗口在桌宠的哪一边 */
export type Side = 'left' | 'right';

/** 聊天窗口在桌宠哪一侧、和桌宠顶部还是底部对齐（决定气泡尾巴的位置） */
export interface Placement {
  side: Side;
  top: boolean;
}
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
  placement: Placement;
  history: ChatMessage[];
  /** 没有聊天记录时角色说的开场白 */
  greeting: string;
  /** 需要告诉用户的系统提示（比如数据库从备份恢复了），只出现一次 */
  notice: string | null;
}

export interface ErrorInfo {
  kind: string;
  message: string;
  detail: string;
  hint: string;
}

export type ChatEvent =
  | { type: 'delta'; text: string }
  | { type: 'done'; text: string; finish: FinishReason; task?: string | null }
  | ({ type: 'error' } & ErrorInfo);

// ---------------- 设置窗口 ----------------

export interface General {
  always_on_top: boolean;
  pet_scale: number;
  idle_animation: boolean;
  dnd: boolean;
  /** 显卡加速（重启后生效） */
  gpu_acceleration: boolean;
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
  initial_tab: string | null;
  daily: DailySettings;
}

export interface DailySettings {
  autostart: boolean;
  hotkey_enabled: boolean;
  hotkey: string;
  hide_in_fullscreen: boolean;
  sleep_minutes: number;
  proactive: boolean;
  proactive_minutes: number;
  proactive_daily_max: number;
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

// ---------------- 角色设置 ----------------

export interface CharacterPrefs {
  banned_words: string[];
  custom_rules: string[];
}

export interface CharacterItem {
  id: string;
  name: string;
  description: string;
  source: string;
  color: string;
  origin: 'builtin' | 'user';
  preview: string;
  user_images: string[];
  prefs: CharacterPrefs;
}

export interface CharactersView {
  current: string;
  list: CharacterItem[];
  global: CharacterPrefs;
  states: string[];
  user_root: string;
}

// ---------------- 记忆 ----------------

export interface Memory {
  id: number;
  kind: 'fact' | 'summary';
  content: string;
  scope: string;
  category: string;
  source_kind: string;
  source_ref: string;
  trusted: boolean;
  importance: number;
  created_at: number;
  updated_at: number;
  last_used_at: number | null;
  use_count: number;
  status: 'active' | 'superseded';
  superseded_by: number | null;
  day: string;
}

export interface MemoryFilter {
  query: string;
  kind: string;
  scope: string;
  include_history: boolean;
  limit: number;
  offset: number;
}

export interface MemoryOverview {
  stats: { facts: number; summaries: number; history: number; conversations: number };
  paused: boolean;
  last_backup: string | null;
  scopes: { id: string; name: string }[];
}

// ---------------- 任务执行 ----------------

export type Executor = 'claude' | 'codex';
export type StepKind = 'read' | 'edit' | 'command' | 'network' | 'outside' | 'note' | 'other';

export interface ApprovalItem {
  kind: StepKind;
  detail: string;
  rule: string | null;
  path: string | null;
}

export type TaskEvent =
  | { type: 'started'; task_id: string; executor: string }
  | { type: 'step'; kind: StepKind; text: string }
  | { type: 'needs_approval'; summary: string; items: ApprovalItem[] }
  | { type: 'done'; summary: string; cost_usd: number; changed: string[]; backup: string | null }
  | { type: 'failed'; message: string; detail: string };

export interface TaskSettings {
  executor: Executor;
  root: string;
  timeout_minutes: number;
  budget_usd: number;
  claude_model: string;
  codex_model: string;
  claude_path: string;
  codex_path: string;
}

export interface ExecutorInfo {
  found: boolean;
  path: string;
  version: string;
}

export interface TaskDefaults {
  settings: TaskSettings;
  root: string;
  claude: ExecutorInfo;
  codex: ExecutorInfo;
}

export interface TaskRecord {
  id: string;
  created_at: number;
  finished_at: number | null;
  executor: string;
  goal: string;
  workdir: string;
  status: string;
  summary: string;
  cost_usd: number;
}
