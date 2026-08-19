# Mini Wiki 剪藏扩展

让浏览器在网页中选中文字后，一键收藏到 Mini Wiki 桌面应用。

## 安装（开发者模式加载，免上架）

### Chrome / Edge / Brave 等（Chromium）
1. 打开 `chrome://extensions`（Edge 为 `edge://extensions`）。
2. 右上角打开「开发者模式」。
3. 点击「加载已解压的扩展程序」，选择本目录 `clip-extension/`。
4. 装好后，在任意网页选中一段文字：
   - 右键 → 「添加到 Mini Wiki」；或
   - 选区上方会浮出「＋ Mini Wiki」按钮，点击即可。

### Firefox
1. 打开 `about:debugging#/runtime/this-firefox`。
2. 点击「临时载入附加组件」，选择本目录下的 `manifest.json`。

## 工作原理
- Mini Wiki 桌面应用启动时会监听本机 `http://127.0.0.1:18735`。
- 扩展把选中的「标题 / 文本 / 来源网址」POST 到该端口。
- 应用收到后弹出「收录到 Mini Wiki」面板，选择保存到哪个节点（或新建顶层节点）即可写入本地知识库。
- 仅监听 `127.0.0.1`，不对外暴露，安全。

## 排错
- 提示「未连接到 Mini Wiki」：确认 Mini Wiki 桌面应用已启动。
- 若 18735 端口被占用，应用会自动顺延尝试到 18740。
