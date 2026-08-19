#!/usr/bin/env bash
# 用系统 hdiutil 把已构建好的 Mini Wiki.app 打成标准 DMG 安装包。
# 绕过 Tauri 自带 create-dmg 对 osascript 的依赖（在受限环境下会失败）。
# 前提：先 `npx tauri build` 生成 .app（位于 src-tauri/target/release/bundle/macos/）。
set -e

PROJ="$(cd "$(dirname "$0")" && pwd)"
APP="$PROJ/src-tauri/target/release/bundle/macos/Mini Wiki.app"
OUT="$PROJ/src-tauri/target/release/bundle/dmg/Mini Wiki_0.2.0_x64.dmg"

if [ ! -d "$APP" ]; then
  echo "找不到 $APP，请先运行: npx tauri build" >&2
  exit 1
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
cp -R "$APP" "$TMP/"
ln -s /Applications "$TMP/Applications"
hdiutil create -volname "Mini Wiki" -srcfolder "$TMP" -ov -format UDZO "$OUT"
echo "DMG 已生成: $OUT"
