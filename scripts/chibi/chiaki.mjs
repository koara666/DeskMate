// 七海千秋的 Q 版形象（原创绘制，DECISIONS #55）：按她的特征用代码画出一整套 SVG——
// 粉米色波波头（发梢往外翘）、粉色眼睛、像素飞船发卡、深青色猫耳兜帽外套、粉色书包带、手里抱着游戏机。
// 每个表情 / 动作一张，输出到 characters/nanami-chiaki/assets/<名字>.svg（160×180，和桌宠窗口一样的比例）。
// 用法：node scripts/chibi/chiaki.mjs [输出目录]
// 新增动作：在 POSES 里加一项（眼睛、嘴、腮红、手、兜帽、装饰的组合），重新运行即可。
import { mkdirSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

const OUT = resolve(process.argv[2] ?? 'characters/nanami-chiaki/assets');

const C = {
  line: '#4a3640',
  hair: '#ecd5cb',
  hairShade: '#d6b6aa',
  hairLight: '#faece6',
  skin: '#fff0e7',
  skinShade: '#f7d7ca',
  blush: '#f6a0b4',
  iris: '#ee7f9f',
  irisDark: '#c4507a',
  pupil: '#6e1f3e',
  hood: '#33585a',
  hoodShade: '#243f40',
  hoodLight: '#4a7375',
  shirt: '#f7f2ee',
  ribbon: '#f48fab',
  strap: '#f3c3ca',
  skirt: '#dccabb',
  skirtShade: '#c6b2a2',
  sock: '#2d4243',
  shoe: '#eab3bf',
  console: '#f8f5f7',
  screen: '#2c4048',
  pink: '#f48fab',
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

/** 后面的头发：波波头，发梢往外翘 */
function backHair() {
  return path(
    'M30,72 C25,38 48,13 80,13 C112,13 135,38 130,72 C129,86 133,98 142,107 C133,111 123,108 117,101 L43,101 C37,108 27,111 18,107 C27,98 31,86 30,72 Z',
    C.hair,
  );
}

/** 兜帽放下时：脖子后面的一圈帽子 */
function hoodDown() {
  return path('M50,103 C58,96 102,96 110,103 L114,110 C100,118 60,118 46,110 Z', C.hoodShade);
}

/** 兜帽戴上：猫耳兜帽盖住头顶 */
function hoodUp() {
  return (
    path('M44,38 L40,8 L66,26 Z', C.hood) +
    path('M116,38 L120,8 L94,26 Z', C.hood) +
    fillOnly('M45,32 L43,15 L58,26 Z', C.strap) +
    fillOnly('M115,32 L117,15 L102,26 Z', C.strap) +
    path(
      'M26,76 C22,40 46,16 80,16 C114,16 138,40 134,76 C134,90 130,102 124,108 L114,104 C118,88 116,60 104,46 C96,38 64,38 56,46 C44,60 42,88 46,104 L36,108 C30,102 26,90 26,76 Z',
      C.hood,
    ) +
    stroke('M56,46 C64,38 96,38 104,46', C.hoodLight, 2)
  );
}

/** 脸 */
function face() {
  return path('M43,62 C43,40 58,31 80,31 C102,31 117,40 117,62 C117,83 103,98 80,101 C57,98 43,83 43,62 Z', C.skin);
}

/** 前面的刘海和两边的鬓发 */
function frontHair(hood) {
  const bangs = path(
    'M40,59 C37,35 55,20 80,20 C105,20 123,35 120,59 L115,57 L111,53 L106,57 L99,54 L92,58 L86,54 L79,58 L72,54 L66,58 L59,54 L53,57 L47,54 Z',
    C.hair,
  );
  const shine = stroke('M58,30 C66,25 76,24 84,25', C.hairLight, 2.5);
  const left = path('M42,54 C35,70 38,90 30,104 C40,103 46,96 48,86 C48,74 47,63 49,56 Z', C.hair);
  const right = path('M118,54 C125,70 122,90 130,104 C120,103 114,96 112,86 C112,74 113,63 111,56 Z', C.hair);
  const shade = fillOnly('M47,54 L53,57 L59,54 L66,58 L72,54 L79,58 L86,54 L92,58 L99,54 L106,57 L111,53 L115,57 L114,60 C100,57 60,57 46,60 Z', C.hairShade, 'opacity="0.45"');
  return (hood ? '' : shine) + left + right + bangs + shade;
}

/** 像素飞船发卡（右边） */
function hairClip() {
  const px = [
    '..X..',
    '..X..',
    'X.X.X',
    'XXXXX',
    'X.W.X',
  ];
  const s = 2.4;
  let out = '';
  px.forEach((row, y) =>
    [...row].forEach((c, x) => {
      if (c === '.') return;
      out += `<rect x="${(103 + x * s).toFixed(1)}" y="${(30 + y * s).toFixed(1)}" width="${s}" height="${s}" fill="${c === 'W' ? C.white : C.pink}"/>`;
    }),
  );
  return g(out, 'transform="rotate(-12 109 36)"');
}

// —— 眼睛 ——
const EYE_L = 63;
const EYE_R = 97;
const EYE_Y = 71;

function eyeOpen(cx, { look = 0, lookY = 0, big = false, lid = true } = {}) {
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
function body({ arms = 'console', hood = false } = {}) {
  let s = '';
  // 腿、鞋
  s += path('M61,160 L60,173 C60,176 70,176 70,173 L70,160 Z', C.sock);
  s += path('M90,160 L90,173 C90,176 100,176 100,173 L99,160 Z', C.sock);
  s += `<ellipse cx="65" cy="175.5" rx="7" ry="3.5" fill="${C.shoe}" stroke="${C.line}" stroke-width="${SW}"/>`;
  s += `<ellipse cx="95" cy="175.5" rx="7" ry="3.5" fill="${C.shoe}" stroke="${C.line}" stroke-width="${SW}"/>`;
  // 裙子
  s += path('M49,152 L111,152 L116,165 C100,168 60,168 44,165 Z', C.skirt);
  s += stroke('M62,154 L60,166 M80,154 L80,167 M98,154 L100,166', C.skirtShade, 1.4);
  // 放下的兜帽在脖子后面（外套前片会盖住中间）
  if (!hood) s += hoodDown();
  s += `<rect x="74" y="96" width="12" height="10" fill="${C.skinShade}"/>`;
  // 外套
  s += path('M51,104 C43,109 39,120 40,134 L40,152 C40,158 46,160 52,160 L108,160 C114,160 120,158 120,152 L120,134 C121,120 117,109 109,104 Z', C.hood);
  s += fillOnly('M108,108 C115,116 117,128 117,140 L117,154 C114,157 110,158 106,158 Z', C.hoodShade, 'opacity="0.6"');
  // 衬衫、蝴蝶结
  s += path('M69,104 L80,121 L91,104 Z', C.shirt);
  s += path('M80,113 L72,109 L72,118 Z', C.ribbon) + path('M80,113 L88,109 L88,118 Z', C.ribbon);
  s += `<circle cx="80" cy="113.5" r="2.4" fill="${C.ribbon}" stroke="${C.line}" stroke-width="1.4"/>`;
  s += stroke('M78,115 L75,125 M82,115 L85,125', C.ribbon, 2.2);
  // 书包带
  s += path('M53,107 L59,106 L58,148 L52,148 Z', C.strap);
  s += path('M107,107 L101,106 L102,148 L108,148 Z', C.strap);
  // 外套口袋的小徽章
  s += `<rect x="96" y="128" width="7" height="8" rx="1.5" fill="${C.hoodLight}" stroke="${C.line}" stroke-width="1"/>`;
  if (arms === 'console') {
    // 袖子抱着游戏机
    s += path('M50,108 C41,118 44,136 59,139 L68,135 C61,131 58,123 58,114 Z', C.hood);
    s += path('M110,108 C119,118 116,136 101,139 L92,135 C99,131 102,123 102,114 Z', C.hood);
    s += consoleBox(80, 137);
    s += `<circle cx="63" cy="139" r="4" fill="${C.skin}" stroke="${C.line}" stroke-width="1.5"/>`;
    s += `<circle cx="97" cy="139" r="4" fill="${C.skin}" stroke="${C.line}" stroke-width="1.5"/>`;
  } else if (arms === 'cheer') {
    // 一只手举起游戏机
    s += path('M50,108 C41,118 44,136 59,139 L66,134 C60,130 58,122 58,114 Z', C.hood);
    s += path('M109,106 C118,98 124,86 126,74 L117,72 C114,84 108,94 102,100 Z', C.hood);
    s += g(consoleBox(122, 64), 'transform="rotate(-18 122 64)"');
    s += `<circle cx="120" cy="72" r="4" fill="${C.skin}" stroke="${C.line}" stroke-width="1.5"/>`;
    s += `<circle cx="62" cy="138" r="4" fill="${C.skin}" stroke="${C.line}" stroke-width="1.5"/>`;
  } else if (arms === 'cover') {
    // 两只袖子捂着脸颊（害羞）
    s += path('M50,108 C40,116 40,96 50,88 L58,92 C54,98 56,104 58,110 Z', C.hood);
    s += path('M110,108 C120,116 120,96 110,88 L102,92 C106,98 104,104 102,110 Z', C.hood);
  } else {
    // 手垂着
    s += path('M50,108 C43,120 43,138 48,146 L56,144 C54,134 55,122 58,114 Z', C.hood);
    s += path('M110,108 C117,120 117,138 112,146 L104,144 C106,134 105,122 102,114 Z', C.hood);
  }
  return s;
}

/** 游戏机（粉白色，屏幕上一个像素小怪） */
function consoleBox(cx, cy) {
  const x = cx - 17;
  const y = cy - 10;
  let s = `<rect x="${x}" y="${y}" width="34" height="20" rx="5" fill="${C.console}" stroke="${C.line}" stroke-width="${SW}"/>`;
  s += `<rect x="${x + 9}" y="${y + 3.5}" width="16" height="12" rx="1.5" fill="${C.screen}"/>`;
  const inv = ['X...X', '.XXX.', 'XX.XX', 'XXXXX', '.X.X.'];
  inv.forEach((row, ry) =>
    [...row].forEach((c, rx) => {
      if (c === 'X') s += `<rect x="${(x + 12 + rx * 2).toFixed(1)}" y="${(y + 5 + ry * 1.8).toFixed(1)}" width="2" height="1.8" fill="#8ff0c8"/>`;
    }),
  );
  s += `<rect x="${x + 2.5}" y="${y + 8.5}" width="5" height="1.8" fill="${C.line}"/><rect x="${x + 4.1}" y="${y + 6.9}" width="1.8" height="5" fill="${C.line}"/>`;
  s += `<circle cx="${x + 29}" cy="${y + 8}" r="1.6" fill="${C.pink}"/><circle cx="${x + 30.5}" cy="${y + 12}" r="1.6" fill="${C.pink}"/>`;
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
function draw({ eyes = 'open', mouth = 'smile', blush: b = 0, brows = '', arms = 'console', hood = false, tilt = 0, extra = '' }) {
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
    (hood ? '' : hairClip());
  const headG = tilt ? g(head, `transform="rotate(${tilt} 80 100)"`) : head;
  // 头画在身体后面一部分（后面的头发在身体后），所以：后发 → 身体 → 脸和刘海
  const back = hood ? hoodUp() : backHair();
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 160 180" width="160" height="180">${
    tilt ? g(back, `transform="rotate(${tilt} 80 100)"`) : back
  }${body({ arms, hood })}${headG}${extra}</svg>`;
}

/** 全部表情 / 动作：名字 → 组合 */
export const POSES = {
  // 状态
  idle: { eyes: 'open', mouth: 'smile', blush: 1 },
  talking: { eyes: 'open', mouth: 'open', blush: 1 },
  thinking: { eyes: 'up', mouth: 'flat', extra: dots },
  happy: { eyes: 'happy', mouth: 'grin', blush: 1, extra: sparkle(132, 30, 0.8) + sparkle(28, 40, 0.6) },
  sad: { eyes: 'tearful', mouth: 'frown', brows: 'worried', extra: sweat(122, 40) },
  sleep: { eyes: 'closed', mouth: 'o', blush: 1, tilt: -7, extra: zzz + bubble },
  // 待机变体（心情、夜里、天气、季节）
  idle_good: { eyes: 'open', mouth: 'cat', blush: 2, extra: notes },
  idle_low: { eyes: 'sleepy', mouth: 'frown', brows: 'worried' },
  idle_night: { eyes: 'sleepy', mouth: 'yawn', blush: 1, extra: text(122, 30, 'z', 12, C.hoodLight) },
  idle_rain: { eyes: 'open', mouth: 'flat', hood: true },
  idle_winter: { eyes: 'sleepy', mouth: 'smile', blush: 2, hood: true },
  // 反应
  pat: { eyes: 'happy', mouth: 'cat', blush: 2, extra: heart(126, 26, 1) + heart(36, 30, 0.7) },
  dizzy: { eyes: 'spiral', mouth: 'wavy', tilt: 6, extra: swirl + sweat(124, 44) },
  pout: { eyes: 'squint', mouth: 'pout', blush: 2, brows: 'angry', extra: anger },
  surprised: { eyes: 'wide', mouth: 'o', extra: exclaim },
  confused: { eyes: 'open', mouth: 'wavy', brows: 'worried', tilt: 5, extra: question },
  hood: { eyes: 'sleepy', mouth: 'flat', hood: true },
  win: { eyes: 'happy', mouth: 'grin', blush: 1, arms: 'cheer', extra: sparkle(30, 34, 0.9) + sparkle(36, 60, 0.6) + sparkle(140, 104, 0.6) },
  lose: { eyes: 'tearful', mouth: 'wavy', brows: 'worried', arms: 'down', extra: sweat(124, 40) },
  wink: { eyes: 'wink', mouth: 'cat', blush: 1, extra: sparkle(130, 66, 0.7) },
  shy: { eyes: 'side', mouth: 'smile', blush: 2, arms: 'cover' },
  wake: { eyes: 'sleepy', mouth: 'yawn', tilt: -4, extra: text(118, 30, '…', 18, C.hoodLight) },
};

mkdirSync(OUT, { recursive: true });
for (const [name, spec] of Object.entries(POSES)) {
  writeFileSync(join(OUT, `${name}.svg`), draw(spec) + '\n');
}
console.log(`画好了 ${Object.keys(POSES).length} 张 → ${OUT}`);
