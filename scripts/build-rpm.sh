#!/usr/bin/env bash
set -euo pipefail
root=$(cd -- "$(dirname -- "$0")/.." && pwd)
binary=$(realpath "${1:-$root/target/release/nexus-manager}")
output=$(realpath -m "${2:-$root/dist}")
version=$(sed -n 's/^version = "\([^"]*\)"$/\1/p' "$root/crates/nexus-manager/Cargo.toml")
test -n "$version"
test -x "$binary"
mkdir -p "$output"
stage=$(mktemp -d)
trap 'rm -rf -- "$stage"' EXIT
rpmbuild -bb "$root/packaging/nexus-boot.spec" \
    --define "_topdir $stage" --define "_tmppath $stage" \
    --define "_rpmdir $output" \
    --define "_build_name_fmt %{NAME}-%{VERSION}-%{RELEASE}.%{ARCH}.rpm" \
    --define "nexus_version $version" --define "project_dir $root" \
    --define "manager_binary $binary"
