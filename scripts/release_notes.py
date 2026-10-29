"""从 CHANGELOG.md 里取出某个版本的更新说明，作为 GitHub 发布页的说明（只有「改了什么」）。

用法：python scripts/release_notes.py v0.5.0 > notes.md
找不到这个版本的条目就报错——发布前必须先在 CHANGELOG.md 里写好。
"""

import os
import re
import sys


def section(text, version):
    out, on = [], False
    for line in text.splitlines():
        m = re.match(r"^## (\S+)", line)
        if m:
            if on:
                break
            on = m.group(1) == version
            continue
        if on:
            out.append(line)
    return "\n".join(out).strip()


def main():
    if len(sys.argv) != 2:
        sys.exit("用法：python scripts/release_notes.py v0.5.0")
    version = sys.argv[1].lstrip("v")
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    with open(os.path.join(root, "CHANGELOG.md"), encoding="utf-8") as f:
        body = section(f.read(), version)
    if not body:
        sys.exit(f"CHANGELOG.md 里没有 {version} 的更新说明。")
    sys.stdout.reconfigure(encoding="utf-8")
    print(body)


if __name__ == "__main__":
    main()
