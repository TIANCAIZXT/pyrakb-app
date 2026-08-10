#!/usr/bin/env bash
# PyraKB 免 gh 一键发布脚本（用 GitHub REST API + git 内嵌令牌）
# 适用于本机装不了 gh 的情况。用法：
#   GH_TOKEN=ghp_xxx ./publish_token.sh
# 令牌需 repo 权限；用完可 unset，勿外泄。
set -euo pipefail

REPO_NAME="${REPO_NAME:-pyrakb-app}"
TAG="v0.1.0"
TITLE="PyraKB v0.1.0"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

if [ -z "${GH_TOKEN:-}" ]; then echo "✗ 请先设置 GH_TOKEN=ghp_xxx 再运行"; exit 1; fi
API="https://api.github.com"
AUTH="Authorization: Bearer $GH_TOKEN"

DMG_PATH="${DMG_PATH:-$SCRIPT_DIR/../PyraKB_0.1.0_macOS.dmg}"
[ -f "$DMG_PATH" ] || { echo "✗ 找不到安装包：$DMG_PATH"; exit 1; }

echo "==> 获取登录名"
LOGIN=$(curl -fsS -H "$AUTH" "$API/user" | python3 -c "import sys,json;print(json.load(sys.stdin)['login'])")
echo "    账号: $LOGIN"

echo "==> 创建仓库（已存在则跳过）"
HTTP=$(curl -s -o /tmp/gh_repo.json -w "%{http_code}" -X POST -H "$AUTH" -H "Content-Type: application/json" \
  -d "{\"name\":\"$REPO_NAME\",\"private\":false,\"description\":\"PyraKB — 真实文件层本地知识库 (Tauri 2)\"}" \
  "$API/user/repos")
if [ "$HTTP" = "201" ]; then echo "    仓库已创建"; elif [ "$HTTP" = "409" ] || [ "$HTTP" = "422" ]; then echo "    仓库已存在，继续"; else echo "✗ 建仓失败 HTTP=$HTTP"; cat /tmp/gh_repo.json; exit 1; fi

REMOTE="https://github.com/$LOGIN/$REPO_NAME.git"
git remote remove origin 2>/dev/null || true
git remote add origin "$REMOTE"

echo "==> 推送 main（令牌仅本次注入，不写入 remote URL）"
git -c "url.https://$GH_TOKEN@github.com/.insteadOf=https://github.com/" push -u origin main

echo "==> 创建 Release $TAG（触发 CI 自动构建 Windows/Linux）"
REL_HTTP=$(curl -s -o /tmp/gh_rel.json -w "%{http_code}" -X POST -H "$AUTH" -H "Content-Type: application/json" \
  -d "{\"tag_name\":\"$TAG\",\"name\":\"$TITLE\",\"body\":\"PyraKB 首个发布版（真实文件层）。\n\n- macOS: PyraKB_0.1.0_macOS.dmg\n- Windows / Linux 包由 CI 自动构建并挂到本 Release。\n\n数据位于 ~/Documents/PyraKB，新建目录=真实文件夹+_content.md，删除软删到 .pyrakb/trash/。\",\"draft\":false,\"prerelease\":false}" \
  "$API/repos/$LOGIN/$REPO_NAME/releases")
if [ "$REL_HTTP" = "201" ]; then echo "    Release 已创建"; else echo "✗ 发版失败 HTTP=$REL_HTTP"; cat /tmp/gh_rel.json; exit 1; fi
REL_ID=$(python3 -c "import json;print(json.load(open('/tmp/gh_rel.json'))['id'])")

echo "==> 上传 macOS DMG 资产"
UP_HTTP=$(curl -s -o /dev/null -w "%{http_code}" -X POST \
  -H "$AUTH" -H "Content-Type: application/octet-stream" \
  --data-binary @"$DMG_PATH" \
  "$API/repos/$LOGIN/$REPO_NAME/releases/$REL_ID/assets?name=PyraKB_0.1.0_macOS.dmg")
echo "    上传 HTTP=$UP_HTTP"

echo "==> 回写 Skill config.json"
CONFIG="$SCRIPT_DIR/../.workbuddy/skills/pyrakb-installer/config.json"
if [ -f "$CONFIG" ]; then
  python3 - "$CONFIG" "$LOGIN/$REPO_NAME" <<'PY'
import json, sys
p, repo = sys.argv[1], sys.argv[2]
d = json.load(open(p))
d["repo"] = repo
d["fallbackTag"] = "v0.1.0"
json.dump(d, open(p, "w"), ensure_ascii=False, indent=2)
print("    已写入 repo =", repo)
PY
fi

echo ""
echo "✅ 完成！Release 地址："
echo "   https://github.com/$LOGIN/$REPO_NAME/releases/tag/$TAG"
echo "   CI 正在自动构建 Windows .msi / Linux .deb，几分钟后挂到该 Release，并生成 dist-manifest.json。"
echo "   PyraKB Skill 现已能从此 Release 拉取全平台安装包。"
