"""把一个角色打成「角色包」（tar.bz2），软件里用户挑哪位角色就下载哪位（src-tauri/src/charpacks.rs）。

用法：
  python scripts/chibi/build_character_pack.py nanami-chiaki makise-kurisu       打包这几位（输出到 dist-packs/）
  python scripts/chibi/build_character_pack.py --all                              打包登记表里的全部角色
  python scripts/chibi/build_character_pack.py --all --update-registry            打完把版本、校验值、大小、名字简介写回登记表
  加 --version N：指定这次的版本号（默认沿用登记表里的；内容变了就要升一版再传新的发布页）

角色文件夹 characters/<id>/ 里要已经有 Q 版形象和预录语音（assets/*.png、voice/）——它们不进仓库，
先用 scripts/chibi/fetch_art.py 和 scripts/voice/fetch_voice_pack.py 从发布页取下来（或者本机刚做好的就在那里）。

包里只放白名单里的文件（和程序解压时认的一模一样，多一个都会被程序拒绝）：
  character.json persona.md canon.md quotes.md / lines/*.md / assets/*.{png,webp,jpg,jpeg,gif,svg} / voice/index.json、voice/<32位十六进制>.opus
打包是确定的（排好序、时间戳清零、固定压缩级别），同样的内容打出来字节一样。

发布：传到 GitHub 发布页 character-<短名>-v<N>（预发布、不要勾 Latest；标签不能以 v 加数字开头），附件名 <id>.tar.bz2。
"""

import argparse
import bz2
import hashlib
import io
import json
import os
import re
import sys
import tarfile

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import builtin_characters as bc  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
NAME = re.compile(r"^[A-Za-z0-9_\-][A-Za-z0-9_.\-]{0,79}$")
IMG = ("png", "webp", "jpg", "jpeg", "gif", "svg")
TOP = ("character.json", "persona.md", "canon.md", "quotes.md")
MAX_FILE = 8 * 1024 * 1024
MAX_TOTAL = 96 * 1024 * 1024


def allowed(rel):
    """rel 是 characters/<id>/ 之后的相对路径（正斜杠）。和 charpacks.rs 的 entry_allowed 同一套规则。"""
    parts = rel.split("/")
    if len(parts) == 1:
        return parts[0] in TOP
    if len(parts) != 2 or not NAME.match(parts[1]):
        return False
    d, n = parts
    if d == "lines":
        return n.endswith(".md")
    if d == "assets":
        return n.rsplit(".", 1)[-1] in IMG
    if d == "voice":
        return n == "index.json" or re.fullmatch(r"[0-9a-f]{32}\.opus", n) is not None
    return False


def collect(cid):
    base = os.path.join(ROOT, "characters", cid)
    if not os.path.isdir(base):
        sys.exit(f"{cid}：找不到 characters/{cid}/")
    files, skipped = [], []
    for dp, _dirs, names in os.walk(base):
        for n in names:
            full = os.path.join(dp, n)
            rel = os.path.relpath(full, base).replace(os.sep, "/")
            if allowed(rel):
                files.append((rel, full))
            else:
                skipped.append(rel)
    if skipped:
        sys.exit(f"{cid}：文件夹里有不在白名单里的文件，程序解压时会拒绝，先挪走：{', '.join(sorted(skipped)[:8])}")
    files.sort()
    rels = {r for r, _ in files}
    for need in ("character.json", "persona.md", "assets/idle.png", "voice/index.json"):
        if need not in rels:
            sys.exit(f"{cid}：缺少 {need}（先用 fetch_art.py / fetch_voice_pack.py 把形象和语音取下来）")
    total = 0
    for rel, full in files:
        size = os.path.getsize(full)
        if size > MAX_FILE:
            sys.exit(f"{cid}/{rel} 有 {size} 字节，超过单个文件 {MAX_FILE} 的上限")
        total += size
    if total > MAX_TOTAL:
        sys.exit(f"{cid}：解压后共 {total} 字节，超过上限 {MAX_TOTAL}")
    # 声音：index.json 里每一句都要有对应的文件
    idx = json.load(open(os.path.join(base, "voice", "index.json"), encoding="utf-8")).get("lines", {})
    missing = [i for i in idx if f"voice/{i}.opus" not in rels]
    if missing:
        sys.exit(f"{cid}：语音包 index.json 里有 {len(missing)} 句没有对应的 .opus 文件")
    return files


def build(cid, out_dir):
    files = collect(cid)
    raw = io.BytesIO()
    with tarfile.open(fileobj=raw, mode="w", format=tarfile.GNU_FORMAT) as tar:
        for rel, full in files:
            with open(full, "rb") as f:
                data = f.read()
            if rel.endswith((".md", ".json", ".svg")):
                # 文本统一成 LF：不管是在哪台机器上打的，同样的内容字节一样
                data = data.replace(b"\r\n", b"\n")
            ti = tarfile.TarInfo(f"{cid}/{rel}")
            ti.size = len(data)
            ti.mtime = 0
            ti.mode = 0o644
            ti.uid = ti.gid = 0
            ti.uname = ti.gname = ""
            tar.addfile(ti, io.BytesIO(data))
    blob = bz2.compress(raw.getvalue(), 9)
    os.makedirs(out_dir, exist_ok=True)
    path = os.path.join(out_dir, f"{cid}.tar.bz2")
    with open(path, "wb") as f:
        f.write(blob)
    return path, hashlib.sha256(blob).hexdigest(), len(blob), len(files)


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    ap = argparse.ArgumentParser()
    ap.add_argument("ids", nargs="*")
    ap.add_argument("--all", action="store_true")
    ap.add_argument("--out", default=os.path.join(ROOT, "dist-packs"))
    ap.add_argument("--update-registry", action="store_true")
    ap.add_argument("--version", type=int)
    a = ap.parse_args()
    reg_path = os.path.join(ROOT, "scripts", "builtin_characters.json")
    full = json.load(open(reg_path, encoding="utf-8"))
    ids = list(bc.registry()) if a.all else a.ids
    if not ids:
        ap.error("要打包哪个角色？写 id，或者 --all")
    for cid in ids:
        if cid not in full:
            sys.exit(f"{cid} 没登记在 scripts/builtin_characters.json")
        path, sha, size, n = build(cid, a.out)
        print(f"{cid}: {n} 个文件 → {path}\n  大小 {size} 字节  SHA-256 {sha}")
        if a.update_registry:
            m = json.load(open(os.path.join(ROOT, "characters", cid, "character.json"), encoding="utf-8"))
            e = full[cid]
            old = (e.get("pack") or {}).get("version", 0)
            version = a.version or old or 1
            e["name"] = m.get("name", e.get("label", cid))
            e["description"] = m.get("description", "")
            e["color"] = m.get("color", "")
            e["order"] = m.get("order", 99)
            e["pack"] = {"version": version, "sha256": sha, "size": size}
    if a.update_registry:
        full["_说明"] = (
            "内置「完整角色」登记表：每个有 Q 版形象 + 日语声音的角色都在这里登记。软件里用户挑哪位角色就下载哪位的「角色包」"
            "（pack：设定 + 台词 + Q 版形象 + 预录语音，发布页 character-<短名>-v<N>，附件 <id>.tar.bz2），软件本体不带任何角色；"
            "形象包 art、语音包 voicepack 是打角色包的原料，声音模型 model 是想要每句都出声的人自己下的。"
            "地址按固定规则从这里推出来（scripts/builtin_characters.py）。新增角色只改这个文件，不要再往别处抄地址和校验值。"
        )
        with open(reg_path, "w", encoding="utf-8", newline="\n") as f:
            json.dump(full, f, ensure_ascii=False, indent=2)
            f.write("\n")
        print("已写回登记表")


if __name__ == "__main__":
    main()
