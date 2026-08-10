#!/usr/bin/env bash
# PyraKB 一键发布脚本
# 作用：把本地 pyrakb-app 仓库推到 GitHub 并发布 v0.1.0 Release，上传 macOS DMG，
#       同时把仓库地址写回 ../.workbuddy/skills/pyrakb-installer/config.json（供 Skill 拉包）。
#
# 前置（二选一）：
#   A) 交互登录：先在本机终端执行 `brew install gh && gh auth login`（浏览器授权，选 public repo 权限）
#   B) 令牌登录：导出环境变量后运行本脚本  →  GH_TOKEN=ghp_xxx ./publish.sh
#      （令牌需 repo 权限；用完后 unset，勿外泄）
#
# 用法：
#   ./publish.sh                 # 仓库名默认 pyrakb-app
#   REPO_NAME=my-kb ./publish.sh # 自定义仓库名
set -euo pipefail

REPO_NAME="${REPO_NAME:-pyrakb-app}"
TAG="v0.1.0"
TITLE="PyraKB v0.1.0"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# DMG 路径：默认工作区根目录的 PyraKB_0.1.0_macOS.dmg
DMG_PATH="${DMG_PATH:-$SCRIPT_DIR/../PyraKB_0.1.0_macOS.dmg}"
if [ ! -f "$DMG_PATH" ]; then
  echo "✗ 找不到安装包：$DMG_PATH"
  echo "  请先把构建好的 PyraKB_0.1.0_macOS.dmg 放到 pyrakb-app 的上级目录（工作区根）。"
  exit 1
fi

echo "==> 检查 gh CLI"
if ! command -v gh >/dev/null 2>&1; then
  echo "未安装 gh，尝试用 brew 安装（若失败请手动：brew install gh）..."
  if command -v brew >/dev/null 2>&1; then
    brew install gh
  else
    echo "✗ 没有 brew，请先安装 Homebrew 或手动安装 gh：https://cli.github.com/"
    exit 1
  fi
fi

echo "==> 检查 GitHub 登录态"
if ! gh auth status >/dev/null 2>&1; then
  if [ -n "${GH_TOKEN:-}" ]; then
    echo "检测到 GH_TOKEN，将用于认证。"
    export GITHUB_TOKEN="$GH_TOKEN"
  else
    echo "✗ 尚未登录 GitHub。请二选一："
    echo "   1) 终端执行：gh auth login   （浏览器授权，勾选 public repo）"
    echo "   2) 或导出令牌：GH_TOKEN=你的token ./publish.sh"
    exit 1
  fi
fi

# 用登录账号的姓名/邮箱修正本地 git 身份，保证贡献归因正确
GH_LOGIN="$(gh api user --jq .login)"
GH_NAME="$(gh api user --jq .name // empty)"
GH_EMAIL="$(gh api user --jq .email // empty)"
[ -n "$GH_NAME" ] && git config user.name "$GH_NAME"
if [ -n "$GH_EMAIL" ] && [ "$GH_EMAIL" != "null" ]; then
  git config user.email "$GH_EMAIL"
else
  git config user.email "$GH_LOGIN@users.noreply.github.com"
fi
echo "==> GitHub 账号：$GH_LOGIN"

echo "==> 创建仓库并推送（若不存在则新建，已存在则直接推送）"
if gh repo view "$REPO_NAME" >/dev/null 2>&1; then
  echo "仓库已存在，直接配置 remote 并推送。"
  git remote remove origin 2>/dev/null || true
  git remote add origin "https://github.com/$GH_LOGIN/$REPO_NAME.git"
else
  gh repo create "$REPO_NAME" --public --source . --remote origin --push --description "PyraKB — 真实文件层本地知识库（Tauri 2）"
fi
git push -u origin main

echo "==> 发布 Release $TAG 并上传 DMG"
if gh release view "$TAG" >/dev/null 2>&1; then
  echo "Release $TAG 已存在，追加资产。"
  gh release upload "$TAG" "$DMG_PATH" --clobber
else
  gh release create "$TAG" "$DMG_PATH" \
    --title "$TITLE" \
    --notes "PyraKB 首个发布版（真实文件层）。

- macOS: PyraKB_0.1.0_macOS.dmg（双击安装；首次若被拦截，右键「打开」→「仍要打开」）
- Windows / Linux 包由 CI（build.yml）在后续发版自动生成并挂到本 Release。

数据位于 ~/Documents/PyraKB，新建目录=真实文件夹+_content.md，删除软删到 .pyrakb/trash/。"
fi

echo "==> 回写 Skill 的 config.json（仓库地址）"
CONFIG="$SCRIPT_DIR/../.workbuddy/skills/pyrakb-installer/config.json"
if [ -f "$CONFIG" ]; then
  # 用 python 安全改写 json 的 repo 字段
  PY="${PY:-python3}"
  "$PY" - "$CONFIG" "$GH_LOGIN/$REPO_NAME" <<'PY'
import json, sys
p, repo = sys.argv[1], sys.argv[2]
d = json.load(open(p))
d["repo"] = repo
d["fallbackTag"] = "v0.1.0"
json.dump(d, open(p, "w"), ensure_ascii=False, indent=2)
print("已写入 repo =", repo)
PY
else
  echo "（未找到 Skill config.json，跳过回写；手动在 WorkBuddy 的 pyrakb-installer/config.json 设 repo=$GH_LOGIN/$REPO_NAME）"
fi

RELEASE_URL="https://github.com/$GH_LOGIN/$REPO_NAME/releases/tag/$TAG"
echo ""
echo "✅ 完成！Release 地址："
echo "   $RELEASE_URL"
echo "   Skill 现在可从该 Release 拉取 PyraKB_0.1.0_macOS.dmg。"
echo "   Windows/Linux 包：推到 GitHub 后，Actions 会自动构建并挂到这个 Release（见 build.yml）。"
