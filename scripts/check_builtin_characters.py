"""检查内置完整角色是不是都按同一套做法打包（CI 里自动跑；本机推送 / 打标签前也可以跑）。

  python scripts/check_builtin_characters.py            离线检查（登记表、角色文件夹、文档、流水线、仓库里没有版权图和录音）
  python scripts/check_builtin_characters.py --online   再去 GitHub 发布页核对包是否已经传好、校验值是否对得上

规则（每个有 Q 版形象 + 日语声音的角色都一样，登记在 scripts/builtin_characters.json）：
  1. 每个角色打成「角色包」放发布页，用户在软件里选了哪位就下载哪位；登记表里的校验值编进程序，下载下来对不上就不装。
     只有一位初始角色（bundled: true，七海千秋）的角色包随安装包带着，全新安装直接装上，之后换、删都随用户
  2. 形象包、预录语音包是打角色包的原料，也放发布页；声音模型也放发布页，不在角色包里
  3. 图和录音不进代码仓库，仓库里只有代码画的 SVG 兜底和日语台词
  4. 有日语台词（zh || ja）的角色必须登记，登记了就必须四个包齐全
  5. docs/VOICE.md 里列出每个角色的模型发布页和校验值
"""

import hashlib
import os
import re
import subprocess
import sys
import urllib.error
import urllib.request

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import builtin_characters as bc  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
problems = []


def bad(msg):
    problems.append(msg)


def read(*parts):
    with open(os.path.join(ROOT, *parts), encoding="utf-8") as f:
        return f.read()


def offline():
    reg = bc.registry()
    chars_dir = os.path.join(ROOT, "characters")

    for cid, c in reg.items():
        where = f"登记表里的 {cid}"
        if not re.fullmatch(r"[a-z]+", c.get("short", "")):
            bad(f"{where}：short（短名）要全是小写字母")
        if not c.get("label"):
            bad(f"{where}：缺少 label（显示名）")
        for kind in bc.ALL_KINDS:
            p = c.get(kind) or {}
            if not isinstance(p.get("version"), int) or p["version"] < 1:
                bad(f"{where}：{kind}.version 要是 ≥1 的整数")
            if not re.fullmatch(r"[0-9a-f]{64}", p.get("sha256", "")):
                bad(f"{where}：{kind}.sha256 要是 64 位小写十六进制")
            if p.get("version") and re.match(r"v\d", bc.tag(c["short"], kind, p["version"])):
                bad(f"{where}：{kind} 的标签不能以 v 加数字开头")
        # 选角色页在下载之前就要显示名字、简介、大小，这些编进程序，必须和角色文件夹里的一致
        pk = c.get("pack") or {}
        if not isinstance(pk.get("size"), int) or not 0 < pk["size"] < 64 * 1024 * 1024:
            bad(f"{where}：pack.size 要是下载大小（字节，小于 64 MB）")
        if not c.get("name") or not c.get("description"):
            bad(f"{where}：缺少 name / description（scripts/chibi/build_character_pack.py --update-registry 会从 character.json 写进来）")
        cj = os.path.join(chars_dir, cid, "character.json")
        if os.path.isfile(cj):
            import json
            m = json.load(open(cj, encoding="utf-8"))
            for key in ("name", "description", "color"):
                if c.get(key) != m.get(key, ""):
                    bad(f"{where}：{key} 和 characters/{cid}/character.json 对不上——改完角色设定要重新打包并 --update-registry")
        d = os.path.join(chars_dir, cid)
        if not os.path.isdir(d):
            bad(f"{where}：characters/{cid}/ 不存在")
            continue
        for f in ("character.json", "persona.md", "canon.md", "quotes.md", "lines/voice.md", "assets/placeholder.svg"):
            if not os.path.isfile(os.path.join(d, f)):
                bad(f"{cid}：缺少 characters/{cid}/{f}（完整角色都要有）")
        # 每个角色的声音页「试听一句」要有日语
        vm = os.path.join(d, "lines", "voice.md")
        if os.path.isfile(vm) and "||" not in read("characters", cid, "lines", "voice.md"):
            bad(f"{cid}：lines/voice.md 里没有日语试听台词（zh || ja）")

    # 反过来：有日语台词的角色必须登记
    for cid in sorted(os.listdir(chars_dir)):
        ld = os.path.join(chars_dir, cid, "lines")
        if cid in reg or not os.path.isdir(ld):
            continue
        for name in os.listdir(ld):
            if name.endswith(".md") and "||" in read("characters", cid, "lines", name):
                bad(f"{cid} 有日语台词（lines/{name}）但没登记到 scripts/builtin_characters.json，"
                    "按规矩要有形象包、语音包、声音模型三个发布页")
                break

    # 文档：每个角色的模型发布页和校验值
    voice_doc = read("docs", "VOICE.md")
    for cid, c in reg.items():
        m = c["model"]
        t = bc.tag(c["short"], "model", m["version"])
        if t not in voice_doc or m["sha256"] not in voice_doc:
            bad(f"docs/VOICE.md 里没有 {c['label']} 的模型发布页（{t}）或校验值")

    # 流水线：预览图要有、角色包必须真的传好了（否则发出去的软件里选了角色下载不下来）
    rel = read(".github", "workflows", "release.yml")
    starters = [cid for cid, c in reg.items() if c.get("bundled")]
    if len(starters) != 1:
        bad(f"登记表里要正好有一位 bundled: true 的初始角色（随安装包带着），现在是 {starters}")
    for script in ("scripts/chibi/fetch_art.py --require", "scripts/chibi/make_previews.py", "scripts/chibi/bundle_default_pack.py", "scripts/check_builtin_characters.py --online"):
        if script not in rel:
            bad(f"release.yml 里没有 `python {script}`（缺包时发布要失败，保证下载版 = 本机版）")
    conf = read("src-tauri", "tauri.conf.json")
    if '"../characters/": ' in conf or "../characters/nanami" in conf or "../characters/makise" in conf:
        bad("tauri.conf.json 的 resources 里不能带角色文件夹：角色是用户选了才下载的（初始角色以角色包的形式放在 characters/_bundled/）")
    if '"../characters/_bundled/"' not in conf:
        bad("tauri.conf.json 的 resources 里没有 characters/_bundled/：初始角色的角色包要随安装包带着")

    # .gitignore：图和录音不进仓库
    ign = read(".gitignore")
    for pat in ("characters/*/assets/*.png", "/characters/*/voice/", "/characters/_catalog/*.png", "/characters/_bundled/*.tar.bz2", "/dist-packs/"):
        if pat not in ign:
            bad(f".gitignore 里没有 {pat}")

    # 仓库里没有版权图和录音（git 跟踪的文件）
    try:
        tracked = subprocess.run(["git", "ls-files", "characters"], cwd=ROOT, capture_output=True, text=True, check=True).stdout.splitlines()
    except Exception:
        tracked = []  # 没有 git（解压的源码快照）就跳过这一项
    for f in tracked:
        if re.search(r"/assets/[^/]+\.(png|jpe?g|webp|gif)$", f) or "/voice/" in f:
            bad(f"{f} 被 git 跟踪了：图和录音只能放发布页，不进代码仓库")


def get(url, byte_range=False):
    headers = {"User-Agent": "DeskMate-check"}
    if byte_range:
        headers["Range"] = "bytes=0-0"
    with urllib.request.urlopen(urllib.request.Request(url, headers=headers), timeout=90) as r:
        return r.read() if not byte_range else b""


def online():
    for cid, c in bc.registry().items():
        for kind in bc.ALL_KINDS:
            v = c[kind]["version"]
            url = f"{bc.BASE}/{bc.tag(c['short'], kind, v)}/{bc.filename(c['short'], kind, v, cid)}"
            try:
                if kind == "model":
                    get(url, byte_range=True)  # 304 MB，只确认传上去了
                    print(f"  有  {c['label']}·声音模型  {url}")
                else:
                    sha = hashlib.sha256(get(url)).hexdigest()
                    if sha != c[kind]["sha256"]:
                        bad(f"{url} 校验值对不上：期望 {c[kind]['sha256']}，实际 {sha}")
                    else:
                        print(f"  有  {c['label']}·{ {'pack': '角色包', 'art': '形象包'}.get(kind, '语音包') }  校验通过")
            except urllib.error.HTTPError as e:
                bad(f"发布页上还没有 {url}（{e.code}）——先传好四个发布页再推 v* 标签")
            except Exception as e:
                bad(f"访问 {url} 失败：{e}")


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    offline()
    if "--online" in sys.argv:
        online()
    if problems:
        print("内置角色打包规则没通过：")
        for p in problems:
            print(f"  - {p}")
        sys.exit(1)
    print(f"内置角色打包规则检查通过（{', '.join(bc.registry())}）")


if __name__ == "__main__":
    main()
