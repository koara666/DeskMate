// 设置页测试共用的假数据
import type { SettingsView } from '../lib/types';

export function testView(): SettingsView {
  return structuredClone(view);
}

const view: SettingsView = {
  general: { always_on_top: true, pet_scale: 1, idle_animation: true, dnd: false, gpu_acceleration: false },
  backends: [
    {
      config: {
        id: 'deepseek',
        kind: 'deepseek',
        name: 'DeepSeek',
        base_url: 'https://api.deepseek.com',
        model: 'deepseek-flash',
        reasoning: 'disabled',
        price: { input: 0.3, cached_input: 0.006, cache_write: 0.3, output: 1.2 },
      },
      kind_name: 'DeepSeek',
      needs_key: true,
      key_url: 'https://platform.deepseek.com/api_keys',
      note: '',
      key_mask: null,
      removable: false,
    },
  ],
  kinds: [{ kind: 'deepseek', display_name: 'DeepSeek' }],
  chat_backend: 'deepseek',
  background_backend: '',
  budget: { daily_usd: 1, monthly_usd: 10 },
  data_dir: 'C:\\data',
  version: '0.1.0',
  initial_tab: null,
  companion: {
    enabled: true,
    cooldown_minutes: 20,
    frequency: 'sometimes',
    quiet_enabled: false,
    quiet_start: '23:30',
    quiet_end: '08:00',
    daily_calls: 10,
    daily_usd: 0.05,
    events: true,
    system_notify: true,
  },
  advanced: { allow_delegation: false, read_dirs: [], programs: [] },
  activity: { enabled: true, comments: true, record: true, rules: [], private_apps: [] },
  care: { sit: true, sit_minutes: 60, water: true, water_minutes: 150, late: true, late_time: '00:30', meal: true },
  briefing: {
    on_startup: true,
    times: [],
    weather: true,
    holiday: true,
    meal: true,
    news: false,
    reminders: true,
    special: true,
    weather_in_chat: true,
    city: null,
    region: 'Auto',
    feeds: [{ name: 'IT之家', url: 'https://www.ithome.com/rss/', enabled: false }],
    special_dates: [],
  },
  daily: {
    autostart: false,
    hotkey_enabled: true,
    hotkey: 'Alt+Shift+D',
    hide_in_fullscreen: true,
    sleep_minutes: 10,
    proactive: false,
    proactive_minutes: 90,
    proactive_daily_max: 3,
  },
};
