#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -1)
out="${MIDAD_DIST_DIR:-$PWD/target/dist}"
case "${1:-}" in
    "") notarize=false ;;
    --notarize)
        notarize=true
        : "${MIDAD_SIGN_IDENTITY:?Set the Developer ID Application identity}"
        : "${MIDAD_TEAM_ID:?Set the selected Apple Developer team ID}"
        : "${MIDAD_NOTARY_PROFILE:?Set the notarytool Keychain profile}"
        case "$MIDAD_TEAM_ID" in *[!A-Z0-9]*|'') echo "Invalid team ID" >&2; exit 1;; esac
        out="$out/notarized"
        ;;
    *) echo "Usage: $0 [--notarize]" >&2; exit 2 ;;
esac
mkdir -p "$out"
stage=$(mktemp -d "$out/.build.XXXXXX")
trap 'rm -rf "$stage"' EXIT
disk="$stage/disk"
mkdir "$disk"
for arch in aarch64 x86_64; do
    cargo build --release --locked --target "$arch-apple-darwin"
done
app="$disk/مداد.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
lipo -create target/aarch64-apple-darwin/release/midad target/x86_64-apple-darwin/release/midad -output "$app/Contents/MacOS/midad"
cp packaging/Info.plist "$app/Contents/Info.plist"
cp assets/Midad.icns "$app/Contents/Resources/"
cp assets/OFL.txt "$app/Contents/Resources/Amiri-OFL.txt"
for face in arefruqaa reemkufi notonastaliqurdu; do
    cp "assets/$face/OFL.txt" "$app/Contents/Resources/$face-OFL.txt"
done
cp packaging/THIRD-PARTY.txt "$app/Contents/Resources/"
submit() {
    result="$stage/submission.json"
    xcrun notarytool submit "$1" --keychain-profile "$MIDAD_NOTARY_PROFILE" \
        --wait --output-format json > "$result" || { cat "$result" >&2; return 1; }
    submission=$(plutil -extract id raw "$result")
    if [ "$(plutil -extract status raw "$result")" != Accepted ]; then
        xcrun notarytool log "$submission" --keychain-profile "$MIDAD_NOTARY_PROFILE" >&2
        return 1
    fi
    echo "Accepted: $submission"
}
if "$notarize"; then
    requirement="anchor apple generic and certificate leaf[field.1.2.840.113635.100.6.1.13] exists and certificate leaf[subject.OU] = \"$MIDAD_TEAM_ID\""
    codesign --force --options runtime --timestamp --sign "$MIDAD_SIGN_IDENTITY" "$app"
    codesign --verify --all-architectures --strict -R="$requirement" "$app"
    ditto -c -k --keepParent "$app" "$stage/app.zip"
    submit "$stage/app.zip"
    xcrun stapler staple "$app"
    xcrun stapler validate "$app"
    spctl --assess --type execute --verbose=2 "$app"
else
    codesign --force --sign - "$app"
    codesign --verify --strict "$app"
fi
ln -s /Applications "$disk/Applications"
dmg="$stage/Midad-$version-macOS-universal.dmg"
hdiutil create -quiet -srcfolder "$disk" -volname Midad -format UDZO "$dmg"
if "$notarize"; then
    codesign --force --timestamp --sign "$MIDAD_SIGN_IDENTITY" "$dmg"
    codesign --verify --strict -R="$requirement" "$dmg"
    submit "$dmg"
    xcrun stapler staple "$dmg"
    xcrun stapler validate "$dmg"
    spctl --assess --type open --context context:primary-signature --verbose=2 "$dmg"
fi
hdiutil verify -quiet "$dmg"
mv "$dmg" "$out/"
