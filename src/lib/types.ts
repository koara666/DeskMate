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
  /** 角色自己的界面（皮肤 + 颜色）；测试里可以不给 */
  theme?: Theme;
}

/** 角色的界面：skin = 程序自带的皮肤（default / arcade / lab），colors = 颜色变量 */
export interface Theme {
  skin: string;
  colors: Record<string, string>;
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
  /** 正在专注计时（计划）：桌宠身上显示还剩多久 */
  focus?: PlanFocus | null;
}

/** 专注计时（计划） */
export interface PlanFocus {
  task_id: number;
  title: string;
  /** 结束时间（Unix 秒） */
  ends_at: number;
  minutes: number;
  /** 后端的「现在」（Unix 秒），用来对齐时钟 */
  now: number;
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

export type MarkerKind = 'task' | 'open' | 'read' | 'checkin' | 'plan';

/** 气泡里的动作按钮（角色提议打开 / 读取，用户点了才做） */
export interface ActionView {
  kind: MarkerKind;
  target: string;
  label: string;
  /** 不能做的原因（有原因时不显示按钮） */
  error: string | null;
}

export type ProactiveKind = 'reminder' | 'plan' | 'care' | 'briefing' | 'activity' | 'random';

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
  theme?: Theme;
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
  /** 回复下面的一行小字（比如「上网查了……」） */
  | { type: 'note'; text: string }
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
  web: WebSettings;
  voice: VoiceSettings;
  /** 语音输入（对着麦克风说话） */
  asr: AsrSettings;
  /** 现在是哪个角色（声音页按角色分别保存 GPT-SoVITS 的设置） */
  character_id: string;
  /** 当前角色的预录语音有多少句（0 = 没有语音包） */
  voice_pack: number;
  /** 语音服务 Key 的末尾 4 位：[Fish Audio, OpenAI 兼容] */
  voice_keys: [string | null, string | null];
}

export interface SovitsProfile {
  url: string;
  ref_audio: string;
  ref_text: string;
  ref_lang: string;
}

export interface VoiceSettings {
  enabled: boolean;
  /** '' = 还没选；system / fish / sovits / openai */
  engine: string;
  volume: number;
  max_chars: number;
  speed: number;
  /** 她说话的语言：ja（默认，气泡照常中文）/ zh */
  lang: string;
  scenes: { reactions: boolean; proactive: boolean; chat: boolean; games: boolean };
  fullscreen_voice: boolean;
  daily_chars: number;
  system_voice: string;
  system_pitch: number;
  fish_url: string;
  fish_model: string;
  fish_engine: string;
  sovits_url: string;
  sovits_ref_audio: string;
  sovits_ref_text: string;
  sovits_ref_lang: string;
  /** 每个角色自己的 GPT-SoVITS 设置（角色 id → 设置），优先于上面通用的那一份 */
  sovits_profiles: Record<string, SovitsProfile>;
  openai_url: string;
  openai_model: string;
  openai_voice: string;
}

/** 语音输入：对着麦克风说话，本机识别（中文模型），变成一条聊天消息。默认关闭 */
export interface AsrSettings {
  enabled: boolean;
  /** 识别完直接发给她；关掉就先放进输入框 */
  auto_send: boolean;
  /** 连续对话：她说完话自己接着听 */
  continuous: boolean;
  /** 麦克风（空 = 系统默认） */
  input_device: string;
  hotkey_enabled: boolean;
  hotkey: string;
}

export interface AsrDownload {
  running: boolean;
  done: number;
  total: number;
  error: string;
}

export interface AsrDevice {
  id: string;
  name: string;
}

export interface AsrStatus {
  model_ready: boolean;
  download: AsrDownload;
  listening: boolean;
  devices: AsrDevice[];
}

export type AsrPhase = 'idle' | 'loading' | 'listening' | 'hearing' | 'thinking';

/** 录音状态；`note` 是要告诉用户的一句话（比如「没听到你说话」） */
export interface AsrStateEvent {
  state: AsrPhase;
  level: number;
  note: string;
}

export interface AsrTextEvent {
  text: string;
  auto_send: boolean;
}

/** 联网搜索：开了以后问新闻、最新消息或说「搜一下」，她会先上网查 */
export interface WebSettings {
  enabled: boolean;
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
  /** 切到新界面（比如刚打开一个游戏）时看一眼，由她决定说不说 */
  on_switch: boolean;
  /** 按时间定时看（做这些事满 5 分钟后每隔一阵看一眼） */
  timed_look: boolean;
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
  trigger: 'hotkey' | 'auto' | 'switch' | 'preview';
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

/** 可下载的角色（选了哪位才下载哪位；初始角色七海千秋随安装包带着、全新安装直接装上；登记表编在程序里，没下载也能看到） */
export interface PackView {
  id: string;
  name: string;
  description: string;
  color: string;
  /** 下载大小（字节） */
  size: number;
  /** 安装包自带的小预览图（data URL），没有就是空 */
  preview: string;
  installed: boolean;
  installed_version: number;
  latest_version: number;
  /** 装的是旧版本，有新的可以更新 */
  update: boolean;
  running: boolean;
  done: number;
  total: number;
  error: string;
}

/** 一句没念出来的话和原因（设置页「声音」里显示） */
export interface VoiceSkip {
  /** 时间（Unix 秒） */
  at: number;
  /** 那句话的开头 */
  text: string;
  reason: string;
}

/** 下载进度事件（charpack-progress） */
export interface PackProgress {
  id: string;
  running: boolean;
  done: number;
  total: number;
  error: string;
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

// ---------------- 计划（src-tauri/src/plans） ----------------

export type PlanMode = 'content' | 'habit';
export type PlanPace = 'even' | 'asap';
export type PlanStatus = 'active' | 'paused' | 'done' | 'archived';

export interface Plan {
  id: number;
  title: string;
  /** 用户新建时写的原话 */
  goal: string;
  emoji: string;
  mode: PlanMode;
  start_day: string;
  /** 截止（空 = 没有） */
  end_day: string;
  /** 周一到周日每天能用多少分钟（0 = 那天不排） */
  week_minutes: number[];
  off_days: string[];
  pace: PlanPace;
  /** "HH:MM"，空 = 不固定 */
  remind_at: string;
  supervise: boolean;
  /** 她的安排思路 */
  strategy: string;
  status: PlanStatus;
  sort: number;
  created_at: number;
  updated_at: number;
  /** 属于哪个计划书（是其中一条线；null = 单独的计划） */
  goal_id?: number | null;
  /** 在计划书里默认占几份 */
  share?: number;
}

export interface PlanItem {
  id: number;
  plan_id: number;
  title: string;
  detail: string;
  minutes: number;
  status: 'todo' | 'done' | 'skipped';
  sort: number;
  done_at: number | null;
  /** 属于计划书的哪个阶段 */
  phase_id?: number | null;
  /** 最早哪天开始 / 自己的截止 / 钉在哪天（空 = 没有） */
  after_day?: string;
  due_day?: string;
  pin_day?: string;
  /** 还是路线图里粗的一大块 */
  rough?: boolean;
}

export interface PlanItemView extends PlanItem {
  spent: number;
  left: number;
}

export interface PlanTask {
  id: number;
  plan_id: number | null;
  item_id: number | null;
  day: string;
  orig_day: string;
  title: string;
  minutes: number;
  start: string;
  status: 'todo' | 'done' | 'skipped';
  spent: number;
  finishes: boolean;
  note: string;
  source: 'auto' | 'user';
  sort: number;
  reminded: boolean;
  done_at: number | null;
  created_at: number;
}

export interface PlanTaskView extends PlanTask {
  /** 计划的名字（带图标；单独的事情为空） */
  label: string;
}

export interface PlanNote {
  id: number;
  plan_id: number | null;
  day: string;
  content: string;
  created_at: number;
}

export interface PlanProgress {
  total_minutes: number;
  done_minutes: number;
  items_total: number;
  items_done: number;
  percent: number;
  /** 预计哪天做完 */
  finish: string;
  /** 到截止还差多少分钟 */
  short_minutes: number;
  overdue: boolean;
  streak: number;
  week_days: number;
  week_done: number;
}

export interface PlanDayStat {
  day: string;
  planned: number;
  done: number;
  tasks: number;
  tasks_done: number;
  tasks_todo: number;
}

export interface PlanUpcomingEntry {
  plan_id: number | null;
  label: string;
  title: string;
  minutes: number;
  /** 已经定下的（挪过去的、提前加的）；其余是推算的 */
  stored: boolean;
}

export interface PlanUpcoming {
  day: string;
  entries: PlanUpcomingEntry[];
}

export interface PlanSummary {
  plan: Plan;
  progress: PlanProgress;
}

export interface PlanView {
  plan: Plan;
  items: PlanItemView[];
  progress: PlanProgress;
  upcoming: PlanUpcoming[];
  history: PlanDayStat[];
  notes: PlanNote[];
}

export interface PlanToday {
  day: string;
  tasks: PlanTaskView[];
  notes: PlanNote[];
  streak: number;
  planned: number;
  done: number;
  upcoming: PlanUpcoming[];
  plans: PlanSummary[];
  /** 她对今天的一句话 */
  say: string;
}

/** 统计页（后端 plans/stats.rs）：只统计到今天为止 */
/** 日历上的一条（提前知道的事：作业截止、停课……），见 plans/agenda.rs */
export interface AgendaEntry {
  id: number;
  /** YYYY-MM-DD */
  day: string;
  title: string;
  note: string;
  /** 搞定了（不再提醒） */
  done: boolean;
  done_at: number | null;
  created_at: number;
  updated_at: number;
}

export interface PlanStats {
  today: string;
  /** 热力图第一格的日子（周一）；heat 从这天起一天一格，到今天为止 */
  heat_start: string;
  heat: { day: string; minutes: number; done: number; total: number }[];
  /** 堆叠柱里的颜色：p<计划 id> / other（其他计划）/ loose（随手的事） */
  series: { key: string; label: string }[];
  /** 最近 8 周，旧的在前，最后一个是本周 */
  weeks: { start: string; minutes: number; parts: { key: string; minutes: number }[] }[];
  numbers: {
    week_minutes: number;
    last_week_minutes: number;
    /** 过去 28 天的完成率（百分数）；没安排过为 null */
    rate: number | null;
    rate_done: number;
    rate_total: number;
    streak: number;
    best_streak: number;
  };
  plans: {
    plan_id: number;
    title: string;
    emoji: string;
    status: PlanStatus;
    goal_id: number | null;
    goal_title: string | null;
    /** 最近 4 周每周做完的分钟，旧的在前 */
    weeks: number[];
    total_minutes: number;
    done_items: number;
  }[];
  review: string;
}

/** 新建计划时用户已经定好的（模型不能改） */
export interface PlanFixed {
  title: string;
  mode: PlanMode | null;
  week_minutes: number[] | null;
  end_day: string;
  start_day: string;
  remind_at: string;
}

export interface PlanPreview {
  /** 前两周每天排什么：[日期, [[标题, 分钟]]] */
  days: [string, [string, number][]][];
  finish: string;
  short_minutes: number;
  total_minutes: number;
  per_week: string;
}

export interface PlanDraft {
  plan: Plan;
  items: PlanItem[];
  /** ai = 模型安排的，local = 本地规则拆的 */
  source: 'ai' | 'local';
  notes: string[];
  preview: PlanPreview;
}

export type PlanStrictness = 'gentle' | 'normal' | 'strict';

export interface PlanSettings {
  supervise: boolean;
  strictness: PlanStrictness;
  start_remind: boolean;
  distraction: boolean;
  checkin: boolean;
  checkin_at: string;
  celebrate: boolean;
  chat_context: boolean;
  briefing: boolean;
  focus_minutes: number;
  /** 计划书：节点当天问达标没有 */
  milestone_check: boolean;
  /** 计划书：每周自查 */
  review: boolean;
  /** 节点、自查几点开始问 */
  review_at: string;
}

export interface PlanBoot {
  character: CharacterView;
  /** 她的几张图（idle / happy / think …） */
  images: Record<string, string>;
  /** 先显示哪一页：today / new / new:<文字> / plan:<id> / settings */
  view: string | null;
  settings: PlanSettings;
  today: string;
}

export interface PlanDoneEffect {
  all_today: boolean;
  plan_finished: boolean;
}

// ---------------- 计划书（src-tauri/src/plans/goal.rs） ----------------

export interface GoalBuffer {
  start: string;
  end: string;
  /** 按平时的百分之几排（0 = 不排） */
  percent: number;
  note: string;
}

export interface GoalResource {
  title: string;
  url: string;
  note: string;
  plan_id: number | null;
  phase_id: number | null;
}

export interface GoalQa {
  q: string;
  a: string;
}

export interface Goal {
  id: number;
  title: string;
  emoji: string;
  summary: string;
  /** 用户的原话 */
  source: string;
  /** 计划书原文（可以整段改） */
  doc: string;
  strategy: string;
  start_day: string;
  end_day: string;
  /** 平时周一到周日每天一共能用多少分钟 */
  week_minutes: number[];
  off_days: string[];
  buffers: GoalBuffer[];
  resources: GoalResource[];
  checklist: string[];
  answers: GoalQa[];
  remind_at: string;
  /** 每周哪天自查（0 = 周一 … 6 = 周日；-1 = 不自查） */
  review_day: number;
  status: PlanStatus;
  sort: number;
  created_at: number;
  updated_at: number;
  expanded_until: string;
}

export interface GoalPhase {
  id: number;
  goal_id: number;
  title: string;
  start_day: string;
  end_day: string;
  objective: string;
  criteria: string[];
  /** 这个阶段每天一共多少分钟（null = 用计划书的） */
  week_minutes: number[] | null;
  /** 线的 id → 份额（空 = 用每条线自己的） */
  shares: Record<string, number>;
  sort: number;
}

export type MilestoneStatus = 'todo' | 'passed' | 'missed' | 'skipped';

export interface GoalMilestone {
  id: number;
  goal_id: number;
  phase_id: number | null;
  plan_id: number | null;
  title: string;
  day: string;
  criteria: string;
  status: MilestoneStatus;
  result: string;
  checked_at: number | null;
  sort: number;
}

export interface GoalTrack {
  plan: Plan;
  items: PlanItem[];
}

export interface GoalFull {
  goal: Goal;
  tracks: GoalTrack[];
  phases: GoalPhase[];
  milestones: GoalMilestone[];
}

/** 某条线在某个阶段的量 */
export interface GoalCell {
  plan_id: number;
  share: number;
  total: number;
  done: number;
  items: number;
  items_done: number;
  rough: number;
  finish: string;
}

export interface GoalPhaseView {
  phase: GoalPhase;
  state: 'past' | 'current' | 'future';
  cells: GoalCell[];
  total: number;
  done: number;
  percent: number;
  finish: string;
}

export interface GoalItemViz {
  id: number;
  phase_id: number | null;
  title: string;
  minutes: number;
  spent: number;
  status: 'todo' | 'done' | 'skipped';
  rough: boolean;
  pin_day: string;
  due_day: string;
  finish: string;
}

export interface GoalTrackViz {
  plan_id: number;
  routine: boolean;
  total: number;
  done: number;
  percent: number;
  finish: string;
  /** 不属于任何阶段的内容 */
  loose: GoalCell;
  items: GoalItemViz[];
}

export interface GoalShort {
  lane: number;
  minutes: number;
  by: string;
}

export interface GoalViz {
  phases: GoalPhaseView[];
  tracks: GoalTrackViz[];
  short: GoalShort[];
  /** 钉在已经过去的日子还没做的：[线, 条目] */
  missed: [number, number][];
  total: number;
  done: number;
  percent: number;
  finish: string;
  current_phase: number | null;
  /** 往后两周：[日期, [线, 标题, 分钟][]] */
  days: [string, [number, string, number][]][];
}

export interface GoalTrackWeek {
  plan_id: number;
  planned: number;
  done: number;
  tasks: number;
  tasks_done: number;
}

export interface GoalWeekStat {
  from: string;
  to: string;
  planned: number;
  done: number;
  tasks: number;
  tasks_done: number;
  tracks: GoalTrackWeek[];
}

export interface GoalView {
  full: GoalFull;
  viz: GoalViz;
  week: GoalWeekStat;
  notes: PlanNote[];
}

export interface GoalSummary {
  goal: Goal;
  percent: number;
  tracks: number[];
  phase: string;
  /** 下一个节点：[名字, 日期] */
  next: [string, string] | null;
  short: boolean;
}

export interface GoalDraft {
  full: GoalFull;
  viz: GoalViz;
  source: 'ai' | 'local';
  notes: string[];
}

export interface PlanQuestion {
  q: string;
  options: string[];
}

/** 「让她来安排」的结果 */
export type PlanThink =
  | { kind: 'plan'; draft: PlanDraft }
  | { kind: 'questions'; questions: PlanQuestion[] }
  | { kind: 'goal'; draft: GoalDraft };
