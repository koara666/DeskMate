# -*- coding: utf-8 -*-
"""只读的弹丸论破 PC 版 WAD（AGAR 格式）读取器。

只读取用户自己电脑上的游戏文件，不修改它。格式：
  "AGAR" + 主版本 u32 + 次版本 u32 + 额外头长度 u32 + 额外头
  文件数 u32，每个文件：名字长度 u32 + 名字 + 大小 u64 + 偏移 u64
  目录数 u32，每个目录：名字长度 u32 + 名字 + 条目数 u32 + 条目（名字长度 u32 + 名字 + 类型 u8）
  之后是数据区，文件偏移相对数据区开头。
"""
import struct


class Wad:
    def __init__(self, path):
        self.path = path
        self.files = {}  # 名字 -> (偏移, 大小)
        with open(path, "rb") as f:
            if f.read(4) != b"AGAR":
                raise ValueError("不是 AGAR 格式的 WAD：" + path)
            _major, _minor, extra = struct.unpack("<III", f.read(12))
            f.read(extra)
            (count,) = struct.unpack("<I", f.read(4))
            for _ in range(count):
                (n,) = struct.unpack("<I", f.read(4))
                name = f.read(n).decode("utf-8", "replace")
                size, off = struct.unpack("<QQ", f.read(16))
                self.files[name] = (off, size)
            (dirs,) = struct.unpack("<I", f.read(4))
            for _ in range(dirs):
                (n,) = struct.unpack("<I", f.read(4))
                f.read(n)
                (entries,) = struct.unpack("<I", f.read(4))
                for _ in range(entries):
                    (n,) = struct.unpack("<I", f.read(4))
                    f.read(n + 1)
            self.data_start = f.tell()

    def read(self, name):
        off, size = self.files[name]
        with open(self.path, "rb") as f:
            f.seek(self.data_start + off)
            return f.read(size)
