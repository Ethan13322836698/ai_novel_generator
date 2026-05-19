#!/usr/bin/env bash
set -e

APP_NAME="AI小说创作"
BINARY="novel_ai"
BUNDLE="${APP_NAME}.app"
VERSION="1.0.0"

echo "🔨 编译 Release…"
cargo build --release

echo "📦 打包 ${BUNDLE}…"
rm -rf "${BUNDLE}"
mkdir -p "${BUNDLE}/Contents/MacOS"
mkdir -p "${BUNDLE}/Contents/Resources"

cp "target/release/${BINARY}" "${BUNDLE}/Contents/MacOS/${BINARY}"

cat > "${BUNDLE}/Contents/Info.plist" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>        <string>${BINARY}</string>
  <key>CFBundleIdentifier</key>        <string>com.novelai.app</string>
  <key>CFBundleName</key>              <string>${APP_NAME}</string>
  <key>CFBundleDisplayName</key>       <string>AI小说创作工具</string>
  <key>CFBundleVersion</key>           <string>${VERSION}</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundlePackageType</key>       <string>APPL</string>
  <key>CFBundleSignature</key>         <string>NVAI</string>
  <key>NSHighResolutionCapable</key>   <true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
  <key>LSMinimumSystemVersion</key>    <string>12.0</string>
  <key>NSHumanReadableCopyright</key>  <string>Copyright © 2025 NovelAI</string>
</dict>
</plist>
EOF

# Remove quarantine so macOS doesn't block it
xattr -cr "${BUNDLE}" 2>/dev/null || true

echo ""
echo "✅ 打包完成：${BUNDLE}"
echo "   大小：$(du -sh "${BUNDLE}" | cut -f1)"
echo ""
echo "▶ 双击 ${BUNDLE} 或运行：open '${BUNDLE}'"
