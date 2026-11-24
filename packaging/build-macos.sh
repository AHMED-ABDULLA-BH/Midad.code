#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
cargo build --locked --release
app="../مداد.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
install -m 755 target/release/midad "$app/Contents/MacOS/midad"
install -m 644 packaging/Info.plist "$app/Contents/Info.plist"
install -m 644 assets/OFL.txt "$app/Contents/Resources/Amiri-OFL.txt"
install -m 644 assets/Midad.icns "$app/Contents/Resources/Midad.icns"
for face in arefruqaa reemkufi notonastaliqurdu; do
    install -m 644 "assets/$face/OFL.txt" "$app/Contents/Resources/$face-OFL.txt"
done
codesign --force --sign - "$app"
