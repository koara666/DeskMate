// 21 点：一副牌，A 算 1 或 11，JQK 算 10；庄家（角色）小于 17 点就要牌。
export interface Card {
  rank: string;
  suit: string;
}

const RANKS = ['A', '2', '3', '4', '5', '6', '7', '8', '9', '10', 'J', 'Q', 'K'];
const SUITS = ['♠', '♥', '♦', '♣'];

export function newDeck(random = Math.random): Card[] {
  const d: Card[] = [];
  for (const s of SUITS) for (const r of RANKS) d.push({ rank: r, suit: s });
  for (let i = d.length - 1; i > 0; i--) {
    const j = Math.floor(random() * (i + 1));
    [d[i], d[j]] = [d[j], d[i]];
  }
  return d;
}

export function score(hand: Card[]): number {
  let total = 0;
  let aces = 0;
  for (const c of hand) {
    if (c.rank === 'A') {
      aces++;
      total += 11;
    } else if (['J', 'Q', 'K'].includes(c.rank)) total += 10;
    else total += Number(c.rank);
  }
  while (total > 21 && aces > 0) {
    total -= 10;
    aces--;
  }
  return total;
}

export const isBlackjack = (hand: Card[]) => hand.length === 2 && score(hand) === 21;

/** 庄家要牌直到 17 点以上 */
export function dealerPlay(dealer: Card[], deck: Card[]): Card[] {
  const h = dealer.slice();
  while (score(h) < 17 && deck.length) h.push(deck.pop()!);
  return h;
}

/** 用户的结果 */
export function settle(player: Card[], dealer: Card[]): 'win' | 'lose' | 'draw' {
  const p = score(player);
  const d = score(dealer);
  if (p > 21) return 'lose';
  if (d > 21) return 'win';
  if (isBlackjack(player) && !isBlackjack(dealer)) return 'win';
  if (isBlackjack(dealer) && !isBlackjack(player)) return 'lose';
  return p > d ? 'win' : p < d ? 'lose' : 'draw';
}
