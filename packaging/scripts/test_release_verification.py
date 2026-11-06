#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import importlib.util
import io
import tarfile
import tempfile
import unittest
from pathlib import Path
from subprocess import CompletedProcess


SCRIPT = Path(__file__).with_name("verify-release-assets.py")
SPEC = importlib.util.spec_from_file_location("release_verification", SCRIPT)
release = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(release)

VERSION = release.VERSION
LINUX_HASHED = (
    f"rog-helper_{VERSION}_amd64.deb",
    f"rog-helper-v{VERSION}-x86_64.AppImage",
    f"rog-helper-{VERSION}-linux-x86_64.tar.xz",
    "rog-helper",
    "rog-helperd",
    "rog-helper-ui",
)
RPM_NAME = f"rog-helper-{VERSION}-1.x86_64.rpm"


def make_inputs(root: Path) -> tuple[Path, Path]:
    linux = root / "linux"
    rpm = root / "rpm"
    linux.mkdir()
    rpm.mkdir()
    for name in LINUX_HASHED:
        if name.endswith(".tar.xz"):
            with tarfile.open(linux / name, "w:xz") as archive:
                data = b"release payload"
                item = tarfile.TarInfo("rog-helper/bin/rog-helper")
                item.size = len(data)
                archive.addfile(item, io.BytesIO(data))
        else:
            (linux / name).write_bytes(("linux:" + name).encode())
    (rpm / RPM_NAME).write_bytes(b"rpm-payload")
    write_manifest(linux, f"rog-helper-{VERSION}-SHA256SUMS.txt", LINUX_HASHED)
    write_manifest(rpm, f"rog-helper-{VERSION}-RPM-SHA256SUMS.txt", (RPM_NAME,))
    return linux, rpm


def write_manifest(root: Path, manifest: str, filenames: tuple[str, ...]) -> None:
    lines = [f"{hashlib.sha256((root / name).read_bytes()).hexdigest()}  {name}" for name in filenames]
    (root / manifest).write_text("\n".join(lines) + "\n", encoding="ascii")


def metadata_runner(deb_version: str = VERSION, rpm_version: str = VERSION):
    def run(args, **kwargs):
        if args[0] == "dpkg-deb":
            output = f"Package: rog-helper\nVersion: {deb_version}\nArchitecture: amd64\n"
        else:
            output = f"rog-helper\n{rpm_version}\n1\nx86_64\n"
        return CompletedProcess(args, 0, stdout=output, stderr="")

    return run


class ReleaseVerificationTests(unittest.TestCase):
    def invoke(self, root: Path, linux: Path, rpm: Path, runner=None):
        return release.verify_release_assets(
            linux,
            rpm,
            root / "bundle",
            root / "provenance.txt",
            tag=f"v{VERSION}",
            commit="a" * 40,
            run_id="123456",
            runner=runner or metadata_runner(),
        )

    def test_accepts_exact_inventory_and_writes_hash_provenance(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            linux, rpm = make_inputs(root)
            result = self.invoke(root, linux, rpm)
            self.assertEqual(set(result), set(LINUX_HASHED) | {RPM_NAME} | {
                f"rog-helper-{VERSION}-SHA256SUMS.txt",
                f"rog-helper-{VERSION}-RPM-SHA256SUMS.txt",
            })
            self.assertEqual(len(list((root / "bundle").iterdir())), 9)
            self.assertNotIn("rog-helper-privileged", " ".join(result))
            provenance = (root / "provenance.txt").read_text(encoding="utf-8")
            self.assertIn(f"tag: v{VERSION}", provenance)
            self.assertIn("commit: " + "a" * 40, provenance)
            self.assertIn("workflow run: 123456", provenance)
            self.assertIn(hashlib.sha256((root / "bundle" / RPM_NAME).read_bytes()).hexdigest(), provenance)

    def test_rejects_missing_unexpected_and_empty_assets(self):
        for mutation in ("missing", "unexpected", "empty"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                linux, rpm = make_inputs(root)
                if mutation == "missing":
                    (linux / "rog-helperd").unlink()
                elif mutation == "unexpected":
                    (linux / "rog-helper-privileged").write_bytes(b"never release this")
                else:
                    (linux / "rog-helperd").write_bytes(b"")
                with self.assertRaises(release.VerificationError):
                    self.invoke(root, linux, rpm)

    def test_rejects_symlinks_nested_paths_and_duplicate_basenames(self):
        for mutation in ("symlink", "nested", "duplicate"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                linux, rpm = make_inputs(root)
                if mutation == "symlink":
                    target = linux / "rog-helperd"
                    target.unlink()
                    target.symlink_to(linux / "rog-helper")
                elif mutation == "nested":
                    (linux / "nested").mkdir()
                    (linux / "nested" / "rog-helperd").write_bytes(b"duplicate")
                else:
                    (rpm / LINUX_HASHED[0]).write_bytes(b"duplicate basename")
                with self.assertRaises(release.VerificationError):
                    self.invoke(root, linux, rpm)

    def test_rejects_bad_primary_and_rpm_manifests(self):
        for mutation in ("malformed", "missing", "duplicate", "extra", "mismatch", "rpm-malformed"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                linux, rpm = make_inputs(root)
                primary = linux / f"rog-helper-{VERSION}-SHA256SUMS.txt"
                if mutation == "malformed":
                    primary.write_text("not a checksum entry\n", encoding="ascii")
                elif mutation == "missing":
                    primary.write_text("", encoding="ascii")
                elif mutation == "duplicate":
                    primary.write_text(primary.read_text() * 2, encoding="ascii")
                elif mutation == "extra":
                    primary.write_text(primary.read_text() + "0" * 64 + "  rog-helper-privileged\n", encoding="ascii")
                elif mutation == "mismatch":
                    primary.write_text("0" * 64 + "  " + LINUX_HASHED[0] + "\n" + "\n".join(
                        line for line in primary.read_text().splitlines() if not line.endswith(LINUX_HASHED[0])
                    ) + "\n", encoding="ascii")
                else:
                    (rpm / f"rog-helper-{VERSION}-RPM-SHA256SUMS.txt").write_text("bad\n", encoding="ascii")
                with self.assertRaises(release.VerificationError):
                    self.invoke(root, linux, rpm)

    def test_rejects_package_metadata_drift(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            linux, rpm = make_inputs(root)
            with self.assertRaisesRegex(release.VerificationError, "DEB version"):
                self.invoke(root, linux, rpm, metadata_runner(deb_version=VERSION + ".0"))

    def test_rejects_tarball_links(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            linux, rpm = make_inputs(root)
            tarball = linux / f"rog-helper-{VERSION}-linux-x86_64.tar.xz"
            with tarfile.open(tarball, "w:xz") as archive:
                item = tarfile.TarInfo("../../outside")
                item.type = tarfile.SYMTYPE
                item.linkname = "/etc/passwd"
                archive.addfile(item)
            write_manifest(linux, f"rog-helper-{VERSION}-SHA256SUMS.txt", LINUX_HASHED)
            with self.assertRaisesRegex(release.VerificationError, "unsafe path or link"):
                self.invoke(root, linux, rpm)

    def test_rejects_wrong_tag_commit_or_run_id(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            linux, rpm = make_inputs(root)
            with self.assertRaises(release.VerificationError):
                release.verify_release_assets(linux, rpm, root / "bundle", root / "provenance.txt",
                                              tag=f"v{VERSION}.0", commit="a" * 40, run_id="123456",
                                              runner=metadata_runner())


if __name__ == "__main__":
    unittest.main()
