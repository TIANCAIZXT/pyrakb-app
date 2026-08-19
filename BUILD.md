# Mini Wiki 桌面安装包 — 构建与分发说明

> 当前形态：**真实文件层已落地**。Tauri 2 把现有 HTML 原型包成真正的桌面应用，**数据落盘为真实本地文件**（不再用 localStorage）：每个节点 = `~/Documents/PyraKB/<标题>/` 文件夹 + `_content.md` 正文；索引/图片/主题在 `.pyrakb/state.json`；删除节点 = 软删除（文件夹移入 `.pyrakb/trash/<id>/`，可找回）。导入解析的 Word/PDF 库仍走 CDN（需联网），后续可改 Rust 原生库离线解析。

---

## 零、如何拿到 Windows 安装包（下载，无需本地有 Windows）

**macOS 无法交叉编译出 Windows `.msi`**（Tauri 官方限制）。最干净的"下载"通路是 **GitHub Actions 自动构建**——工程里已配好 `.github/workflows/build.yml`，推到 GitHub 后 CI 会同时产出 macOS `.dmg` + Windows `.msi` + Linux 包，你去 Actions 产物的 Artifacts 里下载即可。

**三步：**

1. **把 `pyrakb-app/` 初始化为 Git 仓库并推到 GitHub**（新仓库即可，公开/私有都行）：
   ```bash
   cd pyrakb-app
   git init -b main
   git add .
   git commit -m "Mini Wiki v0.2.0"
   # 在 github.com 新建空仓库，复制其 URL，然后：
   git remote add origin <你的仓库URL>
   git push -u origin main
   ```
   > 注意：`.github/workflows/build.yml` 已就绪，推送后 CI 会自动触发。

2. **等 CI 跑完**：进 GitHub 仓库 → **Actions** 标签 → 找 `Build Installers` 任务（每个平台几分钟到十几分钟）。绿色对勾即成功。

3. **下载**：点进该次运行 → 底部 **Artifacts** 区域 → 下载 `pyrakb-windows-latest`（里面是 `Mini Wiki_0.2.0_x64_en-US.msi`）。同理 `pyrakb-macos-latest` 是 `.dmg`。

> 想手动触发？仓库 → **Actions → Build Installers → Run workflow** 即可，不必改代码。
> 想发正式 Release 自动出包？把 `on:` 加一行 `release: { types: [published] }` 并打 tag 即可（可随时找我加）。

如果你**自己有 Windows 电脑**想本地直接出包，见下方「三」。

---

## 一、工程结构

```
pyrakb-app/
├── package.json          # 前端脚本(dev/build) + Tauri CLI 依赖
├── app-icon.png          # 1024 源图标（蓝底金字塔）
├── src/
│   └── index.html        # 前端 = 现有 HTML 原型
├── src-tauri/
│   ├── Cargo.toml        # Rust 依赖（tauri 2）
│   ├── build.rs
│   ├── tauri.conf.json   # 应用 / 窗口 / 打包配置
│   ├── icons/            # 全套平台图标（已生成）
│   └── src/main.rs       # 入口
└── node_modules/         # Tauri CLI（npm 安装）
```

技术栈：**Tauri 2 + Rust（后端骨架）+ 纯静态前端（现有原型）**。

## 二、macOS 构建（本机已构建出 .dmg）

> **两个已踩的坑（务必先看）**
> 1. **前端资产嵌入损坏**：Tauri v2 的 codegen 把 `frontendDist`（HTML 原型）打进二进制时产物是**乱码**（混入垃圾文本，非我的 69KB 页面），导致窗口空白。自定义协议 `pyrakb://` 在 release 模式下 WebView 也无法正确加载。**最终方案**：`main.rs` 用 `include_str!("../../src/index.html")` 编译期嵌入 HTML → 运行时写入 `app_cache_dir/index.html` 临时文件 → `file://` 协议加载。**不要删 main.rs 里的这段，也不要改回依赖 asset 嵌入或自定义协议。**
> 2. **DMG 在无界面环境必失败**：`tauri build` 最后调 `bundle_dmg.sh`（create-dmg）依赖 Finder/AppleScript 美化窗口，headless（含 CI）必挂。本机与 CI 都改为：先 `npx tauri build` 只出 `.app`，再用 `hdiutil create -srcfolder "Mini Wiki.app"` 手动压 `.dmg`。
> 3. **Tauri 2.6+ 不会自动生成 app-command 权限**：`#[tauri::command]` 标注的命令**不会**自动产生 `allow-xxx` 权限，运行时 `invoke` 会报 `permission not found: pyrakb-app:allow-load-vault`（注意命名空间是**裸名** `allow-load-vault`，不是 `pyrakb-app:allow-load-vault`）。必须在 `src-tauri/permissions/commands.toml` 显式声明 `[[set]]` + 每个命令的 `[[permission]]`，并在 `capabilities/default.json` 的 `permissions` 数组里引用裸名（`allow-load-vault` 等）。新增任何命令都要同步加这两处，否则前端调不动。

```bash
cd pyrakb-app
source "$HOME/.cargo/env"     # 让 cargo 进入 PATH
# 构建并出 .app（bundle_dmg.sh 在 headless 会跳过失败，但 .app 已生成）
npx tauri build
# 手动压 dmg（headless 可用）
hdiutil create -volname "Mini Wiki" -srcfolder "src-tauri/target/release/bundle/macos/Mini Wiki.app" -ov -format UDZO "src-tauri/target/release/bundle/dmg/Mini Wiki_0.2.0.dmg"
```

产物：`src-tauri/target/release/bundle/dmg/Mini Wiki_0.2.0.dmg`（本机为 `_x64`，在 Apple Silicon 上经 Rosetta 2 运行，Intel 原生运行）。

**签名说明**：本机无 Apple Developer 证书，使用 ad-hoc 签名。用户首次打开若被 Gatekeeper 拦截，右键「打开」→「仍要打开」即可。**正式对外分发**需：
- 购买 Apple Developer（$99/年）获取开发者证书；
- 在 `tauri.conf.json` 的 `bundle.macOS.signingIdentity` 填证书，并配置 notarize（appleId / teamId / password）。

## 三、Windows 构建（在你自己的 Windows 机器上）

1. **装 Rust**：访问 https://rustup.rs 下载 `rustup-init.exe`，默认安装（自动含 `stable` + `x86_64-pc-windows-msvc` target）。
2. **装 Node.js 22+**：https://nodejs.org（LTS 版）。
3. **装 Visual Studio 生成工具**：https://visualstudio.microsoft.com/visual-cpp-build-tools/ ，勾选「使用 C++ 的桌面开发」，并确认勾选 **MSVC v143** + **Windows 10/11 SDK**。
4. **装 WebView2 Runtime**：Win11 一般已自带；Win10 到 https://developer.microsoft.com/microsoft-edge/webview2/ 安装。
5. 把整个 `pyrakb-app/` 目录拷到 Windows（可保留 `node_modules`，否则重新 `npm install`）。
6. 在该目录执行：
   ```powershell
   npm install
   npm run build
   ```
7. 产物：`src-tauri\target\release\bundle\msi\Mini Wiki_0.2.0_x64_en-US.msi`
8. （可选）**代码签名**：准备 EV / OV 代码签名证书（如 DigiCert、Sectigo），在 `tauri.conf.json` 的 `bundle.windows` 配置 `certificateThumbprint` 与 `timestampUrl`，避免 SmartScreen 拦截。

> Windows 包**无法在 macOS 上交叉构建**（Tauri 官方限制），必须在 Windows 环境或 CI（GitHub Actions）产出 `.msi`。

## 四、版本号 / 标识 / 窗口

- **改版本**：`package.json` 的 `version` 与 `tauri.conf.json` 的 `version` 同步修改。
- **改应用标识**：`tauri.conf.json` 的 `identifier`（当前 `com.pyrakb.kb`）。
- **改窗口标题 / 尺寸**：`tauri.conf.json` 的 `app.windows`。

## 五、下一步（按原排期工程化）

1. ~~Rust 真实文件数据层：本地文件夹树 + `.pyrakb/` 隐藏目录 + 文件 CRUD。~~ ✅ 已完成（见上）。
2. ~~前端从 `localStorage` 迁移到调用 `tauri::invoke` 读写本地库。~~ ✅ 已完成（`load_vault` / `sync_vault` / `reveal_vault` 三个命令对账同步）。
3. 导入解析改 Rust 原生库（docx-rs / pdf-extract）保证离线。
4. 双向链接改节点 id 锚定（当前靠标题精确匹配）。
5. 回收站 UI：当前软删除已落盘到 `.pyrakb/trash/`，但前端暂无「还原/彻底删除」界面，可在 `state.trash` 上补一个回收站面板。

## 六、通过 GitHub Release 自动分发（Skill 拉取）

> 目标：让 WorkBuddy Skill 能**稳定拉到最新二进制**。CI 不再只给 Artifacts，而是在发版时把安装包挂到 GitHub Release 资产，并额外生成 `dist-manifest.json`。

**发版流程（一次性，之后每次发版自动跑）：**
1. 把 `pyrakb-app/` 推到 GitHub（新仓库，公开/私有都行）。
2. 在仓库打一个 tag 并**发布 Release**（推荐：仓库 → Releases → Draft a new release → 填 `v0.1.0` → Publish）。
   - `release: published` 触发 `build.yml`：矩阵构建 macOS(.dmg) + Windows(.msi) + Linux(.deb)。
   - 各平台包自动 `gh release upload` 挂到该 Release 资产。
   - `publish-manifest` 任务读取资产列表，生成 `dist-manifest.json`（含各平台资产名与 `baseUrl`）并一并挂上。
3. 下载位置：该 Release 的 **Assets** 里直接下载 `.dmg` / `.msi` / `.deb`；`dist-manifest.json` 也在此。

**Skill 侧怎么拉（供 A 步骤实现）：**
- 稳定地址 = `https://api.github.com/repos/<owner>/<repo>/releases/latest`（拿到 `tag_name` + 资产列表，按扩展名挑 `.dmg`/`.msi`/`.deb`）。
- 或直接抓 `dist-manifest.json`：`https://github.com/<owner>/<repo>/releases/latest/download/dist-manifest.json`，里面已按 `assets.macos/windows/linux` 分好，`baseUrl` 拼文件名即得下载直链。
- 注意：CI 的 `macos-latest` 现为 Apple Silicon，出的 `.dmg` 是 `aarch64`；如需 Intel/x64 或 universal 包，后续在 `build.yml` 加 `TAURI_BUILD_TARGET` / universal 配置。

**本地调试**：`git push` 到 `main`（或手动 Run workflow）只上传 Actions Artifacts，不发布 Release，适合验证构建是否通过。
