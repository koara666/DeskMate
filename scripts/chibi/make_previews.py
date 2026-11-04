"""给每位可下载的角色做一张小预览图，随安装包带上（用户还没下载角色之前，选角色的页面上要能看见 TA 长什么样）。

  python scripts/chibi/make_previews.py

把 characters/<id>/assets/idle.png（先用 fetch_art.py 取下来）复制到 characters/_catalog/<id>.png。
预览图和 Q 版形象一样不进代码仓库（.gitignore）；发布流程里在构建安装包之前自动跑，缺图就失败，
免得发出去的安装包里选角色页是空白的。
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import builtin_characters as bc  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
PNG_MAGIC = b"\x89PNG\r\n\x1a\n"
MAX_BYTES = 512 * 1024


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    out = os.path.join(ROOT, "characters", "_catalog")
    os.makedirs(out, exist_ok=True)
    missing = []
    for cid, c in bc.registry().items():
        src = os.path.join(ROOT, "characters", cid, "assets", "idle.png")
        if not os.path.isfile(src):
            missing.append(cid)
            continue
        data = open(src, "rb").read()
        if not data.startswith(PNG_MAGIC) or len(data) > MAX_BYTES:
            sys.exit(f"{cid} 的 idle.png 不是 PNG，或者超过 {MAX_BYTES} 字节")
        with open(os.path.join(out, cid + ".png"), "wb") as f:
            f.write(data)
        print(f"  {c['label']}：预览图 {len(data)} 字节")
    if missing:
        sys.exit("这些角色还没有 Q 版形象（先跑 scripts/chibi/fetch_art.py）：" + "、".join(missing))


if __name__ == "__main__":
    main()
