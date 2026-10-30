"""构建安装包时，把千秋的预录语音包放进内置角色包：用户装完 / 更新完就能听到她的声音，不用自己下载或放文件。

用法：
  python scripts/voice/fetch_voice_pack.py              从 GitHub 发布页下载（发布流程里自动调用）
  python scripts/voice/fetch_voice_pack.py --from-zip X 用本地的包（本机打包、还没上传时用；同样要校验）
  加 --require：发布页上找不到就失败（发布用，保证装上的版本和本机一样）；不加只警告、不带语音继续构建

- 语音不进代码仓库（解出来的 characters/nanami-chiaki/voice/ 已写进 .gitignore）：
  放在发布页 voicepack-chiaki-v1，哪天需要撤下，删掉那个附件，之后的安装包就不再带。
- 下载到了但 SHA-256 对不上：直接失败——不能把来路不明的东西打进安装包。
- 只解压 index.json 和「32 位十六进制 + .opus」，文件头必须是 Ogg，每个不超过 1 MB。
- 每句的编号和程序里查找的算法见 src-tauri/src/voice.rs（pack_id）。
"""

import hashlib
import io
import json
import os
import re
import shutil
import sys
import time
import urllib.error
import urllib.request
import zipfile

URL = "https://github.com/koara666/DeskMate/releases/download/voicepack-chiaki-v1/chiaki-voicepack-v1.zip"
SHA256 = "afe8561b7aaa860951fbbb17ad83f7d2a64dacef45e345d430d1184a0da22e80"
MAX_ZIP = 30 * 1024 * 1024
MAX_EACH = 1024 * 1024
NAME = re.compile(r"^[0-9a-f]{32}\.opus$")
OGG_MAGIC = b"OggS"


def warn(msg):
    # GitHub Actions 里会显示成黄色警告；本机就是一行字
    print(f"::warning::{msg}" if os.environ.get("GITHUB_ACTIONS") else f"警告：{msg}")


def download():
    last = None
    for i in range(3):
        try:
            req = urllib.request.Request(URL, headers={"User-Agent": "DeskMate-build"})
            with urllib.request.urlopen(req, timeout=60) as r:
                if not r.geturl().startswith("https://"):
                    sys.exit("下载地址被跳转到了不安全的地址，已停止。")
                data = r.read(MAX_ZIP + 1)
                if len(data) > MAX_ZIP:
                    sys.exit("语音包比预期大得多，已停止。")
                return data
        except urllib.error.HTTPError as e:
            if e.code == 404:
                return None
            last = e
        except Exception as e:  # 网络抖动：重试
            last = e
        time.sleep(3 * (i + 1))
    sys.exit(f"下载语音包失败（重试 3 次）：{last}")


def main():
    # Windows 构建机的控制台默认不是 UTF-8，打印中文会报错退出
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    dest = os.path.join(root, "characters", "nanami-chiaki", "voice")
    if "--from-zip" in sys.argv:
        data = open(sys.argv[sys.argv.index("--from-zip") + 1], "rb").read()
    else:
        data = download()
        if data is None:
            msg = "千秋预录语音包在 GitHub 发布页上找不到（voicepack-chiaki-v1），这次的安装包不带预录语音。"
            if "--require" in sys.argv:
                sys.exit(msg)
            warn(msg)
            return
    got = hashlib.sha256(data).hexdigest()
    if got != SHA256:
        sys.exit(f"语音包校验没通过：期望 {SHA256}，实际 {got}。不能打进安装包。")

    files = {}
    with zipfile.ZipFile(io.BytesIO(data)) as z:
        for info in z.infolist():
            if (info.filename != "index.json" and not NAME.match(info.filename)) or info.file_size > MAX_EACH:
                continue
            body = z.read(info)
            if info.filename != "index.json" and not body.startswith(OGG_MAGIC):
                continue
            files[info.filename] = body
    if "index.json" not in files:
        sys.exit("语音包里没有 index.json。")
    lines = json.loads(files["index.json"]).get("lines", {})
    missing = [k for k in lines if f"{k}.opus" not in files]
    if not lines or missing:
        sys.exit(f"语音包不完整：索引 {len(lines)} 句，缺 {len(missing)} 个音频文件。")

    # 先清掉旧的再放，免得上一版留下的句子混进来
    shutil.rmtree(dest, ignore_errors=True)
    os.makedirs(dest)
    for name, body in files.items():
        with open(os.path.join(dest, name), "wb") as f:
            f.write(body)
    print(f"千秋预录语音包：放进了 {len(lines)} 句 -> {dest}")


if __name__ == "__main__":
    main()
