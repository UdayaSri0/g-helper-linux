#!/usr/bin/env python3
"""Test RPM/Arch upgrade hooks and release checksums without running host services."""

from pathlib import Path
import os
import shlex
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class PackageIntegration(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="rog-helper-package-test-")
        self.root = Path(self.directory.name)
        self.log = self.root / "calls"
        self.env = dict(os.environ, ROG_HELPER_TEST_LOG=str(self.log))
        self.mock = self.root / "systemctl"
        self.mock.write_text('#!/bin/sh\nprintf "%s\\n" "$*" >> "$ROG_HELPER_TEST_LOG"\n')
        self.mock.chmod(0o755)
        (self.root / "udevadm").symlink_to(self.mock)
        self.env["PATH"] = f"{self.root}:{os.environ['PATH']}"

    def tearDown(self):
        self.directory.cleanup()

    def calls(self):
        return self.log.read_text() if self.log.exists() else ""

    def test_arch_upgrade_restarts_only_an_already_active_helper(self):
        script = shlex.quote(str(ROOT / "packaging/arch/rog-helper.install"))
        subprocess.run(["bash", "-c", f"source {script}; post_upgrade"], env=self.env, check=True, capture_output=True)
        self.assertIn("try-restart rog-helper-privileged.service", self.calls())
        self.assertNotIn("start rog-helper-privileged.service\n", self.calls().replace("try-restart", ""))

    def test_rpm_fresh_install_does_not_start_helper_but_upgrade_refreshes_it(self):
        spec = (ROOT / "packaging/rpm/rog-helper.spec.in").read_text()
        post = spec.split("\n%post\n", 1)[1].split("\n%preun\n", 1)[0]
        post = post.replace("/bin/systemctl", shlex.quote(str(self.mock))).replace("/bin/udevadm", shlex.quote(str(self.root / "udevadm")))
        subprocess.run(["sh", "-c", post, "rpm-post", "1"], env=self.env, check=True)
        self.assertNotIn("try-restart", self.calls())
        subprocess.run(["sh", "-c", post, "rpm-post", "2"], env=self.env, check=True)
        self.assertIn("try-restart rog-helper-privileged.service", self.calls())

    def test_checksums_include_all_native_assets_and_skip_unrelated_versions(self):
        import tomllib
        version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
        names = ["rog-helper", f"rog-helper_{version}_amd64.deb", f"rog-helper-{version}-1.x86_64.rpm", f"rog-helper-v{version}-x86_64.AppImage", f"rog-helper-{version}-linux-x86_64.tar.xz"]
        for name in names + ["rog-helper-0.0.0-1.x86_64.rpm"]:
            (self.root / name).touch()
        common = shlex.quote(str(ROOT / "packaging/scripts/package-common.sh"))
        subprocess.run(["bash", "-c", f"source {common}; write_sha256sums {shlex.quote(str(self.root))}"], check=True)
        manifest = (self.root / f"rog-helper-{version}-SHA256SUMS.txt").read_text()
        for name in names:
            self.assertIn(name, manifest)
        self.assertNotIn("0.0.0", manifest)
        subprocess.run(["sha256sum", "--check", f"rog-helper-{version}-SHA256SUMS.txt"], cwd=self.root, check=True, capture_output=True)


if __name__ == "__main__":
    unittest.main()
