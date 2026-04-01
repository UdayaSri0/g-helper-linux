#!/usr/bin/env python3

from __future__ import annotations

import argparse
from pathlib import Path

from PIL import Image

ICON_NAME = "rog-helper"
ICON_SIZES = (16, 24, 32, 48, 64, 128, 256, 512)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate hicolor PNG icon sizes from assets/logo.png."
    )
    parser.add_argument(
        "--source",
        type=Path,
        default=Path("assets/logo.png"),
        help="Master source image to resize.",
    )
    parser.add_argument(
        "--output-root",
        type=Path,
        default=Path("packaging/desktop/icons/hicolor"),
        help="Root hicolor directory to populate.",
    )
    return parser.parse_args()


def render_icon_set(source: Path, output_root: Path) -> None:
    if not source.is_file():
        raise SystemExit(f"missing source image: {source}")

    image = Image.open(source).convert("RGBA")
    resampling = getattr(Image, "Resampling", Image).LANCZOS

    for size in ICON_SIZES:
        target_dir = output_root / f"{size}x{size}" / "apps"
        target_dir.mkdir(parents=True, exist_ok=True)
        target_path = target_dir / f"{ICON_NAME}.png"
        resized = image.resize((size, size), resample=resampling)
        resized.save(target_path, format="PNG", optimize=True)
        print(f"wrote {target_path}")


def main() -> None:
    args = parse_args()
    render_icon_set(args.source, args.output_root)


if __name__ == "__main__":
    main()
