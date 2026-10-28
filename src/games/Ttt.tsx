// 井字棋：你是 ×、她是 ○，你先手。难度决定她多容易失误（困难下她不会输）。
import { useEffect, useState } from 'preact/hooks';
import type { GameProps } from './registry';
import { ResultBanner } from './Hud';
import { aiMove, winner, winningLine, type Board, type Difficulty } from './logic/ttt';

const DIFFS: { id: Difficulty; name: string }[] = [
  { id: 'easy', name: '简单' },
  { id: 'normal', name: '普通' },
  { id: 'hard', name: '困难' },
];

export function Ttt({ say, finish, random = Math.random }: GameProps & { random?: () => number }) {
  const [board, setBoard] = useState<Board>(Array(9).fill(null));
  const [diff, setDiff] = useState<Difficulty>('normal');
  const [over, setOver] = useState<'win' | 'lose' | 'draw' | null>(null);

  useEffect(() => say('start'), []);

  const end = (w: 'X' | 'O' | 'draw') => {
    const o = w === 'X' ? 'win' : w === 'O' ? 'lose' : 'draw';
    setOver(o);
    say(o);
    finish(o);
  };

  const click = (i: number) => {
    if (over || board[i]) return;
    const b = board.slice();
    b[i] = 'X';
    let w = winner(b);
    if (!w) {
      b[aiMove(b, 'O', diff, random)] = 'O';
      w = winner(b);
    }
    setBoard(b);
    if (w) end(w);
  };
  const reset = () => {
    setBoard(Array(9).fill(null));
    setOver(null);
    say('start');
  };
  const line = winningLine(board) ?? [];

  return (
    <div class="game ttt">
      <div class="seg ttt-diff" role="radiogroup" aria-label="难度">
        {DIFFS.map((d) => (
          <button
            type="button"
            key={d.id}
            role="radio"
            aria-checked={diff === d.id}
            class={diff === d.id ? 'on' : ''}
            disabled={board.some((x) => x) && !over}
            onClick={() => setDiff(d.id)}
          >
            {d.name}
          </button>
        ))}
      </div>
      <div class="ttt-board" role="grid">
        {board.map((c, i) => (
          <button
            type="button"
            key={i}
            class={`ttt-cell ${c === 'X' ? 'x' : c === 'O' ? 'o' : ''} ${line.includes(i) ? 'hit' : ''}`}
            aria-label={`第 ${i + 1} 格${c ? (c === 'X' ? '，你' : '，她') : ''}`}
            disabled={!!c || !!over}
            onClick={() => click(i)}
          >
            {c && <span class="piece">{c === 'X' ? '×' : '○'}</span>}
          </button>
        ))}
      </div>
      {over && <ResultBanner result={over} />}
      {over && (
        <button type="button" class="btn btn-small" onClick={reset}>
          再来一局
        </button>
      )}
    </div>
  );
}
