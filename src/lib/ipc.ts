// 前端与 Rust 的唯一通道。组件只依赖这里的接口，测试时可以替换成假实现。
import { Channel, invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type {
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
  /** 气泡里点了「打开」 */
  open(target: string): Promise<string>;
  /** 气泡里点了「读取」：读完由角色说说内容（流式） */
  read(path: string, requestId: string, onEvent: (e: ChatEvent) => void): Promise<void>;
  /** 主动发言的结果：用户理会了 / 无人理会自动收起 */
  feedback(engaged: boolean): Promise<void>;
  /** 确认（或手动填写）提醒 */
  saveReminder(reminder: Reminder): Promise<SavedReminder>;
}

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
  open: (target) => invoke<string>('ability_open', { target }),
  read: (path, requestId, onEvent) => invoke('ability_read', { path, requestId, onEvent: channel(onEvent) }),
  feedback: (engaged) => invoke('proactive_feedback', { engaged }),
  saveReminder: (reminder) => invoke<SavedReminder>('reminder_save', { reminder }),
};

export interface PetApi {
  init(): Promise<PetInit>;
  click(): Promise<void>;
  menu(): Promise<void>;
  startDrag(): Promise<void>;
  onState(cb: (s: PetState) => void): Promise<UnlistenFn>;
  onChanged(cb: () => void): Promise<UnlistenFn[]>;
}

export const petApi: PetApi = {
  init: () => invoke<PetInit>('pet_init'),
  click: () => invoke('pet_click'),
  menu: () => invoke('pet_menu'),
  startDrag: () => getCurrentWindow().startDragging(),
  onState: (cb) => listen<PetState>('pet-state', (e) => cb(e.payload)),
  onChanged: (cb) =>
    Promise.all([listen('settings-changed', () => cb()), listen('character-changed', () => cb())]),
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
