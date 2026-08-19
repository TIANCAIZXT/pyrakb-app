// Mini Wiki 剪藏扩展 —— 内容脚本
// 在网页中选中文字时浮出"＋ Mini Wiki"按钮，点击即收录。

let btn = null;

document.addEventListener('mouseup', () => {
  // 延迟一点，等浏览器确定选区
  setTimeout(() => {
    const sel = window.getSelection();
    const text = sel ? sel.toString().trim() : '';
    if (text && sel.rangeCount > 0) {
      showBtn(sel.getRangeAt(0).getBoundingClientRect());
    } else {
      hideBtn();
    }
  }, 10);
});

// 滚动或切换选区时隐藏按钮
document.addEventListener('scroll', hideBtn, true);
document.addEventListener('selectionchange', () => {
  const sel = window.getSelection();
  if (!sel || sel.isCollapsed) hideBtn();
});

function showBtn(rect) {
  if (!btn) {
    btn = document.createElement('div');
    btn.textContent = '＋ Mini Wiki';
    btn.style.cssText =
      'position:fixed;z-index:2147483647;background:#2E6BE6;color:#fff;padding:5px 11px;' +
      'border-radius:9px;font-size:12px;font-family:-apple-system,system-ui,sans-serif;' +
      'cursor:pointer;box-shadow:0 4px 14px rgba(0,0,0,.3);user-select:none;';
    btn.addEventListener('mousedown', (e) => e.preventDefault()); // 防止点按钮时丢失选区
    btn.addEventListener('click', sendSelection);
    document.body.appendChild(btn);
  }
  const x = rect.left + window.scrollX;
  const y = rect.bottom + window.scrollY + 6;
  btn.style.left = x + 'px';
  btn.style.top = y + 'px';
  btn.style.display = 'block';
}
function hideBtn() { if (btn) btn.style.display = 'none'; }

function sendSelection() {
  const sel = window.getSelection();
  const text = sel ? sel.toString() : '';
  if (!text.trim()) { hideBtn(); return; }
  const payload = { title: document.title, text, url: location.href };
  hideBtn();
  chrome.runtime.sendMessage({ type: 'mw-clip', payload });
  flash('已发送到 Mini Wiki');
}

function flash(m) {
  const t = document.createElement('div');
  t.textContent = m;
  t.style.cssText =
    'position:fixed;left:50%;top:18px;transform:translateX(-50%);background:#2E6BE6;color:#fff;' +
    'padding:6px 13px;border-radius:9px;z-index:2147483647;font-size:12px;' +
    'font-family:-apple-system,system-ui,sans-serif;box-shadow:0 4px 14px rgba(0,0,0,.3);';
  document.body.appendChild(t);
  setTimeout(() => t.remove(), 1500);
}
