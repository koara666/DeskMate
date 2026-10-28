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
  /** 她的名字（比分牌上用） */
  name?: string;
}

export interface GameInfo {
  id: string;
  name: string;
  desc: string;
  /** 按需加载 */
  load: () => Promise<ComponentType<GameProps>>;
  /** 需要连上模型才能玩 */
  needsModel?: boolean;
}

export const GAMES: GameInfo[] = [
  { id: 'rps', name: '猜拳', desc: '三局两胜', load: () => import('./Rps').then((m) => m.Rps) },
  { id: 'ttt', name: '井字棋', desc: '三个连成一线就赢，可以选难度', load: () => import('./Ttt').then((m) => m.Ttt) },
  { id: 'blackjack', name: '21 点', desc: '她当庄家，不超过 21 点、比她大就赢', load: () => import('./Blackjack').then((m) => m.Blackjack) },
  { id: 'fortune', name: '今日运势', desc: '每天抽一支签', load: () => import('./Fortune').then((m) => m.FortuneGame) },
  { id: 'idiom', name: '成语接龙', desc: '接上一个成语的最后一个字', needsModel: true, load: () => import('./LlmGames').then((m) => m.Idiom) },
  { id: 'turtle', name: '海龟汤', desc: '问是非题，猜出故事背后的真相（题目原创）', needsModel: true, load: () => import('./LlmGames').then((m) => m.Turtle) },
  { id: 'twenty', name: '二十问', desc: '她想一个东西，你用 20 个是非题猜出来', needsModel: true, load: () => import('./LlmGames').then((m) => m.Twenty) },
];
