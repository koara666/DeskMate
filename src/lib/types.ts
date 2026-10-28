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
  /** 待机图片变体的偏好（心情、天气、夜里、季节），用第一个有图的 */
  variants?: string[];
  /** 心情（-100~100） */
  mood?: number;
  idle_animation: boolean;
  dnd: boolean;
}

export interface ChatMessage {
  role: Role;
  content: string;
}

/** 聊天记录里的一条（created_at 是 Unix 秒） */
export interface HistoryMessage extends ChatMessage {
  created_at: number;
}

export interface ChatInit {
  character: CharacterView;
  mode: ChatMode;
  placement: Placement;
  history: HistoryMessage[];
  /** 没有聊天记录时角色说的开场白 */
  greeting: string;
  /** 需要告诉用户的系统提示（比如数据库从备份恢复了），只出现一次 */
  notice: string | null;
}

export type MarkerKind = 'task' | 'open' | 'read';

/** 气泡里的动作按钮（角色提议打开 / 读取，用户点了才做） */
export interface ActionView {
  kind: MarkerKind;
  target: string;
  label: string;
  /** 不能做的原因（有原因时不显示按钮） */
  error: string | null;
}

export type ProactiveKind = 'reminder' | 'care' | 'briefing' | 'activity' | 'random';

/** 角色主动说的话（经过调度器） */
export interface ProactiveMessage {
  id: number;
  kind: ProactiveKind;
  text: string;
  /** 弹出了气泡（false = 全屏时改发了系统通知） */
  popped: boolean;
  /** 小事件的选项 */
  choices?: string[];
  /** 气泡下面的一行小字（比如「看了「Hades」· 遮住 2 处」） */
  note?: string;
  /** 有新版本：显示更新卡片 */
  update?: UpdateView;
  /** 聊天窗口原来是关着的：只显示「她说的话」气泡（没有输入框，点一下变成聊天框） */
  speech?: boolean;
}

/** 头顶气泡菜单 */
export interface MenuItem {
  id: string;
  icon: string;
  label: string;
  on: boolean;
}

export interface MenuShow {
  items: MenuItem[];
  /** 放在桌宠下面（气泡往下排） */
  below: boolean;
  /** 按住右键打开的 */
  hold: boolean;
}

export interface CompanionStatus {
  character: string;
  level: number;
  points: number;
  next: number | null;
  title: string;
  mood: number;
  mood_word: string;
  nickname: string;
  call: string;
}

export type GameOutcome = 'win' | 'lose' | 'draw' | 'done';
export type GameEvent =
  | 'start'
  | 'round_win'
  | 'round_lose'
  | 'round_draw'
  | 'win'
  | 'lose'
  | 'draw'
  | 'lead'
  | 'behind'
  | 'special';

export interface GameStats {
  game: string;
  win: number;
  lose: number;
  draw: number;
  played: number;
}

export interface Fortune {
  day: string;
  level: string;
  name: string;
  color: string;
  item: string;
  direction: string;
  comment: string;
}

export interface LlmGameStart {
  session: string;
  intro: string;
  puzzle: string | null;
  limit: number;
}

export interface LlmGameTurn {
  line: string;
  answer: string | null;
  idiom: string | null;
  ended: GameOutcome | null;
  reveal: string | null;
  asked: number;
  limit: number;
}

export interface Diary {
  id: number;
  day: string;
  character_id: string;
  content: string;
  created_at: number;
}

export interface ErrorInfo {
  kind: string;
  message: string;
  detail: string;
  hint: string;
}

export type ChatEvent =
  | { type: 'delta'; text: string }
  | {
      type: 'done';
      text: string;
      finish: FinishReason;
      task?: string | null;
      actions?: ActionView[];
      /** 用户吩咐的提醒（等用户确认） */
      reminder?: ReminderDraft;
      /** 没听懂的提醒：给「手动设置」按钮 */
      reminder_form?: boolean;
      /** 用户想玩游戏：打开这个游戏（空串 = 游戏列表） */
      open_game?: string;
    }
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
  companion: CompanionSettings;
  advanced: AdvancedSettings;
  briefing: BriefingSettings;
  activity: ActivitySettings;
  care: CareSettings;
  vision: VisionSettings;
  update: UpdateSettings;
}

export interface UpdateSettings {
  auto_check: boolean;
  skipped: string;
  last_check: number;
}

/** 有新版本 */
export interface UpdateView {
  version: string;
  current: string;
  /** 更新说明（纯文本） */
  notes: string;
  page: string;
  can_install: boolean;
}

export interface UpdateProgress {
  phase: 'downloading' | 'verifying' | 'launching' | 'failed';
  percent: number;
  message?: string;
}

export interface VisionSettings {
  enabled: boolean;
  backend: string;
  hotkey: string;
  /** 主动看：在做这些事时偶尔看一眼（game / video / coding / writing） */
  auto_categories: string[];
  /** 不主动看的程序（程序名 / 游戏名） */
  exclude: string[];
  interval_minutes: number;
  daily_max: number;
  /** 发送前在本机用文字识别涂黑敏感文字 */
  mask_text: boolean;
}

/** 「预览她会看到什么」：只在设置窗口里显示，不发送、不保存 */
export interface VisionPreview {
  image: string | null;
  app: string;
  masked: number;
  ocr_missing: boolean;
  refused: string | null;
}

/** 本次运行里看过的记录（不存图、不存标题） */
export interface LookRecord {
  at: number;
  trigger: 'hotkey' | 'auto' | 'preview';
  app: string;
  outcome: string;
  masked: number;
}

export type ActivityCategory =
  | 'game'
  | 'coding'
  | 'writing'
  | 'video'
  | 'music'
  | 'browsing'
  | 'chat'
  | 'meeting'
  | 'other'
  | 'idle';
export type ActivityAction = 'comment' | 'quiet' | 'avoid';

export interface ActivityRule {
  /** 分类 id 或进程名（xxx.exe） */
  target: string;
  action: ActivityAction;
}

export interface ActivitySettings {
  enabled: boolean;
  comments: boolean;
  record: boolean;
  rules: ActivityRule[];
  private_apps: string[];
}

export interface ActivityTotal {
  category: string;
  label: string;
  secs: number;
}

export interface ActivityOverview {
  current: { category: ActivityCategory; label: string | null; since: number } | null;
  recent: [string, ActivityCategory][];
  days: [string, ActivityTotal[]][];
}

export interface CareSettings {
  sit: boolean;
  sit_minutes: number;
  water: boolean;
  water_minutes: number;
  late: boolean;
  late_time: string;
  meal: boolean;
}

export interface City {
  name: string;
  country: string;
  lat: number;
  lon: number;
  tz: string;
}

export interface Feed {
  name: string;
  url: string;
  enabled: boolean;
}

export interface SpecialDate {
  /** MM-DD */
  date: string;
  label: string;
}

export type Region = 'Auto' | 'CN' | 'JP' | 'US' | 'Other';

export interface BriefingSettings {
  on_startup: boolean;
  times: string[];
  weather: boolean;
  holiday: boolean;
  meal: boolean;
  news: boolean;
  reminders: boolean;
  special: boolean;
  weather_in_chat: boolean;
  city: City | null;
  region: Region;
  feeds: Feed[];
  special_dates: SpecialDate[];
}

export type Frequency = 'rare' | 'sometimes' | 'often';

export interface CompanionSettings {
  enabled: boolean;
  cooldown_minutes: number;
  frequency: Frequency;
  quiet_enabled: boolean;
  quiet_start: string;
  quiet_end: string;
  daily_calls: number;
  daily_usd: number;
  events: boolean;
  system_notify: boolean;
}

export interface ProgramEntry {
  name: string;
  path: string;
}

export interface AdvancedSettings {
  allow_delegation: boolean;
  read_dirs: string[];
  programs: ProgramEntry[];
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
  /** 原作台词库（quotes.md）：常驻段数 / 按关键词取用的段数 */
  quotes_fixed: number;
  quotes_keyed: number;
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

// ---------------- 提醒 ----------------

export type Trigger = 'at' | 'startup' | 'activity';

/** 重复规则（星期一 = 1 … 星期日 = 7） */
export type Repeat =
  | { type: 'none' }
  | { type: 'daily' }
  | { type: 'weekdays' }
  | { type: 'weekly'; days: number[] }
  | { type: 'monthly'; day: number };

export interface Reminder {
  id: number;
  content: string;
  trigger: Trigger;
  /** at：下次提醒时间；startup：从哪天起（那天 0 点）；Unix 秒 */
  at: number | null;
  repeat: Repeat;
  activity: string;
  line: string;
  important: boolean;
  status: 'active' | 'paused' | 'done';
  created_at: number;
  last_fired_at: number | null;
}

/** 列表里的提醒：多一句「什么时候」 */
export interface ReminderView extends Reminder {
  when: string;
}

export interface ReminderDraft {
  reminder: Reminder;
  when: string;
}

export interface SavedReminder {
  reminder: ReminderView;
  line: string;
}
