// 小游戏注册表：新增一个游戏 = 在 src/games/ 下写一个组件 + 在这里加一行（+ 可选的台词场景 game.<id>.<事件>）。
// 核心代码（聊天窗口、后端）不用改。游戏规则和胜负判定都在游戏组件里本地实现。
import type { ComponentType } from 'preact';
import type { GameApi } from '../lib/ipc';

export interface GameProps {
  /** 关键时刻让角色说一句（结果显示在游戏上方的气泡里） */
  say: (event: Parameters<GameApi['say']>[1], vars?: [string, string][]) => void;
  /** 一局结束（从用户的角度） */
  finish: (outcome: Parameters<GameApi['finish']>[1]) => void;
  api: GameApi;
}

export interface GameInfo {
  id: string;
  name: string;
  desc: string;
  /** 按需加载 */
  load: () => Promise<ComponentType<GameProps>>;
}

export const GAMES: GameInfo[] = [
  { id: 'rps', name: '猜拳', desc: '三局两胜', load: () => import('./Rps').then((m) => m.Rps) },
  { id: 'ttt', name: '井字棋', desc: '三个连成一线就赢，可以选难度', load: () => import('./Ttt').then((m) => m.Ttt) },
  { id: 'blackjack', name: '21 点', desc: '她当庄家，不超过 21 点、比她大就赢', load: () => import('./Blackjack').then((m) => m.Blackjack) },
  { id: 'fortune', name: '今日运势', desc: '每天抽一支签', load: () => import('./Fortune').then((m) => m.FortuneGame) },
];
