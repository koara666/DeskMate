// 21 点：她当庄家。你要牌 / 停牌；她小于 17 点就要牌。
import { useEffect, useState } from 'preact/hooks';
import type { GameProps } from './registry';
import { ResultBanner } from './Hud';
import { dealerPlay, isBlackjack, newDeck, score, settle, type Card } from './logic/blackjack';

function CardView({ c, hidden }: { c: Card; hidden?: boolean }) {
  if (hidden) return <span class="bj-card back">?</span>;
  const red = c.suit === '♥' || c.suit === '♦';
  return (
    <span class={`bj-card ${red ? 'red' : ''}`}>
      {c.suit}
      {c.rank}
    </span>
  );
}

export function Blackjack({ say, finish, random = Math.random }: GameProps & { random?: () => number }) {
  const [deck, setDeck] = useState<Card[]>([]);
  const [me, setMe] = useState<Card[]>([]);
  const [her, setHer] = useState<Card[]>([]);
  const [over, setOver] = useState<'win' | 'lose' | 'draw' | null>(null);

  const done = (o: 'win' | 'lose' | 'draw', mine: Card[], hers: Card[]) => {
    setOver(o);
    const vars: [string, string][] = [
      ['点数', String(score(mine))],
      ['对手点数', String(score(hers))],
    ];
    if (o === 'lose' && score(mine) > 21) say('round_lose', vars);
    else if (o === 'win' && score(hers) > 21) say('round_win', vars);
    else say(o, vars);
    finish(o);
  };

  const deal = () => {
    const d = newDeck(random);
    const m = [d.pop()!, d.pop()!];
    const h = [d.pop()!, d.pop()!];
    setDeck(d);
    setMe(m);
    setHer(h);
    setOver(null);
    if (isBlackjack(m) || isBlackjack(h)) {
      if (isBlackjack(m)) say('special');
      const o = settle(m, h);
      setOver(o);
      finish(o);
    } else say('start');
  };
  useEffect(deal, []);

  const hit = () => {
    if (over) return;
    const d = deck.slice();
    const m = [...me, d.pop()!];
    setDeck(d);
    setMe(m);
    if (score(m) > 21) done('lose', m, her);
  };
  const stand = () => {
    if (over) return;
    const d = deck.slice();
    const h = dealerPlay(her, d);
    setDeck(d);
    setHer(h);
    done(settle(me, h), me, h);
  };

  return (
    <div class="game blackjack">
      <div class="bj-row">
        <span class="bj-who">她{over ? `（${score(her)}）` : ''}</span>
        {her.map((c, i) => (
          <CardView key={i} c={c} hidden={!over && i === 1} />
        ))}
      </div>
      <div class="bj-row">
        <span class="bj-who">你（{score(me)}）</span>
        {me.map((c, i) => (
          <CardView key={i} c={c} />
        ))}
      </div>
      {over ? (
        <div class="bj-actions">
          <ResultBanner result={over} />
          <button type="button" class="btn btn-small" onClick={deal}>
            再来一局
          </button>
        </div>
      ) : (
        <div class="bj-actions">
          <button type="button" class="btn btn-small" onClick={hit}>
            要牌
          </button>
          <button type="button" class="btn btn-small btn-plain" onClick={stand}>
            停牌
          </button>
        </div>
      )}
    </div>
  );
}
