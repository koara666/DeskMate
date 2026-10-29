"""把千秋的 Q 版图打成「形象包」，放到 GitHub 的单独发布页（DECISIONS #66）。

用法：python scripts/chibi/pack_art.py [图片目录] [输出文件]
  图片目录默认是 %APPDATA%/DeskMate/characters/nanami-chiaki/assets
  输出默认是仓库根目录的 chiaki-art-v1.zip（已写进 .gitignore，不会误提交）

做法：只收程序认识的图片名（character.rs 里的 STATES / IDLE_VARIANTS / POSES）、只收 PNG、
用「仅存储」不压缩（PNG 本来就压缩过），文件顺序和时间戳固定，所以同样的图每次打出来的包字节一模一样，
SHA-256 也一样——程序里写死的校验值才对得上。打完会打印 SHA-256，要把它填进 src-tauri/src/artpack.rs。
"""

import hashlib
import os
import sys
import zipfile

# 和 src-tauri/src/character.rs 里的三个列表保持一致
NAMES = [
    # STATES
    "idle", "talking", "thinking", "happy", "sad", "sleep",
    # IDLE_VARIANTS
    "idle_good", "idle_low", "idle_rain", "idle_snow", "idle_summer", "idle_winter", "idle_night",
    # POSES
    "pat", "dizzy", "pout", "surprised", "confused", "hood", "win", "lose", "wink", "shy", "wake",
]
PNG_MAGIC = b"\x89PNG\r\n\x1a\n"
MAX_EACH = 2 * 1024 * 1024


def main():
    src = sys.argv[1] if len(sys.argv) > 1 else os.path.join(
        os.environ["APPDATA"], "DeskMate", "characters", "nanami-chiaki", "assets")
    root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    out = sys.argv[2] if len(sys.argv) > 2 else os.path.join(root, "chiaki-art-v1.zip")

    files = []
    for name in sorted(NAMES):
        p = os.path.join(src, name + ".png")
        if not os.path.isfile(p):
            print(f"  （没有 {name}.png，跳过）")
            continue
        data = open(p, "rb").read()
        if not data.startswith(PNG_MAGIC):
            sys.exit(f"{p} 不是有效的 PNG")
        if len(data) > MAX_EACH:
            sys.exit(f"{p} 太大了（{len(data)} 字节，上限 {MAX_EACH}）")
        files.append((name + ".png", data))
    if not any(n == "idle.png" for n, _ in files):
        sys.exit("至少要有 idle.png")

    with zipfile.ZipFile(out, "w", zipfile.ZIP_STORED) as z:
        for name, data in files:
            info = zipfile.ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_STORED
            info.external_attr = 0o644 << 16
            z.writestr(info, data)

    blob = open(out, "rb").read()
    print(f"{len(files)} 张图 -> {out}")
    print(f"大小：{len(blob)} 字节（{len(blob) / 1048576:.2f} MiB）")
    print(f"SHA-256：{hashlib.sha256(blob).hexdigest()}")


if __name__ == "__main__":
    main()
