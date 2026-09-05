#!/usr/bin/env python3
"""Generate Terminus PNG/ICO placeholders without extra Python deps."""

from __future__ import annotations

import struct
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1] / "src-tauri" / "icons"


def png(size: int) -> bytes:
    raw = bytearray()
    for y in range(size):
        raw.append(0)
        for x in range(size):
            nx = x / (size - 1)
            ny = y / (size - 1)
            inside = 0.12 < nx < 0.88 and 0.16 < ny < 0.84
            chevron = abs((nx - 0.42) - 0.22 * (0.5 - abs(ny - 0.5) * 2)) < 0.06 and 0.28 < ny < 0.72
            if chevron:
                r, g, b, a = 62, 224, 138, 255
            elif inside:
                r, g, b, a = 13, 22, 18, 255
            else:
                r, g, b, a = 0, 0, 0, 0
            raw.extend((r, g, b, a))

    def chunk(tag: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(bytes(raw), 9)) + chunk(b"IEND", b"")


def ico(images: list[tuple[int, bytes]]) -> bytes:
    header = struct.pack("<HHH", 0, 1, len(images))
    entries = b""
    payload = b""
    offset = 6 + 16 * len(images)
    for size, data in images:
        entries += struct.pack("<BBBBHHII", size if size < 256 else 0, size if size < 256 else 0, 0, 0, 1, 32, len(data), offset)
        payload += data
        offset += len(data)
    return header + entries + payload


def icns(png256: bytes) -> bytes:
    # ic09 is a PNG-compressed 256px icon accepted by modern macOS.
    body = b"ic09" + struct.pack(">I", len(png256) + 8) + png256
    return b"icns" + struct.pack(">I", len(body) + 8) + body


def main() -> None:
    ROOT.mkdir(parents=True, exist_ok=True)
    p32 = png(32)
    p128 = png(128)
    p256 = png(256)
    (ROOT / "32x32.png").write_bytes(p32)
    (ROOT / "128x128.png").write_bytes(p128)
    (ROOT / "128x128@2x.png").write_bytes(p256)
    (ROOT / "icon.ico").write_bytes(ico([(32, p32), (128, p128), (256, p256)]))
    (ROOT / "icon.icns").write_bytes(icns(p256))
    print(f"wrote icons in {ROOT}")


if __name__ == "__main__":
    main()
