# -*- coding: utf-8 -*-
"""把一整张 Q 版表情拼图（白底、每个人物下面一个粉色名字标签）切成一张张透明背景的 PNG。

用法：
  python scripts/chibi/slice_sheet.py nanami.png 输出文件夹 [--preview 预览.png]
- 名字按拼图里从左到右、从上到下的顺序对应 NAMES（和 docs/art/CHIAKI_PROMPTS.md 第四节一致）。
- 人物和它周围的小装饰（星星、音符、问号……）按连通区域分组，归到最近的标签下面；
  白底从边缘往里填充去掉（衣领、眼睛高光这些被描边包住的白色不受影响），边缘做去白边处理。
- 所有图放在同样大小的画布上、底边对齐、同一个缩放比例，换图时人物不会跳。
需要 numpy、scipy、Pillow。图片是用户自己生成的，只放在用户数据目录，不进仓库。
"""
import argparse
import os

import numpy as np
from PIL import Image
from scipy import ndimage

NAMES = [
    "idle", "talking", "thinking", "happy", "sad", "sleep",
    "idle_good", "idle_low", "idle_night", "idle_rain", "idle_winter", "idle_summer",
    "pat", "shy", "dizzy", "pout", "surprised", "confused",
    "hood", "win", "lose", "wink", "wake",
]


def bands(mask_rows, min_px):
    """一维：连续超过阈值的区间"""
    out, start = [], None
    for y, v in enumerate(mask_rows):
        if v > min_px and start is None:
            start = y
        elif v <= min_px and start is not None:
            out.append((start, y))
            start = None
    if start is not None:
        out.append((start, len(mask_rows)))
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("sheet")
    ap.add_argument("out")
    ap.add_argument("--preview")
    a = ap.parse_args()

    rgb = np.asarray(Image.open(a.sheet).convert("RGB")).astype(np.int16)
    r, g, b = rgb[..., 0], rgb[..., 1], rgb[..., 2]
    h, w = r.shape
    lo = rgb.min(2)
    # 名字标签：淡粉色圆角条
    pink = (r > 235) & (g > 205) & (g < 235) & (b > 225) & (r - g > 12)
    label_rows = [(s, e) for s, e in bands(pink.sum(1), 40) if e - s >= 20]
    labels = []  # (行号, 中心 x, 标签顶部 y)
    label_box = np.zeros_like(pink)  # 只有标签那几个矩形算标签（腮红、头发高光也是淡粉色）
    for row, (s, e) in enumerate(label_rows):
        for xs, xe in bands(pink[s:e].sum(0), 3):
            if xe - xs > 40:
                labels.append((row, (xs + xe) / 2, s))
                label_box[max(0, s - 3):e + 3, max(0, xs - 3):xe + 3] = True
    if len(labels) != len(NAMES):
        raise SystemExit("找到 %d 个标签，应该是 %d 个" % (len(labels), len(NAMES)))

    # 背景 = 从图片边缘连通的「几乎纯白」（背景各通道相差 ≤ 2；她的皮肤是暖白，相差 15 以上，不能算进来）
    whiteish = (lo > 244) & (rgb.max(2) - lo < 6)
    lab, _ = ndimage.label(whiteish)
    edge_ids = set(np.unique(np.concatenate([lab[0], lab[-1], lab[:, 0], lab[:, -1]]))) - {0}
    bg = np.isin(lab, list(edge_ids))
    fg = ~bg & ~label_box
    # 每一行人物的纵向范围：上一条标签的下沿 ~ 这一条标签的上沿
    tops = [0] + [e for _s, e in label_rows[:-1]]
    comps, n = ndimage.label(fg)
    objs = ndimage.find_objects(comps)
    groups = {i: [] for i in range(len(labels))}
    for ci, sl in enumerate(objs, 1):
        ys, xs = sl
        size = int((comps[sl] == ci).sum())
        if size < 12:
            continue
        cy = (ys.start + ys.stop) / 2
        cx = (xs.start + xs.stop) / 2
        row = next((k for k, (s, _e) in enumerate(label_rows) if tops[k] <= cy < s), None)
        if row is None:
            continue
        cand = [(abs(lx - cx), i) for i, (lr, lx, _t) in enumerate(labels) if lr == row]
        groups[min(cand)[1]].append(ci)

    # 去白边：前景里挨着背景的两圈像素，按「离白色多远」给透明度，并把颜色里混进去的白色减掉
    near_bg = ndimage.binary_dilation(bg, iterations=2) & fg
    alpha = np.where(fg, 255, 0).astype(np.float32)
    edge_a = np.clip((255 - lo) / (255 - 150) * 255, 0, 255)
    alpha[near_bg] = edge_a[near_bg]
    af = np.maximum(alpha / 255, 1e-3)[..., None]
    col = np.clip((rgb - (1 - af) * 255) / af, 0, 255)
    rgba = np.dstack([col, alpha]).astype(np.uint8)

    crops = []
    for i, name in enumerate(NAMES):
        if not groups[i]:
            raise SystemExit("%s 没有找到人物" % name)
        m = np.isin(comps, groups[i])
        ys, xs = np.where(m)
        y0, y1, x0, x1 = ys.min(), ys.max() + 1, xs.min(), xs.max() + 1
        piece = rgba[y0:y1, x0:x1].copy()
        piece[..., 3] = np.where(m[y0:y1, x0:x1], piece[..., 3], 0)
        crops.append((name, piece))

    pad = 6
    cw = max(p.shape[1] for _n, p in crops) + pad * 2
    ch = max(p.shape[0] for _n, p in crops) + pad
    os.makedirs(a.out, exist_ok=True)
    tiles = []
    for name, p in crops:
        canvas = Image.new("RGBA", (cw, ch), (0, 0, 0, 0))
        canvas.paste(Image.fromarray(p, "RGBA"), ((cw - p.shape[1]) // 2, ch - p.shape[0]))
        canvas.save(os.path.join(a.out, name + ".png"), optimize=True)
        tiles.append(canvas)
    print("切好 %d 张，每张 %d×%d → %s" % (len(tiles), cw, ch, a.out))

    if a.preview:  # 棋盘格底的预览图，方便看抠得干不干净
        cols = 6
        rows = (len(tiles) + cols - 1) // cols
        sheet = Image.new("RGBA", (cw * cols, ch * rows), (0, 0, 0, 0))
        checker = Image.new("RGBA", sheet.size, (255, 255, 255, 255))
        px = checker.load()
        for y in range(sheet.size[1]):
            for x in range(sheet.size[0]):
                if (x // 12 + y // 12) % 2:
                    px[x, y] = (200, 220, 255, 255)
        for k, t in enumerate(tiles):
            sheet.paste(t, ((k % cols) * cw, (k // cols) * ch), t)
        Image.alpha_composite(checker, sheet).convert("RGB").save(a.preview)


if __name__ == "__main__":
    main()
