// 猜拳：三局两胜。她出什么完全随机（不偷看）。
import { useEffect, useState } from 'preact/hooks';
import type { GameProps } from './registry';
import { ResultBanner, Scoreboard } from './Hud';

type Hand = 'rock' | 'scissors' | 'paper';
const HANDS: { id: Hand; icon: string; name: string }[] = [
  { id: 'rock', icon: '✊', name: '石头' },
  { id: 'scissors', icon: '✌️', name: '剪刀' },
  { id: 'paper', icon: '✋', name: '布' },
];
const BEATS: Record<Hand, Hand> = { rock: 'scissors', scissors: 'paper', paper: 'rock' };

export function judge(me: Hand, her: Hand): 'win' | 'lose' | 'draw' {
  return me === her ? 'draw' : BEATS[me] === her ? 'win' : 'lose';
}

export function Rps({ say, finish, name = '她', random = Math.random }: GameProps & { random?: () => number }) {
  const [score, setScore] = useState({ me: 0, her: 0 });
  const [last, setLast] = useState<{ me: Hand; her: Hand } | null>(null);
  const [over, setOver] = useState(false);
  const [round, setRound] = useState(0);
  const [result, setResult] = useState<'win' | 'lose' | 'draw' | null>(null);

  useEffect(() => say('start'), []);

  const play = (me: Hand) => {
    if (over) return;
    const her = HANDS[Math.floor(random() * 3)].id;
    const r = judge(me, her);
    const next = { me: score.me + (r === 'win' ? 1 : 0), her: score.her + (r === 'lose' ? 1 : 0) };
    setLast({ me, her });
    setScore(next);
    setRound((n) => n + 1);
    setResult(r);
    const vars: [string, string][] = [['分数', `${next.me}:${next.her}`]];
    if (next.me === 2 || next.her === 2) {
      const o = next.me === 2 ? 'win' : 'lose';
      setOver(true);
      say(o, vars);
      finish(o);
    } else {
      say(r === 'win' ? 'round_win' : r === 'lose' ? 'round_lose' : 'round_draw', vars);
    }
  };
  const icon = (h: Hand) => HANDS.find((x) => x.id === h)!.icon;

  return (
    <div class="game rps">
      <Scoreboard me={score.me} her={score.her} name={name} label="三局两胜" />
      <div class="rps-show" aria-live="polite">
        {last ? (
          <div class={`rps-reveal r-${result}`} key={round}>
            <span title="你">{icon(last.me)}</span>
            <span class="vs">vs</span>
            <span title="她">{icon(last.her)}</span>
          </div>
        ) : (
          <span class="muted">出拳吧</span>
        )}
      </div>
      {over && <ResultBanner result={score.me > score.her ? 'win' : 'lose'} />}
      {over ? (
        <button
          type="button"
          class="btn btn-small"
          onClick={() => {
            setScore({ me: 0, her: 0 });
            setLast(null);
            setOver(false);
            setResult(null);
            say('start');
          }}
        >
          再来一局
        </button>
      ) : (
        <div class="rps-hands">
          {HANDS.map((h) => (
            <button type="button" key={h.id} class="rps-hand" aria-label={h.name} onClick={() => play(h.id)}>
              {h.icon}
              <small>{h.name}</small>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
