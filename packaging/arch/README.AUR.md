# Arch Packaging Notes

This directory keeps the project AUR-ready without pretending an AUR package is
already published.

## Local build from this repository

From a checked-out `g-helper-linux` tree:

```bash
cd packaging/arch
makepkg -si
```

The bundled `PKGBUILD` auto-detects the repository root two levels above and
builds from that local checkout.

## Future AUR publishing

When `v0.3.1` is pushed upstream, this packaging can be moved into a dedicated
AUR repository with:

- `PKGBUILD`
- `.SRCINFO`
- `rog-helper.install`

Suggested publish flow:

1. Copy the Arch packaging files into the AUR repository root.
2. Regenerate `.SRCINFO` with:

   ```bash
   ROG_HELPER_ARCH_FORCE_REMOTE=1 \
   ROG_HELPER_ARCH_SOURCE_REPO="https://github.com/UdayaSri0/g-helper-linux.git" \
  ROG_HELPER_ARCH_SOURCE_REF="#tag=v0.3.1" \
   makepkg --printsrcinfo > .SRCINFO
   ```

3. Commit the refreshed `PKGBUILD`, `.SRCINFO`, and `rog-helper.install`.
4. Push to the AUR repository.

The current `PKGBUILD` defaults to the tagged upstream Git source when it is no
longer being run from inside this repository checkout.

The committed `.SRCINFO` in this repository is generated for that future
upstream tag-based source and should be refreshed whenever `pkgver`, source
location, or dependencies change.
