# -*- coding: utf-8 -*-
"""往网络盘（比如 sshfs 挂载的 Q 盘）安全地拷一个大文件，或者从网络盘拷回来。

用法：python scripts/voice-train/safe_copy.py 源文件 目标文件夹 [--mbps 0.25]
- 只拷一个文件（不要往网络盘拷成千上万个小文件，资源管理器容易卡死）；
- 分块写、限速（默认每秒 0.25 MB，比用户的 Q 盘实测上限 0.3~0.4 MB/s 低一点），给网络盘和资源管理器留余地；
- 先写成「文件名.part」，写完从网络盘读回来算 SHA-256，和源文件一致才改成正式文件名；
- 中途断了（.part 还在）再运行一次会续传：先读回已经写过去的部分逐块核对，从第一块不对的地方接着写；
- 同时写一个「文件名.sha256」（sha256sum -c 能直接用），服务器上可以再核对一次；
- 目标文件夹不存在就新建，但不会覆盖已经存在的同名文件（除非加 --overwrite）；
- 卡住检测：超过 --stall 秒没有进展就打印一行「卡住了」。sshfs 的连接卡死时读写会停在系统内核里，
  不报错、不退出、连强制结束都杀不掉（实测），只能靠这行提示发现；重连 Q 盘后再运行一次会续传。
"""
import argparse
import hashlib
import os
import sys
import threading
import time

CHUNK = 1024 * 1024
STALL_MARK = "卡住了"
_last_progress = [time.time()]


def tick():
    _last_progress[0] = time.time()


def watchdog(stall):
    """后台线程：主线程卡在网络盘的读写里时照样能打印提示"""
    warned = 0.0
    while True:
        time.sleep(5)
        idle = time.time() - _last_progress[0]
        if idle > stall and time.time() - warned > 600:
            warned = time.time()
            print(
                "  %s：已经 %.0f 秒没有进展，网络盘的连接可能断了。重连 Q 盘后再运行一次，会从断的地方续传。" % (STALL_MARK, idle),
                flush=True,
            )


def pace(done, mbps, t0):
    """限速：比 mbps 快就睡一会儿"""
    tick()
    ahead = done / (mbps * 1e6) - (time.time() - t0)
    if ahead > 0:
        time.sleep(ahead)


def sha256_of(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while True:
            b = f.read(CHUNK)
            if not b:
                break
            h.update(b)
    return h.hexdigest()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("src")
    ap.add_argument("dst_dir")
    ap.add_argument("--mbps", type=float, default=0.25, help="限速，MB/s")
    ap.add_argument("--stall", type=float, default=180, help="多少秒没有进展算卡住")
    ap.add_argument("--overwrite", action="store_true")
    ap.add_argument("--restart", action="store_true", help="不续传，从头重新写")
    ap.add_argument("--trust-part", action="store_true", help="续传时不读回已写部分（上次已核对过；整个文件由服务器上的 sha256sum -c 核对）")
    a = ap.parse_args()
    threading.Thread(target=watchdog, args=(a.stall,), daemon=True).start()

    name = os.path.basename(a.src)
    dst = os.path.join(a.dst_dir, name)
    part = dst + ".part"
    if os.path.exists(dst) and not a.overwrite:
        sys.exit("目标已经有 %s，不覆盖（要覆盖加 --overwrite）" % dst)
    os.makedirs(a.dst_dir, exist_ok=True)
    size = os.path.getsize(a.src)
    print("源文件 %s，%.1f MB，先算 SHA-256……" % (name, size / 1e6), flush=True)
    want = sha256_of(a.src)
    tick()

    # 上次没传完（.part 还在）就续传：先把已经写过去的部分读回来逐块和源文件比，从第一块不一样的地方接着写
    start = 0
    h = hashlib.sha256()  # 目标上实际内容的哈希：续传时 = 读回来核对过的前段 + 写完后读回来的后段
    if os.path.exists(part) and not a.restart:
        have = min(os.path.getsize(part) // CHUNK * CHUNK, size)
        if a.trust_part:
            # 已写部分上次核对过、不再从慢速网络盘读回；哈希用本地源文件的这一段，整个文件由服务器上的 sha256sum -c 最终核对
            print("发现上次没传完的 %s，不读回核对，从 %.0f MB 接着传" % (os.path.basename(part), have / 1e6), flush=True)
            with open(a.src, "rb") as fi:
                while start < have:
                    b = fi.read(min(CHUNK, have - start))
                    h.update(b)
                    start += len(b)
        else:
            print("发现上次没传完的 %s（%.0f MB），先读回来核对……" % (os.path.basename(part), have / 1e6), flush=True)
            t0, last = time.time(), time.time()
            with open(a.src, "rb") as fi, open(part, "rb") as fp:
                while start < have:
                    b = fp.read(CHUNK)
                    if not b or b != fi.read(len(b)):
                        break
                    h.update(b)
                    start += len(b)
                    pace(start, a.mbps, t0)
                    if time.time() - last > 10:
                        last = time.time()
                        print("  已核对 %.0f / %.0f MB" % (start / 1e6, have / 1e6), flush=True)
            print("  前 %.0f MB 没问题，从这里接着传" % (start / 1e6), flush=True)

    t0, done, last = time.time(), 0, 0
    with open(a.src, "rb") as fi, open(part, "r+b" if start else "wb") as fo:
        fi.seek(start)
        fo.truncate(start)
        fo.seek(start)
        tick()
        while True:
            b = fi.read(CHUNK)
            if not b:
                break
            fo.write(b)
            done += len(b)
            pace(done, a.mbps, t0)
            if time.time() - last > 10:
                last = time.time()
                el = last - t0
                print("  已写 %.0f / %.0f MB（%.2f MB/s）" % ((start + done) / 1e6, size / 1e6, done / 1e6 / max(el, 0.01)), flush=True)
        fo.flush()
        os.fsync(fo.fileno())
    print("写完，用时 %.0f 秒；从目标读回来核对……" % (time.time() - t0), flush=True)
    t0, n = time.time(), 0
    with open(part, "rb") as fp:
        fp.seek(start)
        while True:
            b = fp.read(CHUNK)
            if not b:
                break
            h.update(b)
            n += len(b)
            pace(n, a.mbps, t0)
    got = h.hexdigest()
    if got != want:
        os.remove(part)
        sys.exit("核对失败（%s ≠ %s），已删除半成品，重新运行一次" % (got, want))
    if os.path.exists(dst):
        os.remove(dst)
    os.replace(part, dst)
    with open(dst + ".sha256", "w", encoding="utf-8", newline="\n") as f:
        f.write("%s  %s\n" % (want, name))
    print("完成：%s（SHA-256 %s）" % (dst, want), flush=True)
    if a.trust_part:
        print("注意：续传前已经写过去的部分没有读回核对，用之前在服务器上 sha256sum -c 核对一次。", flush=True)


if __name__ == "__main__":
    main()
