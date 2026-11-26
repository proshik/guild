#!/usr/bin/env bash
# Downloads the standard Hugo binary for this platform into tools/hugo (git-ignored) and checks
# the release archive against the SHA-256 pinned below, taken from the release's
# hugo_<version>_checksums.txt. It builds the landing site in site/; nothing is installed
# globally, and no Node.js is involved.
set -euo pipefail
VERSION="0.170.1"
DIR="$(cd "$(dirname "$0")" && pwd)"
OUT="$DIR/hugo"
os="$(uname -s)"; arch="$(uname -m)"
case "$os-$arch" in
  Darwin-arm64|Darwin-x86_64)
    # macOS ships only as a universal installer package; it is unpacked, never installed.
    asset="hugo_${VERSION}_darwin-universal.pkg"
    sha256="f7672f5e47f13c488ecd9ae96450ed9d2044629d60fe23570312e896c1918d73" ;;
  Linux-aarch64|Linux-arm64)
    asset="hugo_${VERSION}_linux-arm64.tar.gz"
    sha256="6f57f13bc47c000c0d36c7f5a446d6c06f7956d8e102cc2a13735e0c0225b2d2" ;;
  Linux-x86_64)
    asset="hugo_${VERSION}_linux-amd64.tar.gz"
    sha256="841a5462853aefc9942e4de1e8a5d75584b6a5181941e6ad3f1d745190e03cd8" ;;
  *) echo "unsupported platform: $os-$arch" >&2; exit 1 ;;
esac
url="https://github.com/gohugoio/hugo/releases/download/v${VERSION}/${asset}"
echo "downloading $asset"
tmp="$(mktemp -d "$DIR/.hugo.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT
curl -fL -o "$tmp/$asset" "$url"
if command -v sha256sum >/dev/null 2>&1; then
  got="$(sha256sum "$tmp/$asset" | cut -d' ' -f1)"
else
  got="$(shasum -a 256 "$tmp/$asset" | cut -d' ' -f1)"
fi
if [ "$got" != "$sha256" ]; then
  echo "checksum mismatch for $asset: expected $sha256, got $got" >&2
  exit 1
fi
case "$asset" in
  *.pkg)
    pkgutil --expand-full "$tmp/$asset" "$tmp/x"
    bin="$(find "$tmp/x" -type f -name hugo -perm -u+x | head -n 1)" ;;
  *.tar.gz)
    mkdir "$tmp/x"
    tar -xzf "$tmp/$asset" -C "$tmp/x" hugo
    bin="$tmp/x/hugo" ;;
esac
[ -n "$bin" ] && [ -f "$bin" ] || { echo "hugo binary not found in $asset" >&2; exit 1; }
chmod +x "$bin"
mv "$bin" "$OUT"
"$OUT" version >/dev/null 2>&1 && echo "ready at $OUT ($("$OUT" version))" || { echo "binary check failed" >&2; exit 1; }
