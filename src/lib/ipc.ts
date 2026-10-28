// 前端与 Rust 的唯一通道。组件只依赖这里的接口，测试时可以替换成假实现。
import { Channel, invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type {
  MenuShow,
  VoiceSettings,
  UpdateProgress,
  UpdateView,
  UpdateSettings,
  VisionSettings,
  VisionPreview,
  LookRecord,
  LlmGameStart,
  LlmGameTurn,
  Fortune,
  GameEvent,
  GameOutcome,
  GameStats,
  CompanionStatus,
  Diary,
  ActivityOverview,
  ActivitySettings,
  CareSettings,
  BriefingSettings,
  City,
  Reminder,
  ReminderView,
  SavedReminder,
  AdvancedSettings,
  CompanionSettings,
  ProactiveMessage,
  BackendConfig,
  Budget,
  DailySettings,
  CharacterPrefs,
  CharactersView,
  ChatEvent,
  ChatInit,
  ChatMode,
  Executor,
  TaskDefaults,
  TaskEvent,
  TaskRecord,
  TaskSettings,
  General,
  Memory,
  MemoryFilter,
  MemoryOverview,
  PetInit,
  PetState,
  SettingsView,
  Placement,
  TestResult,
  UsageView,
} from './types';

export interface ChatApi {
  init(): Promise<ChatInit>;
  send(text: string, requestId: string, onEvent: (e: ChatEvent) => void): Promise<void>;
  regenerate(requestId: string, onEvent: (e: ChatEvent) => void): Promise<void>;
  stop(requestId: string): Promise<boolean>;
  setMode(mode: ChatMode): Promise<void>;
  /** 气泡模式：告诉后端内容需要多高，窗口跟着变 */
  fit(height: number): Promise<void>;
  hide(): Promise<void>;
  openSettings(): Promise<void>;
  onPlacement(cb: (p: Placement) => void): Promise<UnlistenFn>;
  onFocusInput(cb: () => void): Promise<UnlistenFn>;
  onCharacterChanged(cb: () => void): Promise<UnlistenFn>;
  /** 角色主动说话（经过调度器） */
  onProactive(cb: (m: ProactiveMessage) => void): Promise<UnlistenFn>;
  /** 右键菜单「一起玩…」（可能带游戏 id） */
  onOpenGames(cb: (game?: string) => void): Promise<UnlistenFn>;
  /** 气泡里点了「打开」 */
  open(target: string): Promise<string>;
  /** 气泡里点了「读取」：读完由角色说说内容（流式） */
  read(path: string, requestId: string, onEvent: (e: ChatEvent) => void): Promise<void>;
  /** 主动发言的结果：用户理会了 / 无人理会自动收起 */
  feedback(engaged: boolean): Promise<void>;
  /** 确认（或手动填写）提醒 */
  saveReminder(reminder: Reminder): Promise<SavedReminder>;
  /** 小事件里选了一项：返回角色的回应 */
  choose(id: number, index: number): Promise<string>;
  games: GameApi;
  /** 检查更新：下载安装、跳过（不传时气泡里不显示更新按钮） */
  update?: UpdateApi;
  /** 头顶气泡里点了「提醒」：她说一句，下面给提醒表单 */
  onReminderForm?(cb: (text: string) => void): Promise<UnlistenFn>;
}

/** 更新：只能安装后端上次查到的版本（前端不能指定下载地址） */
export interface UpdateApi {
  install(): Promise<void>;
  skip(version: string): Promise<void>;
  onProgress(cb: (p: UpdateProgress) => void): Promise<UnlistenFn>;
}

export const updateApi: UpdateApi = {
  install: () => invoke('update_install'),
  skip: (version) => invoke('update_skip', { version }),
  onProgress: (cb) => listen<UpdateProgress>('update-progress', (e) => cb(e.payload)),
};

/** 小游戏用的接口（规则在前端，后端只管台词、战绩、心情、亲密度） */
export interface GameApi {
  say(game: string, event: GameEvent, name: string, vars?: [string, string][]): Promise<string | null>;
  finish(game: string, outcome: GameOutcome): Promise<void>;
  stats(): Promise<GameStats[]>;
  fortuneToday(): Promise<Fortune | null>;
  fortuneDraw(): Promise<Fortune>;
  /** 需要模型的小游戏 */
  llmStart(game: string): Promise<LlmGameStart>;
  llmTurn(session: string, kind: 'ask' | 'guess' | 'idiom', input: string): Promise<LlmGameTurn>;
  llmGiveUp(session: string): Promise<LlmGameTurn>;
}

export const gameApi: GameApi = {
  say: (game, event, name, vars = []) => invoke<string | null>('game_say', { game, event, name, vars }),
  finish: (game, outcome) => invoke('game_finish', { game, outcome }),
  stats: () => invoke<GameStats[]>('game_stats'),
  fortuneToday: () => invoke<Fortune | null>('fortune_today'),
  fortuneDraw: () => invoke<Fortune>('fortune_draw'),
  llmStart: (game) => invoke<LlmGameStart>('llm_game_start', { game }),
  llmTurn: (session, kind, input) => invoke<LlmGameTurn>('llm_game_turn', { session, kind, input }),
  llmGiveUp: (session) => invoke<LlmGameTurn>('llm_game_give_up', { session }),
};

function channel(onEvent: (e: ChatEvent) => void): Channel<ChatEvent> {
  const ch = new Channel<ChatEvent>();
  ch.onmessage = onEvent;
  return ch;
}

export const chatApi: ChatApi = {
  init: () => invoke<ChatInit>('chat_init'),
  send: (text, requestId, onEvent) =>
    invoke('chat_send', { text, requestId, onEvent: channel(onEvent) }),
  regenerate: (requestId, onEvent) =>
    invoke('chat_regenerate', { requestId, onEvent: channel(onEvent) }),
  stop: (requestId) => invoke<boolean>('chat_stop', { requestId }),
  setMode: (mode) => invoke('chat_set_mode', { mode }),
  fit: (height) => invoke('chat_fit', { height }),
  hide: () => invoke('chat_hide'),
  openSettings: () => invoke('open_settings'),
  onPlacement: (cb) => listen<Placement>('chat-placement', (e) => cb(e.payload)),
  onFocusInput: (cb) => listen('chat-focus-input', () => cb()),
  onCharacterChanged: (cb) => listen('character-changed', () => cb()),
  onProactive: (cb) => listen<ProactiveMessage>('chat-proactive', (e) => cb(e.payload)),
  onOpenGames: (cb) => listen<string | null>('chat-open-games', (e) => cb(e.payload || undefined)),
  open: (target) => invoke<string>('ability_open', { target }),
  read: (path, requestId, onEvent) => invoke('ability_read', { path, requestId, onEvent: channel(onEvent) }),
  feedback: (engaged) => invoke('proactive_feedback', { engaged }),
  saveReminder: (reminder) => invoke<SavedReminder>('reminder_save', { reminder }),
  choose: (id, index) => invoke<string>('event_choose', { id, index }),
  games: gameApi,
  update: updateApi,
  onReminderForm: (cb) => listen<string>('chat-reminder-form', (e) => cb(e.payload)),
};

export interface PetApi {
  init(): Promise<PetInit>;
  click(): Promise<void>;
  /** 摸头（点头顶 / 在头上蹭）、拖动、按住不放、连戳的反应 */
  touch(part: TouchPart): Promise<void>;
  menu(): Promise<void>;
  /** 右键按下：头顶浮出气泡菜单 */
  menuOpen(hold: boolean): Promise<void>;
  startDrag(): Promise<void>;
  onState(cb: (s: PetState) => void): Promise<UnlistenFn>;
  onChanged(cb: () => void): Promise<UnlistenFn[]>;
}

export const petApi: PetApi = {
  init: () => invoke<PetInit>('pet_init'),
  click: () => invoke('pet_click'),
  touch: (part) => invoke('pet_touch', { part }),
  menu: () => invoke('pet_menu'),
  menuOpen: (hold) => invoke('menu_open', { hold }),
  startDrag: () => getCurrentWindow().startDragging(),
  onState: (cb) => listen<PetState>('pet-state', (e) => cb(e.payload)),
  onChanged: (cb) =>
    Promise.all([
      listen('settings-changed', () => cb()),
      listen('character-changed', () => cb()),
      listen('pet-variant', () => cb()),
    ]),
};

export type TouchPart = 'head' | 'rub' | 'drag' | 'hold' | 'poke';

/** 桌宠窗口里的语音播放（系统语音在这里合成；云端 / 本机服务合成的音频从后端取来播放） */
export interface VoiceApi {
  onSay(cb: (e: { text: string; voice: string; rate: number; pitch: number; volume: number }) => void): Promise<UnlistenFn>;
  onPlay(cb: (e: { id: string; volume: number }) => void): Promise<UnlistenFn>;
  onStop(cb: () => void): Promise<UnlistenFn>;
  audio(id: string): Promise<ArrayBuffer>;
}

export const voiceApi: VoiceApi = {
  onSay: (cb) => listen<{ text: string; voice: string; rate: number; pitch: number; volume: number }>('voice-say', (e) => cb(e.payload)),
  onPlay: (cb) => listen<{ id: string; volume: number }>('voice-play', (e) => cb(e.payload)),
  onStop: (cb) => listen('voice-stop', () => cb()),
  audio: (id) => invoke<ArrayBuffer>('voice_audio', { id }),
};

/** 头顶气泡菜单（单独的透明小窗口） */
export interface MenuApi {
  onShow(cb: (s: MenuShow) => void): Promise<UnlistenFn>;
  /** 按住右键时鼠标的位置（菜单窗口里的坐标） */
  onPointer(cb: (p: { x: number; y: number }) => void): Promise<UnlistenFn>;
  /** 按住的右键松开了 */
  onRelease(cb: (r: { x: number; y: number; held_ms: number }) => void): Promise<UnlistenFn>;
  pick(id: string): Promise<void>;
  close(heldMs: number): Promise<void>;
  focus(): Promise<void>;
}

export const menuApi: MenuApi = {
  onShow: (cb) => listen<MenuShow>('menu-show', (e) => cb(e.payload)),
  onPointer: (cb) => listen<{ x: number; y: number }>('menu-pointer', (e) => cb(e.payload)),
  onRelease: (cb) => listen<{ x: number; y: number; held_ms: number }>('menu-release', (e) => cb(e.payload)),
  pick: (id) => invoke('menu_pick', { id }),
  close: (heldMs) => invoke('menu_close', { heldMs }),
  focus: () => invoke('menu_focus'),
};

/** 生成请求编号（只含字母数字和 -，Rust 端会校验） */
export function newRequestId(): string {
  const rnd = Math.random().toString(36).slice(2, 10);
  return `r-${Date.now().toString(36)}-${rnd}`;
}

export const settingsApi = {
  get: () => invoke<SettingsView>('settings_get'),
  saveGeneral: (general: General) => invoke('settings_save_general', { general }),
  saveBackend: (config: BackendConfig) => invoke('backend_save', { config }),
  addBackend: (kind: string) => invoke<string>('backend_add', { kind }),
  deleteBackend: (id: string) => invoke('backend_delete', { id }),
  setKey: (id: string, key: string) => invoke<string>('backend_set_key', { id, key }),
  clearKey: (id: string) => invoke('backend_clear_key', { id }),
  select: (chatBackend: string, backgroundBackend: string) =>
    invoke('backend_select', { chatBackend, backgroundBackend }),
  test: (id: string) => invoke<TestResult>('backend_test', { id }),
  saveBudget: (budget: Budget) => invoke('budget_save', { budget }),
  saveDaily: (daily: DailySettings) => invoke('daily_save', { daily }),
  saveCompanion: (companion: CompanionSettings) => invoke('companion_save', { companion }),
  saveAdvanced: (advanced: AdvancedSettings) => invoke<AdvancedSettings>('advanced_save', { advanced }),
  saveBriefing: (briefing: BriefingSettings) => invoke<BriefingSettings>('briefing_save', { briefing }),
  citySearch: (query: string) => invoke<City[]>('city_search', { query }),
  briefingNow: () => invoke('briefing_now'),
  activityOverview: () => invoke<ActivityOverview>('activity_overview'),
  companionStatus: () => invoke<CompanionStatus>('companion_status'),
  saveVision: (vision: VisionSettings) => invoke<VisionSettings>('vision_save', { vision }),
  visionCost: () => invoke<[number, number]>('vision_cost'),
  visionPreview: () => invoke<VisionPreview>('vision_preview'),
  visionRecords: () => invoke<LookRecord[]>('vision_records'),
  updateCheck: () => invoke<UpdateView | null>('update_check'),
  voiceSave: (voice: VoiceSettings) => invoke<VoiceSettings>('voice_save', { voice }),
  voiceSetKey: (which: 'fish' | 'openai', key: string) => invoke<string>('voice_set_key', { which, key }),
  voiceClearKey: (which: 'fish' | 'openai') => invoke('voice_clear_key', { which }),
  voiceTest: () => invoke('voice_test'),
  voiceClearCache: () => invoke<number>('voice_clear_cache'),
  updateSave: (autoCheck: boolean) => invoke<UpdateSettings>('update_save', { autoCheck }),
  saveNickname: (nickname: string) => invoke<string>('nickname_save', { nickname }),
  diaryList: () => invoke<Diary[]>('diary_list'),
  diaryDelete: (id: number) => invoke<boolean>('diary_delete', { id }),
  saveActivity: (activity: ActivitySettings) => invoke<ActivitySettings>('activity_save', { activity }),
  deleteActivity: (day: string) => invoke<number>('activity_delete', { day }),
  saveCare: (care: CareSettings) => invoke<CareSettings>('care_save', { care }),
  reminderList: () => invoke<ReminderView[]>('reminder_list'),
  reminderSave: (reminder: Reminder) => invoke<SavedReminder>('reminder_save', { reminder }),
  reminderDelete: (id: number) => invoke('reminder_delete', { id }),
  reminderSetStatus: (id: number, status: 'active' | 'paused') => invoke('reminder_set_status', { id, status }),
  usage: () => invoke<UsageView>('usage_get'),
  openDataDir: (sub?: string) => invoke('open_data_dir', { sub: sub ?? null }),
  characters: () => invoke<CharactersView>('characters_list'),
  reloadCharacters: () => invoke<CharactersView>('characters_reload'),
  selectCharacter: (id: string) => invoke('character_select', { id }),
  savePrefs: (global: CharacterPrefs, characterId: string, prefs: CharacterPrefs) =>
    invoke('prefs_save', { global, characterId, prefs }),
  openCharacterDir: (id: string) => invoke('open_character_dir', { id }),
  onOpenTab: (cb: (tab: string) => void) => listen<string>('settings-tab', (e) => cb(e.payload)),
  memoryOverview: () => invoke<MemoryOverview>('memory_overview'),
  memoryList: (filter: MemoryFilter) => invoke<{ items: Memory[]; total: number }>('memory_list', { filter }),
  memoryUpdate: (id: number, content: string, importance: number) =>
    invoke('memory_update', { id, content, importance }),
  memoryDelete: (id: number) => invoke('memory_delete', { id }),
  memoryAdd: (content: string, scope: string, importance: number) =>
    invoke<number>('memory_add', { content, scope, importance }),
  memorySetPaused: (paused: boolean) => invoke('memory_set_paused', { paused }),
  memoryClearAll: (confirm: string) => invoke('memory_clear_all', { confirm }),
  memoryExport: () => invoke<string>('memory_export'),
  memoryImportList: () => invoke<string[]>('memory_import_list'),
  memoryImport: (file: string) =>
    invoke<{ added: number; duplicates: number; rejected: number }>('memory_import', { file }),
};
export type SettingsApi = typeof settingsApi;

function taskChannel(onEvent: (e: TaskEvent) => void): Channel<TaskEvent> {
  const ch = new Channel<TaskEvent>();
  ch.onmessage = onEvent;
  return ch;
}

export interface TaskApi {
  defaults(): Promise<TaskDefaults>;
  start(goal: string, workdir: string, executor: Executor, allowWrite: boolean, onEvent: (e: TaskEvent) => void): Promise<string>;
  approve(taskId: string, approved: number[], onEvent: (e: TaskEvent) => void): Promise<void>;
  decline(taskId: string): Promise<void>;
  cancel(taskId: string): Promise<boolean>;
  report(taskId: string, requestId: string, onEvent: (e: ChatEvent) => void): Promise<void>;
}

export const taskApi: TaskApi = {
  defaults: () => invoke<TaskDefaults>('task_defaults'),
  start: (goal, workdir, executor, allowWrite, onEvent) =>
    invoke<string>('task_start', { goal, workdir, executor, allowWrite, onEvent: taskChannel(onEvent) }),
  approve: (taskId, approved, onEvent) => invoke('task_approve', { taskId, approved, onEvent: taskChannel(onEvent) }),
  decline: (taskId) => invoke('task_decline', { taskId }),
  cancel: (taskId) => invoke<boolean>('task_cancel', { taskId }),
  report: (taskId, requestId, onEvent) =>
    invoke('chat_task_report', { taskId, requestId, onEvent: channel(onEvent) }),
};

export const taskSettingsApi = {
  defaults: () => invoke<TaskDefaults>('task_defaults'),
  save: (settings: TaskSettings) => invoke<string>('task_settings_save', { settings }),
  list: () => invoke<TaskRecord[]>('task_list'),
  openLog: (taskId: string) => invoke('task_open_log', { taskId }),
};
