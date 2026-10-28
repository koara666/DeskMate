// 小游戏共用的界面小件：比分牌、结果横幅（出现时一次性的弹出动画，没有常驻动画）。
export function Scoreboard({ me, her, name, label }: { me: number; her: number; name: string; label?: string }) {
  return (
    <div class="hud" aria-label={`比分：你 ${me}，${name} ${her}`}>
      <span class="hud-side me">
        <small>YOU</small>
        <b>{me}</b>
      </span>
      <span class="hud-mid">{label ?? 'VS'}</span>
      <span class="hud-side her">
        <b>{her}</b>
        <small>{name}</small>
      </span>
    </div>
  );
}

const RESULT: Record<'win' | 'lose' | 'draw', [string, string]> = {
  win: ['YOU WIN!', '你赢了'],
  lose: ['GAME OVER', '她赢了'],
  draw: ['DRAW', '平局'],
};

export function ResultBanner({ result }: { result: 'win' | 'lose' | 'draw' }) {
  const [big, small] = RESULT[result];
  return (
    <div class={`game-result ${result}`} role="status">
      <b>{big}</b>
      <small>{small}</small>
    </div>
  );
}
