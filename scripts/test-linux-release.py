#!/usr/bin/env python3
"""Linux release regression tests. Needs gcc and readelf; builds no Rust code."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
CHECK = ROOT / "scripts/check-linux-release.py"


class LinuxReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.work = Path(self.temp.name)
        self.addCleanup(self.temp.cleanup)

    def binary(self, interpreter="/lib64/ld-linux-x86-64.so.2", rpath=None):
        source = self.work / "main.c"
        source.write_text('int main(void) { return 0; }\n')
        binary = self.work / "terminus"
        env = os.environ.copy()
        # The Nix shell adds an explicit build-directory RPATH through these
        # flags. Fixtures control their own loader/search paths for each case.
        for key in list(env):
            if key.startswith(("NIX_LDFLAGS", "NIX_CFLAGS_LINK")):
                env.pop(key)
        env["NIX_DONT_SET_RPATH_x86_64_unknown_linux_gnu"] = "1"
        args = ["gcc", str(source), "-o", str(binary),
                f"-Wl,--dynamic-linker={interpreter}"]
        if rpath is not None:
            args.append(f"-Wl,-rpath,{rpath}")
        subprocess.run(args, env=env, check=True, capture_output=True)
        return binary

    def check_binary(self, binary):
        return subprocess.run(["python3", str(CHECK), str(binary)],
                              capture_output=True, text=True)

    def test_accepts_system_loader(self):
        result = self.check_binary(self.binary())
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_rejects_nix_loader(self):
        result = self.check_binary(self.binary("/nix/store/test-glibc/lib/ld-linux-x86-64.so.2"))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("interpreter", result.stderr)

    def test_rejects_nix_and_build_directory_rpaths(self):
        for rpath in ("/nix/store/test-fontconfig/lib", "/tmp/build/outputs/out/lib"):
            with self.subTest(rpath=rpath):
                result = self.check_binary(self.binary(rpath=rpath))
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("path", result.stderr)

    def test_rejects_newer_glibc_even_with_system_loader(self):
        binary = self.binary()
        content = binary.read_bytes()
        self.assertIn(b"GLIBC_2.34", content)
        binary.write_bytes(content.replace(b"GLIBC_2.34", b"GLIBC_2.99"))
        result = self.check_binary(binary)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("GLIBC_2.99", result.stderr)

    def test_rejects_non_elf(self):
        binary = self.work / "terminus"
        binary.write_text("#!/bin/sh\nexit 0\n")
        self.assertNotEqual(self.check_binary(binary).returncode, 0)

    def release_tree(self, binary):
        root = self.work / "repo"
        (root / "scripts").mkdir(parents=True)
        for name in ("release.sh", "build-linux-release.sh", "check-linux-release.py"):
            if (ROOT / "scripts" / name).exists():
                shutil.copy(ROOT / "scripts" / name, root / "scripts" / name)
        (root / "scripts/sign-release.sh").write_text("#!/bin/sh\nexit 0\n")
        (root / "Cargo.toml").write_text('[workspace.package]\nversion = "1.2.3"\n')
        shutil.copy(ROOT / "rust-toolchain.toml", root)
        (root / "misc").mkdir()
        shutil.copy(ROOT / "misc/nfpm-terminus.yaml", root / "misc")
        tools = root / "tools"
        tools.mkdir()
        shutil.copy(binary, root / "fixture")
        # Leave a stale host build to catch accidentally packaging target/release.
        (root / "target/release").mkdir(parents=True)
        (root / "target/release/terminus").write_text("stale Nix development binary")
        (root / "target/release/terminus").chmod(0o755)
        build = '''#!/bin/sh
mkdir -p target/linux-release/x86_64-unknown-linux-gnu/release
cp fixture target/linux-release/x86_64-unknown-linux-gnu/release/terminus
'''
        (tools / "cargo").write_text("#!/bin/sh\necho 'host cargo must not run' >&2\nexit 98\n")
        (tools / "docker").write_text(build)
        (tools / "nfpm").write_text('''#!/bin/sh
set -eu
binary=$(sed -n 's/^  - src: \\(.*terminus\\)$/\\1/p' misc/nfpm-terminus.yaml | head -1)
test -x "$binary"
cp "$binary" "dist/packaged-$3"
''')
        for tool in tools.iterdir():
            tool.chmod(0o755)
        return root, {**os.environ, "PATH": f"{tools}:{os.environ['PATH']}",
                      "GH_REPO": "test/terminus", "GITHUB_REMOTE_URL": "test"}

    def test_release_stops_before_packaging_contaminated_binary(self):
        root, env = self.release_tree(self.binary("/nix/store/test-glibc/lib/ld-linux-x86-64.so.2"))
        result = subprocess.run(["bash", str(root / "scripts/release.sh"),
                                 "--build-only", "--linux-only"], env=env,
                                capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertFalse(list((root / "dist").glob("packaged-*")))
        self.assertFalse((root / "dist/terminus-linux-x86_64.tar.gz").exists())

    def test_tar_deb_and_rpm_use_same_validated_binary(self):
        binary = self.binary()
        expected = binary.read_bytes()
        root, env = self.release_tree(binary)
        result = subprocess.run(["bash", str(root / "scripts/release.sh"),
                                 "--build-only", "--linux-only"], env=env,
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        import tarfile
        with tarfile.open(root / "dist/terminus-linux-x86_64.tar.gz") as archive:
            self.assertEqual(archive.extractfile("terminus/terminus").read(), expected)
        for kind in ("deb", "rpm"):
            self.assertEqual((root / f"dist/packaged-{kind}").read_bytes(), expected)
        self.assertEqual((root / "dist/linux/terminus").read_bytes(), expected)


if __name__ == "__main__":
    unittest.main()
