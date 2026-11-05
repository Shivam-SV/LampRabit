#!/usr/bin/env bash
# Builds MinIO from the source of its last release. MinIO stopped publishing Community
# binaries in October 2025 and archived the project, so LampRabit builds that release
# itself: pure Go, cross-compiled on one Linux runner for every platform.
#   build-minio.sh <version> <os> <arch>  ->  out/minio-<version>-<os>-<arch>.tar.gz
export TARGET_OS="$2" TARGET_ARCH="$3"
. "$(dirname "$0")/lib.sh"
VERSION="$1"
tag_var="MINIO_TAG_${VERSION//./_}"
sum_var="MINIO_SHA256_${VERSION//./_}"
[ -n "${!tag_var:-}" ] && [ -n "${!sum_var:-}" ] || { echo "no tag or checksum for MinIO $VERSION in versions.env" >&2; exit 1; }
TAG="${!tag_var}"

log "Go $GO_VERSION"
go_tar="$(fetch "https://go.dev/dl/go$GO_VERSION.linux-amd64.tar.gz" "$GO_SHA256")"
rm -rf "$WORK/go" && tar -C "$WORK" -xzf "$go_tar"
# GOTOOLCHAIN=local: build with this Go, never one go.mod would download.
export PATH="$WORK/go/bin:$PATH" GOTOOLCHAIN=local CGO_ENABLED=0 GOFLAGS=-mod=mod

src="$(unpack "$(fetch "https://github.com/minio/minio/archive/refs/tags/$TAG.tar.gz" "${!sum_var}")")"
case "$OS" in linux) goos=linux ;; macos) goos=darwin ;; windows) goos=windows ;; esac
case "$ARCH" in x86_64) goarch=amd64 ;; aarch64) goarch=arm64 ;; esac
ext=""; [ "$OS" = windows ] && ext=".exe"

stage="$WORK/stage-minio-$VERSION-$OS-$ARCH"
rm -rf "$stage" && mkdir -p "$stage"
# RELEASE.2025-10-15T17-29-55Z -> 2025-10-15T17:29:55Z, which `minio --version` reports.
released="$(echo "$TAG" | sed -E 's/^RELEASE\.([0-9-]+)T([0-9]+)-([0-9]+)-([0-9]+)Z$/\1T\2:\3:\4Z/')"
log "MinIO $TAG ($OS $ARCH): compiling"
(cd "$src" && GOOS="$goos" GOARCH="$goarch" go build -trimpath -tags kqueue \
    -ldflags "-s -w -X github.com/minio/minio/cmd.Version=$released -X github.com/minio/minio/cmd.ReleaseTag=$TAG -X github.com/minio/minio/cmd.CopyrightYear=2025" \
    -o "$stage/minio$ext" .)
cp "$src/LICENSE" "$stage/LICENSE"
cat > "$stage/NOTICE" <<EOF
minio$ext is MinIO $TAG, built unmodified from its source by LampRabit's CI
(.github/workflows/binaries.yml, build/linux/build-minio.sh). MinIO is licensed under the
GNU AGPL v3 (see LICENSE); its source is at
https://github.com/minio/minio/tree/$TAG
The MinIO project no longer publishes binaries and is archived: no security fixes.
EOF
[ "$OS" = linux ] && [ "$ARCH" = x86_64 ] && "$stage/minio" --version
package minio "$VERSION" "$stage"
