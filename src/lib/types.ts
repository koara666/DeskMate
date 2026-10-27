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
