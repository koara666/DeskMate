# -*- coding: utf-8 -*-
"""往网络盘（比如 sshfs 挂载的 Q 盘）安全地拷一个大文件，或者从网络盘拷回来。

用法：python scripts/voice-train/safe_copy.py 源文件 目标文件夹 [--mbps 20]
- 只拷一个文件（不要往网络盘拷成千上万个小文件，资源管理器容易卡死）；
- 分块写、限速（默认每秒 20 MB），给网络盘和资源管理器留余地；
- 先写成「文件名.part」，写完从网络盘读回来算 SHA-256，和源文件一致才改成正式文件名；
- 同时写一个「文件名.sha256」（sha256sum -c 能直接用），服务器上可以再核对一次；
- 目标文件夹不存在就新建，但不会覆盖已经存在的同名文件（除非加 --overwrite）。
"""
import argparse
import hashlib
import os
import sys
import time

CHUNK = 4 * 1024 * 1024


def sha256_of(path, mbps=0):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        t0, done = time.time(), 0
        while True:
            b = f.read(CHUNK)
            if not b:
                break
            h.update(b)
            done += len(b)
            if mbps:
                ahead = done / (mbps * 1e6) - (time.time() - t0)
                if ahead > 0:
                    time.sleep(ahead)
    return h.hexdigest()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("src")
    ap.add_argument("dst_dir")
    ap.add_argument("--mbps", type=float, default=20, help="限速，MB/s")
    ap.add_argument("--overwrite", action="store_true")
    a = ap.parse_args()

    name = os.path.basename(a.src)
    dst = os.path.join(a.dst_dir, name)
    part = dst + ".part"
    if os.path.exists(dst) and not a.overwrite:
        sys.exit("目标已经有 %s，不覆盖（要覆盖加 --overwrite）" % dst)
    os.makedirs(a.dst_dir, exist_ok=True)
    size = os.path.getsize(a.src)
    print("源文件 %s，%.1f MB，先算 SHA-256……" % (name, size / 1e6), flush=True)
    want = sha256_of(a.src)

    t0, done, last = time.time(), 0, 0
    with open(a.src, "rb") as fi, open(part, "wb") as fo:
        while True:
            b = fi.read(CHUNK)
            if not b:
                break
            fo.write(b)
            done += len(b)
            ahead = done / (a.mbps * 1e6) - (time.time() - t0)
            if ahead > 0:
                time.sleep(ahead)
            if time.time() - last > 10:
                last = time.time()
                el = last - t0
                print("  已写 %.0f / %.0f MB（%.1f MB/s）" % (done / 1e6, size / 1e6, done / 1e6 / max(el, 0.01)), flush=True)
        fo.flush()
        os.fsync(fo.fileno())
    print("写完，用时 %.0f 秒；从目标读回来核对……" % (time.time() - t0), flush=True)
    got = sha256_of(part, a.mbps)
    if got != want:
        os.remove(part)
        sys.exit("核对失败（%s ≠ %s），已删除半成品，重新运行一次" % (got, want))
    if os.path.exists(dst):
        os.remove(dst)
    os.replace(part, dst)
    with open(dst + ".sha256", "w", encoding="utf-8", newline="\n") as f:
        f.write("%s  %s\n" % (want, name))
    print("完成：%s（SHA-256 %s）" % (dst, want), flush=True)


if __name__ == "__main__":
    main()
