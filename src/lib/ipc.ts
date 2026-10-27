// 前端与 Rust 的唯一通道。组件只依赖这里的接口，测试时可以替换成假实现。
import { Channel, invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type { ChatEvent, ChatInit, ChatMode, PetInit, PetState, Side } from './types';

export interface ChatApi {
  init(): Promise<ChatInit>;
  send(text: string, requestId: string, onEvent: (e: ChatEvent) => void): Promise<void>;
  regenerate(requestId: string, onEvent: (e: ChatEvent) => void): Promise<void>;
  stop(requestId: string): Promise<boolean>;
  setMode(mode: ChatMode): Promise<void>;
  hide(): Promise<void>;
  openSettings(): Promise<void>;
  onPlacement(cb: (side: Side) => void): Promise<UnlistenFn>;
  onFocusInput(cb: () => void): Promise<UnlistenFn>;
  onCharacterChanged(cb: () => void): Promise<UnlistenFn>;
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
  hide: () => invoke('chat_hide'),
  openSettings: () => invoke('open_settings'),
  onPlacement: (cb) => listen<Side>('chat-placement', (e) => cb(e.payload)),
  onFocusInput: (cb) => listen('chat-focus-input', () => cb()),
  onCharacterChanged: (cb) => listen('character-changed', () => cb()),
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
