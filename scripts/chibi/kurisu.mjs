// 牧濑红莉栖的 Q 版形象（原创绘制）：按她的特征用代码画出一整套 SVG——
// 红褐色的及胸长发、蓝灰色的眼睛、白衬衫配红领带、浅卡其色外套。
// 每个表情 / 动作一张，输出到 characters/makise-kurisu/assets/<名字>.svg（160×180，和桌宠窗口一样的比例）。
// 用法：node scripts/chibi/kurisu.mjs [输出目录]
// 新增动作：在 POSES 里加一项（眼睛、嘴、腮红、手、装饰的组合），重新运行即可。
import { mkdirSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

const OUT = resolve(process.argv[2] ?? 'characters/makise-kurisu/assets');

const C = {
  line: '#3a2326',
  hair: '#8a3428',
  hairShade: '#5e2119',
  hairLight: '#b95c46',
  skin: '#fff1ea',
  skinShade: '#f5d6ca',
  blush: '#f2a3a8',
  iris: '#7d8db8',
  irisDark: '#4a587f',
  pupil: '#1c2238',
  hood: '#d9c3a2',
  hoodShade: '#bda684',
  hoodLight: '#6a7aa6',
  shirt: '#fbfbfd',
  ribbon: '#c23b36',
  strap: '#e7d9c3',
  skirt: '#4b3a3f',
  skirtShade: '#352a2e',
  sock: '#2b2830',
  shoe: '#6b4a3a',
  console: '#2a2d36',
  screen: '#8fb4e0',
  pink: '#e8636a',
  white: '#ffffff',
  star: '#ffd95a',
};
const SW = 1.8; // 描边粗细

const g = (inner, attrs = '') => `<g ${attrs}>${inner}</g>`;
const path = (d, fill, extra = '') =>
  `<path d="${d}" fill="${fill}" stroke="${C.line}" stroke-width="${SW}" stroke-linejoin="round" stroke-linecap="round" ${extra}/>`;
const fillOnly = (d, fill, extra = '') => `<path d="${d}" fill="${fill}" ${extra}/>`;
const stroke = (d, color = C.line, w = SW, extra = '') =>
  `<path d="${d}" fill="none" stroke="${color}" stroke-width="${w}" stroke-linecap="round" stroke-linejoin="round" ${extra}/>`;

// ———————— 部件 ————————

/** 后面的头发：及胸的长直发 */
function backHair() {
  return path(
    'M26,74 C20,36 46,12 80,12 C114,12 140,36 134,74 C133,100 138,128 146,150 C136,157 122,153 116,141 L44,141 C38,153 24,157 14,150 C22,128 27,100 26,74 Z',
    C.hair,
  );
}

/** Kurisu 没有帽子：保留函数名，什么也不画 */
function hoodDown() {
  return '';
}
function hoodUp() {
  return backHair();
}

/** 脸 */
function face() {
  return path('M43,62 C43,40 58,31 80,31 C102,31 117,40 117,62 C117,83 103,98 80,101 C57,98 43,83 43,62 Z', C.skin);
}

/** 前面的刘海和两边垂下来的长鬓发 */
function frontHair(hood) {
  const bangs = path(
    'M39,60 C36,34 56,19 80,19 C104,19 124,34 121,60 L114,52 L107,59 L99,50 L91,58 L83,47 L76,57 L68,49 L60,58 L52,50 L46,58 Z',
    C.hair,
  );
  const shine = stroke('M56,30 C64,24 76,23 86,24', C.hairLight, 2.5) + stroke('M92,26 C100,26 108,29 114,35', C.hairLight, 2);
  const part = stroke('M80,19 C79,30 77,40 76,48', C.hairShade, 1.4, 'opacity="0.7"');
  const left = path('M41,54 C31,78 34,110 24,142 C38,141 47,130 49,114 C51,94 46,72 51,56 Z', C.hair);
  const right = path('M119,54 C129,78 126,110 136,142 C122,141 113,130 111,114 C109,94 114,72 109,56 Z', C.hair);
  const shade = fillOnly('M46,58 L52,50 L60,58 L68,49 L76,57 L83,47 L91,58 L99,50 L107,59 L114,52 L121,60 C104,56 58,56 39,60 Z', C.hairShade, 'opacity="0.4"');
  return (hood ? '' : shine) + left + right + bangs + shade + part;
}

/** Kurisu 没有发卡 */
function hairClip() {
  return '';
}

// —— 眼睛 ——
const EYE_L = 63;
const EYE_R = 97;
const EYE_Y = 71;

function eyeOpen(cx, { look = 0, lookY = 0, big = false, lid = false } = {}) {
  const rx = big ? 9 : 8;
  const ry = big ? 11 : 10;
  const ix = cx + look;
  const iy = EYE_Y + lookY;
  return (
    `<ellipse cx="${cx}" cy="${EYE_Y}" rx="${rx}" ry="${ry}" fill="${C.white}"/>` +
    `<ellipse cx="${ix}" cy="${iy + 1}" rx="${rx - 1.5}" ry="${ry - 1}" fill="${C.irisDark}"/>` +
    `<ellipse cx="${ix}" cy="${iy + 2.5}" rx="${rx - 3}" ry="${ry - 3.5}" fill="${C.iris}"/>` +
    `<ellipse cx="${ix}" cy="${iy + 2}" rx="${big ? 2.6 : 3.2}" ry="${big ? 3 : 4}" fill="${C.pupil}"/>` +
    `<circle cx="${ix - 3}" cy="${iy - 3}" r="2.6" fill="${C.white}"/>` +
    `<circle cx="${ix + 3}" cy="${iy + 5}" r="1.2" fill="${C.white}"/>` +
    // 上眼线（她总是有点困：眼皮压得低一点）
    stroke(`M${cx - rx - 1},${EYE_Y - (lid ? 4 : 7)} C${cx - 4},${EYE_Y - (lid ? 10 : 13)} ${cx + 4},${EYE_Y - (lid ? 10 : 13)} ${cx + rx + 1},${EYE_Y - (lid ? 5 : 8)}`, C.line, 2.6) +
    (lid
      ? fillOnly(
          `M${cx - rx - 1},${EYE_Y - 4} C${cx - 4},${EYE_Y - 10} ${cx + 4},${EYE_Y - 10} ${cx + rx + 1},${EYE_Y - 5} L${cx + rx + 1},${EYE_Y - 12} L${cx - rx - 1},${EYE_Y - 12} Z`,
          C.skin,
        )
      : '') +
    stroke(`M${cx - 5},${EYE_Y + ry - 0.5} C${cx - 2},${EYE_Y + ry + 1} ${cx + 2},${EYE_Y + ry + 1} ${cx + 5},${EYE_Y + ry - 0.5}`, C.line, 1.2, 'opacity="0.5"')
  );
}
/** ^ ^ 笑眼 */
const eyeHappy = (cx) => stroke(`M${cx - 7},${EYE_Y + 3} C${cx - 3},${EYE_Y - 5} ${cx + 3},${EYE_Y - 5} ${cx + 7},${EYE_Y + 3}`, C.line, 2.6);
/** 闭眼（睡着）：往下弯 */
const eyeClosed = (cx) => stroke(`M${cx - 7},${EYE_Y} C${cx - 3},${EYE_Y + 5} ${cx + 3},${EYE_Y + 5} ${cx + 7},${EYE_Y}`, C.line, 2.4);
/** 半睁（犯困） */
function eyeSleepy(cx) {
  return (
    `<path d="M${cx - 8},${EYE_Y + 1} C${cx - 4},${EYE_Y + 9} ${cx + 4},${EYE_Y + 9} ${cx + 8},${EYE_Y + 1} Z" fill="${C.iris}"/>` +
    `<ellipse cx="${cx}" cy="${EYE_Y + 4}" rx="2.6" ry="2.4" fill="${C.pupil}"/>` +
    stroke(`M${cx - 9},${EYE_Y + 1} L${cx + 9},${EYE_Y + 1}`, C.line, 2.6)
  );
}
/** 蚊香眼（晕） */
function eyeSpiral(cx) {
  return stroke(
    `M${cx},${EYE_Y} m-1,0 a1,1 0 1,1 2,0 a3,3 0 1,1 -5,1 a5,5 0 1,1 9,-2 a7,7 0 1,1 -13,3`,
    C.line,
    1.8,
  );
}
/** > < 用力闭眼 */
const eyeSquint = (cx, dir) =>
  stroke(dir > 0 ? `M${cx - 6},${EYE_Y - 5} L${cx + 5},${EYE_Y} L${cx - 6},${EYE_Y + 5}` : `M${cx + 6},${EYE_Y - 5} L${cx - 5},${EYE_Y} L${cx + 6},${EYE_Y + 5}`, C.line, 2.6);

// —— 嘴 ——
const MY = 88;
const MOUTH = {
  smile: stroke(`M75,${MY} C78,${MY + 3} 82,${MY + 3} 85,${MY}`, C.line, 1.8),
  cat: stroke(`M74,${MY} C76,${MY + 3} 79,${MY + 3} 80,${MY} C81,${MY + 3} 84,${MY + 3} 86,${MY}`, C.line, 1.8),
  flat: stroke(`M76,${MY + 1} L84,${MY + 1}`, C.line, 1.8),
  open: path(`M75,${MY - 1} C75,${MY + 6} 85,${MY + 6} 85,${MY - 1} C82,${MY} 78,${MY} 75,${MY - 1} Z`, '#c45a72') + fillOnly(`M77,${MY + 3} C79,${MY + 5} 81,${MY + 5} 83,${MY + 3} C81,${MY + 2} 79,${MY + 2} 77,${MY + 3} Z`, '#f49aaa'),
  grin: path(`M72,${MY - 2} C72,${MY + 8} 88,${MY + 8} 88,${MY - 2} Z`, '#c45a72') + fillOnly(`M75,${MY + 3} C78,${MY + 6} 82,${MY + 6} 85,${MY + 3} C82,${MY + 2} 78,${MY + 2} 75,${MY + 3} Z`, '#f49aaa'),
  o: `<ellipse cx="80" cy="${MY + 2}" rx="3.2" ry="4" fill="#c45a72" stroke="${C.line}" stroke-width="${SW}"/>`,
  frown: stroke(`M75,${MY + 3} C78,${MY} 82,${MY} 85,${MY + 3}`, C.line, 1.8),
  wavy: stroke(`M73,${MY + 1} C75,${MY - 1} 77,${MY - 1} 78,${MY + 1} C79,${MY + 3} 81,${MY + 3} 82,${MY + 1} C83,${MY - 1} 85,${MY - 1} 87,${MY + 1}`, C.line, 1.8),
  pout: stroke(`M77,${MY + 1} C79,${MY - 1} 81,${MY - 1} 83,${MY + 1}`, C.line, 1.8) + `<ellipse cx="80" cy="${MY + 1}" rx="1.4" ry="1" fill="${C.line}"/>`,
  yawn: `<ellipse cx="80" cy="${MY + 2}" rx="4.5" ry="5.5" fill="#c45a72" stroke="${C.line}" stroke-width="${SW}"/>`,
};

const blush = (strong = false) =>
  `<ellipse cx="55" cy="84" rx="${strong ? 8 : 6.5}" ry="${strong ? 4 : 3}" fill="${C.blush}" opacity="${strong ? 0.75 : 0.45}"/>` +
  `<ellipse cx="105" cy="84" rx="${strong ? 8 : 6.5}" ry="${strong ? 4 : 3}" fill="${C.blush}" opacity="${strong ? 0.75 : 0.45}"/>` +
  (strong ? stroke('M51,82 L53,86 M56,82 L58,86 M101,82 L103,86 M106,82 L108,86', C.irisDark, 1, 'opacity="0.5"') : '');

const browsWorried = stroke('M55,57 C59,54 64,53 68,55', C.line, 1.8) + stroke('M105,57 C101,54 96,53 92,55', C.line, 1.8);
const browsAngry = stroke('M55,53 L68,58', C.line, 2) + stroke('M105,53 L92,58', C.line, 2);

// —— 身体 ——
function body({ arms = 'down' } = {}) {
  let s = '';
  // 腿、鞋
  s += path('M61,160 L60,173 C60,176 70,176 70,173 L70,160 Z', C.sock);
  s += path('M90,160 L90,173 C90,176 100,176 100,173 L99,160 Z', C.sock);
  s += `<ellipse cx="65" cy="175.5" rx="7" ry="3.5" fill="${C.shoe}" stroke="${C.line}" stroke-width="${SW}"/>`;
  s += `<ellipse cx="95" cy="175.5" rx="7" ry="3.5" fill="${C.shoe}" stroke="${C.line}" stroke-width="${SW}"/>`;
  // 裙子
  s += path('M49,152 L111,152 L116,165 C100,168 60,168 44,165 Z', C.skirt);
  s += stroke('M62,154 L60,166 M80,154 L80,167 M98,154 L100,166', C.skirtShade, 1.4);
  s += `<rect x="74" y="96" width="12" height="10" fill="${C.skinShade}"/>`;
  // 外套（浅卡其）
  s += path('M51,104 C43,109 39,120 40,134 L40,152 C40,158 46,160 52,160 L108,160 C114,160 120,158 120,152 L120,134 C121,120 117,109 109,104 Z', C.hood);
  s += fillOnly('M108,108 C115,116 117,128 117,140 L117,154 C114,157 110,158 106,158 Z', C.hoodShade, 'opacity="0.6"');
  // 白衬衫、领口、红领带
  s += path('M66,103 L80,128 L94,103 Z', C.shirt);
  s += path('M66,103 L74,110 L68,116 Z', C.shirt) + path('M94,103 L86,110 L92,116 Z', C.shirt);
  s += path('M77,108 L83,108 L84,114 L76,114 Z', C.ribbon);
  s += path('M76,114 L84,114 L86,138 L80,142 L74,138 Z', C.ribbon);
  if (arms === 'phone') {
    s += path('M50,108 C41,118 44,136 59,139 L68,135 C61,131 58,123 58,114 Z', C.hood);
    s += path('M110,108 C119,118 116,136 101,139 L92,135 C99,131 102,123 102,114 Z', C.hood);
    s += phoneBox(80, 137);
    s += `<circle cx="63" cy="139" r="4" fill="${C.skin}" stroke="${C.line}" stroke-width="1.5"/>`;
    s += `<circle cx="97" cy="139" r="4" fill="${C.skin}" stroke="${C.line}" stroke-width="1.5"/>`;
  } else if (arms === 'cheer') {
    s += path('M50,108 C41,118 44,136 59,139 L66,134 C60,130 58,122 58,114 Z', C.hood);
    s += path('M109,106 C118,98 124,86 126,74 L117,72 C114,84 108,94 102,100 Z', C.hood);
    s += g(phoneBox(122, 64), 'transform="rotate(-18 122 64)"');
    s += `<circle cx="120" cy="72" r="4" fill="${C.skin}" stroke="${C.line}" stroke-width="1.5"/>`;
    s += `<circle cx="62" cy="138" r="4" fill="${C.skin}" stroke="${C.line}" stroke-width="1.5"/>`;
  } else if (arms === 'cover') {
    s += path('M50,108 C40,116 40,96 50,88 L58,92 C54,98 56,104 58,110 Z', C.hood);
    s += path('M110,108 C120,116 120,96 110,88 L102,92 C106,98 104,104 102,110 Z', C.hood);
  } else {
    s += path('M50,108 C43,120 43,138 48,146 L56,144 C54,134 55,122 58,114 Z', C.hood);
    s += path('M110,108 C117,120 117,138 112,146 L104,144 C106,134 105,122 102,114 Z', C.hood);
  }
  return s;
}

/** 手机（深色，屏幕发着蓝光） */
function phoneBox(cx, cy) {
  const x = cx - 17;
  const y = cy - 10;
  let s = `<rect x="${x}" y="${y}" width="34" height="20" rx="4" fill="${C.console}" stroke="${C.line}" stroke-width="${SW}"/>`;
  s += `<rect x="${x + 3}" y="${y + 3}" width="28" height="14" rx="2" fill="${C.screen}"/>`;
  s += `<circle cx="${x + 30}" cy="${y + 10}" r="1" fill="${C.white}" opacity="0.6"/>`;
  return s;
}

// —— 装饰 ——
const heart = (x, y, s = 1, fill = C.pink) =>
  `<path transform="translate(${x} ${y}) scale(${s})" d="M0,3 C0,-1 -6,-2 -6,2.5 C-6,6 0,9 0,10 C0,9 6,6 6,2.5 C6,-2 0,-1 0,3 Z" fill="${fill}" stroke="${C.line}" stroke-width="1.2"/>`;
const sparkle = (x, y, s = 1, fill = C.star) =>
  `<path transform="translate(${x} ${y}) scale(${s})" d="M0,-7 C1,-2 2,-1 7,0 C2,1 1,2 0,7 C-1,2 -2,1 -7,0 C-2,-1 -1,-2 0,-7 Z" fill="${fill}" stroke="${C.line}" stroke-width="1"/>`;
const sweat = (x, y) => `<path d="M${x},${y} C${x + 4},${y + 6} ${x + 3},${y + 10} ${x},${y + 10} C${x - 3},${y + 10} ${x - 4},${y + 6} ${x},${y} Z" fill="#9fd8f5" stroke="${C.line}" stroke-width="1.2"/>`;
const tear = (x, y) => `<path d="M${x},${y} C${x + 3},${y + 5} ${x + 2},${y + 8} ${x},${y + 8} C${x - 2},${y + 8} ${x - 3},${y + 5} ${x},${y} Z" fill="#9fd8f5"/>`;
const text = (x, y, t, size, fill = C.line, extra = '') =>
  `<text x="${x}" y="${y}" font-family="Consolas, 'Courier New', monospace" font-weight="700" font-size="${size}" fill="${fill}" ${extra}>${t}</text>`;
const zzz = text(118, 30, 'Z', 14, C.hoodLight) + text(130, 18, 'z', 10, C.hoodLight);
const bubble = `<circle cx="91" cy="92" r="5" fill="#d8f1ff" stroke="#8cc7e6" stroke-width="1.2" opacity="0.9"/>`;
const dots = `<circle cx="124" cy="22" r="2.4" fill="${C.line}"/><circle cx="132" cy="18" r="2.4" fill="${C.line}"/><circle cx="140" cy="14" r="2.4" fill="${C.line}"/>`;
const exclaim = text(124, 30, '!', 22, C.pink, `stroke="${C.line}" stroke-width="0.8"`);
const question = text(124, 30, '?', 20, C.hoodLight, `stroke="${C.line}" stroke-width="0.6"`);
const anger = stroke('M118,40 L124,46 M124,40 L118,46', C.pink, 2.4) + stroke('M114,43 L128,43 M121,36 L121,50', C.pink, 1.2, 'opacity="0.0"');
const swirl = sparkle(30, 30, 0.7) + sparkle(130, 26, 0.6) + sparkle(138, 54, 0.5);
const notes = text(122, 34, '♪', 16, C.pink) + text(134, 22, '♫', 12, C.hoodLight);

// ———————— 一个姿势 = 各部件的组合 ————————
/**
 * eyes：open / happy / closed / sleepy / spiral / squint / wide / side / up / wink / tearful
 * mouth：MOUTH 的键；blush：0 / 1 / 2；brows：'' / worried / angry；arms：console / cheer / cover / down
 * hood：戴兜帽；tilt：头歪的角度；extra：装饰
 */
function draw({ eyes = 'open', mouth = 'flat', blush: b = 0, brows = '', arms = 'down', hood = false, tilt = 0, extra = '' }) {
  let e = '';
  switch (eyes) {
    case 'happy':
      e = eyeHappy(EYE_L) + eyeHappy(EYE_R);
      break;
    case 'closed':
      e = eyeClosed(EYE_L) + eyeClosed(EYE_R);
      break;
    case 'sleepy':
      e = eyeSleepy(EYE_L) + eyeSleepy(EYE_R);
      break;
    case 'spiral':
      e = eyeSpiral(EYE_L) + eyeSpiral(EYE_R);
      break;
    case 'squint':
      e = eyeSquint(EYE_L, 1) + eyeSquint(EYE_R, -1);
      break;
    case 'wide':
      e = eyeOpen(EYE_L, { big: true, lid: false }) + eyeOpen(EYE_R, { big: true, lid: false });
      break;
    case 'side':
      e = eyeOpen(EYE_L, { look: -3 }) + eyeOpen(EYE_R, { look: -3 });
      break;
    case 'up':
      e = eyeOpen(EYE_L, { look: 2, lookY: -2 }) + eyeOpen(EYE_R, { look: 2, lookY: -2 });
      break;
    case 'wink':
      e = eyeOpen(EYE_L) + eyeHappy(EYE_R);
      break;
    case 'tearful':
      e = eyeOpen(EYE_L, { lookY: 1 }) + eyeOpen(EYE_R, { lookY: 1 }) + tear(56, 80) + tear(104, 80);
      break;
    default:
      e = eyeOpen(EYE_L) + eyeOpen(EYE_R);
  }
  const head =
    face() +
    (b ? blush(b === 2) : '') +
    e +
    (brows === 'worried' ? browsWorried : brows === 'angry' ? browsAngry : '') +
    MOUTH[mouth] +
    frontHair(hood) +
    '';
  const headG = tilt ? g(head, `transform="rotate(${tilt} 80 100)"`) : head;
  // 头画在身体后面一部分（后面的头发在身体后），所以：后发 → 身体 → 脸和刘海
  const back = hood ? hoodUp() : backHair();
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 160 180" width="160" height="180">${
    tilt ? g(back, `transform="rotate(${tilt} 80 100)"`) : back
  }${body({ arms })}${headG}${extra}</svg>`;
}

/** 全部表情 / 动作：名字 → 组合 */
export const POSES = {
  // 状态
  idle: { eyes: 'open', mouth: 'flat' },
  talking: { eyes: 'open', mouth: 'open', extra: stroke('M118,36 L124,30 M122,44 L130,42 M118,52 L124,56', C.pink, 2.2) },
  thinking: { eyes: 'up', mouth: 'flat', brows: 'worried', extra: question },
  happy: { eyes: 'happy', mouth: 'grin', blush: 1, extra: sparkle(132, 30, 0.8) + sparkle(28, 40, 0.6) },
  sad: { eyes: 'sleepy', mouth: 'frown', brows: 'worried', extra: stroke('M118,28 L118,50 M124,26 L124,50 M130,28 L130,50', C.hoodLight, 1.8) },
  sleep: { eyes: 'closed', mouth: 'o', tilt: -7, extra: zzz },
  // 待机变体（心情、夜里、天气、季节）
  idle_good: { eyes: 'happy', mouth: 'cat', blush: 2, extra: heart(30, 36, 1) },
  idle_low: { eyes: 'sleepy', mouth: 'flat' },
  idle_night: { eyes: 'sleepy', mouth: 'o', extra: `<circle cx="96" cy="94" r="3" fill="#ffffff" stroke="${C.line}" stroke-width="1"/>` },
  idle_rain: { eyes: 'open', mouth: 'flat', extra: dots },
  idle_winter: { eyes: 'sleepy', mouth: 'smile', arms: 'phone' },
  // 反应
  pat: { eyes: 'happy', mouth: 'cat', blush: 2, extra: heart(126, 26, 1) + heart(36, 30, 0.7) },
  dizzy: { eyes: 'spiral', mouth: 'wavy', tilt: 6, blush: 1, extra: swirl + sweat(124, 44) },
  pout: { eyes: 'squint', mouth: 'pout', blush: 1, brows: 'angry', extra: anger },
  surprised: { eyes: 'wide', mouth: 'o', extra: exclaim },
  confused: { eyes: 'open', mouth: 'wavy', brows: 'worried', tilt: 5, extra: question },
  hood: { eyes: 'side', mouth: 'flat', arms: 'cover' },
  win: { eyes: 'happy', mouth: 'grin', blush: 1, arms: 'cheer', extra: sparkle(30, 34, 0.9) + sparkle(36, 60, 0.6) + sparkle(140, 104, 0.6) },
  lose: { eyes: 'closed', mouth: 'wavy', brows: 'worried', arms: 'down', extra: sweat(124, 40) },
  wink: { eyes: 'wink', mouth: 'cat', blush: 1, extra: heart(130, 62, 0.8) },
  shy: { eyes: 'side', mouth: 'smile', blush: 2, arms: 'cover' },
  wake: { eyes: 'sleepy', mouth: 'pout', brows: 'angry', tilt: -4, extra: sweat(124, 44) },
  angry: { eyes: 'open', mouth: 'open', blush: 1, brows: 'angry', extra: anger },
  scared: { eyes: 'wide', mouth: 'o', brows: 'worried', extra: sweat(124, 40) + sweat(34, 50) },
};

mkdirSync(OUT, { recursive: true });
for (const [name, spec] of Object.entries(POSES)) {
  writeFileSync(join(OUT, `${name}.svg`), draw(spec) + '\n');
}
console.log(`画好了 ${Object.keys(POSES).length} 张 → ${OUT}`);
