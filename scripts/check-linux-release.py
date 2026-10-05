#!/usr/bin/env python3
"""Reject Linux x86_64 release binaries tied to the builder's environment."""
import os
from pathlib import Path
import re
import subprocess
import sys


def check(binary):
    env = {**os.environ, "LC_ALL": "C"}
    result = subprocess.run(
        ["readelf", "--wide", "--file-header", "--program-headers",
         "--dynamic", "--version-info", str(binary)],
        env=env, capture_output=True, text=True, check=True,
    )
    elf = result.stdout
    if not re.search(r"Machine:\s+Advanced Micro Devices X86-64", elf):
        raise ValueError("expected a Linux x86_64 ELF executable")
    interpreter = re.search(r"Requesting program interpreter: ([^\]]+)\]", elf)
    if not interpreter or interpreter[1] != "/lib64/ld-linux-x86-64.so.2":
        actual = interpreter[1] if interpreter else "missing"
        raise ValueError(f"nonportable interpreter: {actual}")
    for tag, value in re.findall(r"\((NEEDED|RPATH|RUNPATH)\)[^\n]*\[([^\]]*)\]", elf):
        if tag == "NEEDED" and "/" in value:
            raise ValueError(f"absolute shared library path: {value}")
        if tag in ("RPATH", "RUNPATH") and value:
            raise ValueError(f"release must use system library paths, found {tag}: {value}")
    # Ubuntu 22.04 is the release build baseline. Changing only PT_INTERP on
    # a Nix build can still leave it requiring newer glibc/libstdc++ symbols.
    for prefix, maximum in (("GLIBC", (2, 35)), ("GLIBCXX", (3, 4, 29))):
        for name in re.findall(rf"Name: ({prefix}_[0-9.]+)", elf):
            version = tuple(map(int, name.split("_", 1)[1].split(".")))
            if version > maximum:
                raise ValueError(f"{name} exceeds the Linux release baseline")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit("usage: check-linux-release.py <binary>")
    try:
        check(Path(sys.argv[1]))
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        sys.exit(f"Linux release rejected: {error}")
    print("Linux release OK: system loader, system libraries, glibc <= 2.35")
