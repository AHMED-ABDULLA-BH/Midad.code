#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)
out="$PWD/target/dist"
mkdir -p "$out"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
for arch in aarch64 x86_64; do
    cargo build --release --locked --target "$arch-apple-darwin"
done
app="$stage/مداد.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
lipo -create target/aarch64-apple-darwin/release/midad target/x86_64-apple-darwin/release/midad -output "$app/Contents/MacOS/midad"
cp packaging/Info.plist "$app/Contents/Info.plist"
cp assets/Midad.icns "$app/Contents/Resources/"
cp assets/OFL.txt "$app/Contents/Resources/Amiri-OFL.txt"
for face in arefruqaa reemkufi notonastaliqurdu; do
    cp "assets/$face/OFL.txt" "$app/Contents/Resources/$face-OFL.txt"
done
cp packaging/THIRD-PARTY.txt "$app/Contents/Resources/"
codesign --force --sign - "$app"
codesign --verify --strict "$app"
ln -s /Applications "$stage/Applications"
hdiutil create -quiet -ov -srcfolder "$stage" -volname Midad -format UDZO "$out/Midad-$version-macOS-universal.dmg"
