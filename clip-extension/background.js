// Mini Wiki 剪藏扩展 —— 后台脚本
// 右键选中文字 → "添加到 Mini Wiki"；把选中文本 POST 到本机 Mini Wiki 服务。

const CLIP_URL = 'http://127.0.0.1:18735/clip';

// 创建右键菜单（仅在选中文字时出现）
chrome.contextMenus.create(
  { id: 'mw-clip', title: '添加到 Mini Wiki', contexts: ['selection'] },
  () => { if (chrome.runtime.lastError) console.warn(chrome.runtime.lastError); }
);

chrome.contextMenus.onClicked.addListener((info, tab) => {
  if (info.menuItemId !== 'mw-clip') return;
  const text = (info.selectionText || '').trim();
  if (!text) return;
  const payload = {
    title: tab.title || '',
    text,
    url: tab.url || '',
  };
  sendClip(payload);
});

// 供 content.js 浮出按钮调用（通过消息通道）
chrome.runtime.onMessage.addListener((msg, _sender, sendResponse) => {
  if (msg && msg.type === 'mw-clip') {
    sendClip(msg.payload);
    sendResponse({ ok: true });
  }
});

function sendClip(payload) {
  fetch(CLIP_URL, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  })
    .then((r) => {
      if (r.ok) notify('已发送到 Mini Wiki');
      else notify('Mini Wiki 返回异常，请确认应用为最新版');
    })
    .catch(() => notify('未连接到 Mini Wiki，请先启动应用'));
}

function notify(msg) {
  if (chrome.notifications && chrome.notifications.create) {
    chrome.notifications.create({ type: 'basic', iconUrl: '', title: 'Mini Wiki 剪藏', message: msg });
  } else {
    console.log('[Mini Wiki] ' + msg);
  }
}
