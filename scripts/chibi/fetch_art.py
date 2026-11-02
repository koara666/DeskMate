"""构建安装包时，把内置角色的 Q 版形象放进角色包（DECISIONS #66）：用户装完 / 更新完就是 Q 版，不用自己放文件。

用法：
  python scripts/chibi/fetch_art.py                      从 GitHub 发布页下载所有角色的形象包（发布流程里自动调用）
  python scripts/chibi/fetch_art.py --only makise-kurisu 只处理这一个角色
  python scripts/chibi/fetch_art.py --from-zip 角色id=X   用本地的包（本机打包、还没上传时用；同样要校验，可以重复写）
  加 --require：发布页上找不到就失败（发布用）；不加只警告、不带图继续构建

- 图不进代码仓库（解出来的 characters/<角色>/assets/*.png 已写进 .gitignore）：
  它们放在发布页 art-<角色>-v1，哪天需要撤下，删掉那个附件，之后的安装包就不再带。
- 发布页上没有（404）：只警告、继续，安装包里不带（用代码画的内置形象兜底）。
- 下载到了但 SHA-256 对不上：直接失败——不能把来路不明的图打进安装包。
- 只解压名字是「小写字母和下划线 + .png」、文件头是 PNG、不超过 2 MB 的条目。
- 换图时重新打包、传新发布页、改下面的 PACKS。
"""

import hashlib
import io
import os
import re
import sys
import time
import urllib.error
import urllib.request
import zipfile

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import builtin_characters  # noqa: E402

# 角色 id → （显示名，下载地址，SHA-256）；登记表和命名规则在 scripts/builtin_characters.py
PACKS = builtin_characters.packs("art")
MAX_ZIP = 20 * 1024 * 1024
MAX_EACH = 2 * 1024 * 1024
NAME = re.compile(r"^[a-z]+(_[a-z]+)?\.png$")
PNG_MAGIC = b"\x89PNG\r\n\x1a\n"


def warn(msg):
    # GitHub Actions 里会显示成黄色警告；本机就是一行字
    print(f"::warning::{msg}" if os.environ.get("GITHUB_ACTIONS") else f"警告：{msg}")


def download(url):
    last = None
    for i in range(3):
        try:
            req = urllib.request.Request(url, headers={"User-Agent": "DeskMate-build"})
            with urllib.request.urlopen(req, timeout=60) as r:
                if not r.geturl().startswith("https://"):
                    sys.exit("下载地址被跳转到了不安全的地址，已停止。")
                data = r.read(MAX_ZIP + 1)
                if len(data) > MAX_ZIP:
                    sys.exit("形象包比预期大得多，已停止。")
                return data
        except urllib.error.HTTPError as e:
            if e.code == 404:
                return None
            last = e
        except Exception as e:  # 网络抖动：重试
            last = e
        time.sleep(3 * (i + 1))
    sys.exit(f"下载形象包失败（重试 3 次）：{last}")


def local_zips():
    out = {}
    args = sys.argv
    for i, a in enumerate(args):
        if a == "--from-zip":
            cid, _, path = args[i + 1].partition("=")
            out[cid] = path
    return out


def fetch_one(root, cid, locals_):
    label, url, sha = PACKS[cid]
    dest = os.path.join(root, "characters", cid, "assets")
    if cid in locals_:
        data = open(locals_[cid], "rb").read()
    else:
        data = download(url)
        if data is None:
            msg = f"{label}形象包在 GitHub 发布页上找不到（{url.split('/')[-2]}），这次的安装包不带{label}的 Q 版形象。"
            if "--require" in sys.argv:
                sys.exit(msg)  # 发布用：保证装上的版本和本机一样
            warn(msg)
            return
    got = hashlib.sha256(data).hexdigest()
    if got != sha:
        sys.exit(f"{label}形象包校验没通过：期望 {sha}，实际 {got}。不能打进安装包。")

    n = 0
    os.makedirs(dest, exist_ok=True)
    with zipfile.ZipFile(io.BytesIO(data)) as z:
        for info in z.infolist():
            if not NAME.match(info.filename) or info.file_size > MAX_EACH:
                continue
            body = z.read(info)
            if not body.startswith(PNG_MAGIC):
                continue
            with open(os.path.join(dest, info.filename), "wb") as f:
                f.write(body)
            n += 1
    if n == 0:
        sys.exit(f"{label}形象包里没有可用的图片。")
    print(f"{label}形象包：放进了 {n} 张图 -> {dest}")


def main():
    # Windows 构建机的控制台默认不是 UTF-8，打印中文会报错退出
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    only = sys.argv[sys.argv.index("--only") + 1] if "--only" in sys.argv else None
    if only and only not in PACKS:
        sys.exit(f"不认识的角色 {only}，可选：{', '.join(PACKS)}")
    locals_ = local_zips()
    for cid in PACKS:
        if only and cid != only:
            continue
        fetch_one(root, cid, locals_)


if __name__ == "__main__":
    main()
