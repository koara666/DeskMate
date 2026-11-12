"""把初始角色（登记表里 bundled: true 的那位，现在是七海千秋）的角色包放进安装包：characters/_bundled/<id>.tar.bz2

  python scripts/chibi/bundle_default_pack.py

全新安装时程序直接把它装上（src-tauri/src/charpacks.rs 的 install_bundled），打开就能看到桌宠；之后换角色、删掉都随用户。
先用本机 dist-packs/<id>.tar.bz2（build_character_pack.py 打出来的），校验值对得上就用；没有或对不上就从发布页下载。
校验值必须和登记表一致（程序装的时候还会再核对一次），对不上就失败——免得发出去的安装包里是坏的初始角色。
发布流程（release.yml）里在构建安装包之前自动跑。
"""

import hashlib
import os
import shutil
import sys
import urllib.request

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import builtin_characters as bc  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    starters = [(cid, c) for cid, c in bc.registry().items() if c.get("bundled")]
    if len(starters) != 1:
        sys.exit(f"登记表里要正好有一位 bundled: true 的初始角色，现在有 {len(starters)} 位")
    cid, c = starters[0]
    want = c["pack"]["sha256"]
    out_dir = os.path.join(ROOT, "characters", "_bundled")
    os.makedirs(out_dir, exist_ok=True)
    out = os.path.join(out_dir, f"{cid}.tar.bz2")
    # 旧的（别的角色、旧版本）清掉，安装包里只留这一份
    for name in os.listdir(out_dir):
        if name.endswith(".tar.bz2") and name != os.path.basename(out):
            os.remove(os.path.join(out_dir, name))
    if os.path.isfile(out) and sha256(out) == want:
        print(f"  {c['label']}：安装包里的角色包已经是对的（v{c['pack']['version']}）")
        return
    local = os.path.join(ROOT, "dist-packs", f"{cid}.tar.bz2")
    if os.path.isfile(local) and sha256(local) == want:
        shutil.copyfile(local, out)
        print(f"  {c['label']}：用本机打好的角色包（dist-packs，v{c['pack']['version']}）")
        return
    url = bc.packs("pack")[cid][1]
    print(f"  {c['label']}：从发布页下载 {url}")
    tmp = out + ".part"
    req = urllib.request.Request(url, headers={"User-Agent": "DeskMate-build"})
    with urllib.request.urlopen(req, timeout=300) as r, open(tmp, "wb") as f:
        shutil.copyfileobj(r, f)
    got = sha256(tmp)
    if got != want:
        os.remove(tmp)
        sys.exit(f"{c['label']} 的角色包校验没通过：下载到 {got}，登记表里是 {want}")
    os.replace(tmp, out)
    print(f"  {c['label']}：角色包 {os.path.getsize(out)} 字节，校验通过")


if __name__ == "__main__":
    main()
