#!/bin/sh
# Build a release rsnap.app (with icon) and a zip for distribution in dist/.
set -e
cd "$(dirname "$0")/.."

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
# Info.plist versions must be numeric: 0.0.1-alpha -> 0.0.1
short=${version%%-*}
arch=$(uname -m)

cargo build --release -p rsnap

app=dist/rsnap.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/rsnap "$app/Contents/MacOS/rsnap"
cp assets/rsnap.icns "$app/Contents/Resources/rsnap.icns"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>rsnap</string>
  <key>CFBundleDisplayName</key><string>rsnap</string>
  <key>CFBundleIdentifier</key><string>dev.rsnap.rsnap</string>
  <key>CFBundleExecutable</key><string>rsnap</string>
  <key>CFBundleIconFile</key><string>rsnap</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$short</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSApplicationCategoryType</key><string>public.app-category.graphics-design</string>
</dict>
</plist>
PLIST

# Sign with the local certificate from make-signing-cert.sh when it exists:
# its identity is stable, so the Screen Recording permission survives rebuilds.
# Otherwise ad-hoc (required to run on Apple silicon; changes every build).
identity=-
for name in "rsnap Local Signing" "rjot Local Signing"; do
  if security find-certificate -c "$name" >/dev/null 2>&1; then
    identity="$name"
    break
  fi
done
codesign --force --deep --sign "$identity" "$app"

zip="dist/rsnap-$version-macos-$arch.zip"
rm -f "$zip"
ditto -c -k --keepParent "$app" "$zip"
echo "$zip"
