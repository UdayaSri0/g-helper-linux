#!/usr/bin/env python3
"""Fail-closed inventory, checksum, package-metadata, and provenance check."""

from __future__ import annotations

import argparse
import hashlib
import re
import shutil
import subprocess
import sys
import tarfile
import tomllib
from pathlib import Path
from typing import Callable


VERSION = tomllib.loads((Path(__file__).resolve().parents[2] / "Cargo.toml").read_text())["workspace"]["package"]["version"]
TAG = f"v{VERSION}"
PRIMARY_MANIFEST = f"rog-helper-{VERSION}-SHA256SUMS.txt"
RPM_MANIFEST = f"rog-helper-{VERSION}-RPM-SHA256SUMS.txt"
LINUX_HASHED_FILES = (
    f"rog-helper_{VERSION}_amd64.deb",
    f"rog-helper-v{VERSION}-x86_64.AppImage",
    f"rog-helper-{VERSION}-linux-x86_64.tar.xz",
    "rog-helper",
    "rog-helperd",
    "rog-helper-ui",
)
RPM_FILE = f"rog-helper-{VERSION}-1.x86_64.rpm"
LINUX_FILES = frozenset((*LINUX_HASHED_FILES, PRIMARY_MANIFEST))
RPM_FILES = frozenset((RPM_FILE, RPM_MANIFEST))
EXPECTED_FILES = frozenset((*LINUX_FILES, *RPM_FILES))
CHECKSUM_LINE = re.compile(r"([0-9a-fA-F]{64})  ([A-Za-z0-9._+-]+)")
SHA = re.compile(r"[0-9a-fA-F]{40}")


class VerificationError(ValueError):
    pass


def _collect(directory: Path, expected: frozenset[str], label: str) -> dict[str, Path]:
    if directory.is_symlink() or not directory.is_dir():
        raise VerificationError(f"{label} input is not a real directory: {directory}")

    files: dict[str, Path] = {}
    for entry in directory.iterdir():
        if entry.is_symlink():
            raise VerificationError(f"{label} input contains a symlink: {entry.name}")
        if not entry.is_file():
            raise VerificationError(f"{label} input contains a nested or non-regular path: {entry.name}")
        if entry.name in files:
            raise VerificationError(f"duplicate {label} basename: {entry.name}")
        if entry.name not in expected:
            raise VerificationError(f"unexpected {label} file: {entry.name}")
        if entry.stat().st_size == 0:
            raise VerificationError(f"empty {label} file: {entry.name}")
        files[entry.name] = entry

    missing = expected - files.keys()
    if missing:
        raise VerificationError(f"missing {label} file(s): {', '.join(sorted(missing))}")
    return files


def _verify_manifest(manifest: Path, expected: tuple[str, ...], files: dict[str, Path]) -> None:
    try:
        lines = manifest.read_text(encoding="ascii").splitlines()
    except (OSError, UnicodeError) as exc:
        raise VerificationError(f"cannot read checksum manifest {manifest.name}: {exc}") from exc

    seen: set[str] = set()
    for line_number, line in enumerate(lines, 1):
        match = CHECKSUM_LINE.fullmatch(line)
        if match is None:
            raise VerificationError(f"malformed checksum entry in {manifest.name}:{line_number}")
        digest, filename = match.groups()
        if filename not in expected:
            raise VerificationError(f"unexpected checksum entry in {manifest.name}: {filename}")
        if filename in seen:
            raise VerificationError(f"duplicate checksum entry in {manifest.name}: {filename}")
        seen.add(filename)
        actual = hashlib.sha256(files[filename].read_bytes()).hexdigest()
        if digest.lower() != actual:
            raise VerificationError(f"checksum mismatch for {filename}")

    if seen != set(expected):
        missing = set(expected) - seen
        raise VerificationError(f"missing checksum entry in {manifest.name}: {', '.join(sorted(missing))}")


def _run(command: list[str], runner: Callable = subprocess.run) -> str:
    try:
        result = runner(command, check=True, capture_output=True, text=True)
    except (OSError, subprocess.SubprocessError) as exc:
        raise VerificationError(f"metadata command failed ({command[0]}): {exc}") from exc
    return result.stdout


def _verify_package_metadata(files: dict[str, Path], runner: Callable) -> None:
    deb_fields: dict[str, str] = {}
    for line in _run(["dpkg-deb", "-f", str(files[f"rog-helper_{VERSION}_amd64.deb"])], runner).splitlines():
        if line and not line[0].isspace() and ":" in line:
            key, value = line.split(":", 1)
            deb_fields[key.lower()] = value.strip()
    for field, expected in (("package", "rog-helper"), ("version", VERSION), ("architecture", "amd64")):
        if deb_fields.get(field) != expected:
            raise VerificationError(f"DEB {field} must be {expected}, got {deb_fields.get(field)!r}")

    rpm_fields = _run(
        ["rpm", "-qp", "--qf", "%{NAME}\n%{VERSION}\n%{RELEASE}\n%{ARCH}\n", str(files[RPM_FILE])],
        runner,
    ).splitlines()
    if rpm_fields != ["rog-helper", VERSION, "1", "x86_64"]:
        raise VerificationError(f"RPM metadata must be rog-helper {VERSION}-1 x86_64, got {rpm_fields!r}")

    try:
        with tarfile.open(files[f"rog-helper-{VERSION}-linux-x86_64.tar.xz"], mode="r:xz") as archive:
            for member in archive.getmembers():
                path = Path(member.name)
                if path.is_absolute() or ".." in path.parts or member.issym() or member.islnk() or not (member.isfile() or member.isdir()):
                    raise VerificationError(f"tarball contains an unsafe path or link: {member.name}")
    except (OSError, tarfile.TarError) as exc:
        raise VerificationError(f"cannot safely inspect release tarball: {exc}") from exc


def verify_release_assets(
    linux_directory: Path,
    rpm_directory: Path,
    output_directory: Path,
    provenance_file: Path,
    *,
    tag: str,
    commit: str,
    run_id: str,
    runner: Callable = subprocess.run,
) -> dict[str, str]:
    if tag != TAG:
        raise VerificationError(f"expected tag {TAG}, got {tag!r}")
    if not SHA.fullmatch(commit):
        raise VerificationError(f"invalid commit SHA: {commit!r}")
    if not re.fullmatch(r"[1-9][0-9]*", run_id):
        raise VerificationError(f"invalid workflow run ID: {run_id!r}")

    linux_directory = Path(linux_directory)
    rpm_directory = Path(rpm_directory)
    output_directory = Path(output_directory)
    provenance_file = Path(provenance_file)
    linux_files = _collect(linux_directory, LINUX_FILES, "Linux")
    rpm_files = _collect(rpm_directory, RPM_FILES, "RPM")
    overlap = linux_files.keys() & rpm_files.keys()
    if overlap:
        raise VerificationError(f"duplicate basenames across build artifacts: {', '.join(sorted(overlap))}")
    files = {**linux_files, **rpm_files}
    if files.keys() != EXPECTED_FILES:
        raise VerificationError("downloaded files do not match the exact nine-file release inventory")

    _verify_manifest(linux_files[PRIMARY_MANIFEST], LINUX_HASHED_FILES, linux_files)
    _verify_manifest(rpm_files[RPM_MANIFEST], (RPM_FILE,), rpm_files)
    _verify_package_metadata(files, runner)

    output_resolved = output_directory.resolve()
    provenance_resolved = provenance_file.resolve()
    if provenance_resolved == output_resolved or output_resolved in provenance_resolved.parents:
        raise VerificationError("provenance summary must be outside the nine-file bundle")
    if output_directory.exists():
        raise VerificationError(f"bundle output already exists: {output_directory}")
    if any(source.resolve() == output_resolved or source.resolve() in output_resolved.parents for source in (linux_directory, rpm_directory)):
        raise VerificationError("bundle output must not be inside a downloaded artifact directory")

    output_directory.mkdir(parents=True)
    hashes: dict[str, str] = {}
    for name in sorted(files):
        destination = output_directory / name
        shutil.copyfile(files[name], destination)
        hashes[name] = hashlib.sha256(destination.read_bytes()).hexdigest()

    summary = [
        f"ROG Helper {tag} verified release candidate",
        f"tag: {tag}",
        f"commit: {commit}",
        f"workflow run: {run_id}",
        "verified file hashes (SHA-256):",
        *(f"{digest}  {name}" for name, digest in hashes.items()),
    ]
    provenance_file.parent.mkdir(parents=True, exist_ok=True)
    provenance_file.write_text("\n".join(summary) + "\n", encoding="utf-8")
    return hashes


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--linux-dir", type=Path, required=True)
    parser.add_argument("--rpm-dir", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--provenance", type=Path, required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--run-id", required=True)
    args = parser.parse_args()
    try:
        hashes = verify_release_assets(
            args.linux_dir,
            args.rpm_dir,
            args.output_dir,
            args.provenance,
            tag=args.tag,
            commit=args.commit,
            run_id=args.run_id,
        )
    except VerificationError as exc:
        print(f"release asset verification failed: {exc}", file=sys.stderr)
        return 1
    print(f"verified {len(hashes)} release files for {args.tag}")
    print(f"provenance: {args.provenance}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
