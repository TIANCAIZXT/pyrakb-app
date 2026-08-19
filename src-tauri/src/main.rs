// PyraKB Tauri 应用入口（真实文件层）
//
// 嵌入与存储方案：
// - 前端经 Tauri 自身协议加载（WebviewUrl::App），由 generate_context! 打包 frontendDist(../src)，
//   并自动注入 window.__TAURI__；切勿改用 file:// 外部加载（不会注入 API，invoke 全失效）。
// - 数据落盘为真实文件：~/Documents/PyraKB/<标题>/_content.md（每个节点=文件夹+正文）
// - 索引/图片/主题：.pyrakb/state.json
// - 删除节点 = 软删除，文件夹移入 .pyrakb/trash/（可找回）
// - 前端通过 load_vault / sync_vault / reveal_vault 三个命令与磁盘对账同步
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{Emitter, WebviewUrl, WebviewWindowBuilder};

// 浏览器剪藏：本地 HTTP 接收服务（仅监听 127.0.0.1）
use axum::extract::State as AxumState;
use axum::{routing::get, routing::post, Json, Router};
use serde_json::json;
use tower_http::cors::{Any, CorsLayer};

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct NodeFull {
    id: String,
    title: String,
    parent_id: Option<String>,
    tags: Vec<String>,
    content: String,
    #[serde(default)]
    order: i64,
}

#[derive(Serialize, Deserialize, Clone)]
struct ImageMeta {
    name: String,
    #[serde(rename = "dataURL")]
    data_url: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct TrashEntry {
    top: NodeFull,
    descendants: Vec<NodeFull>,
    deleted_at: String,
}

#[derive(Serialize, Deserialize, Default)]
struct Persisted {
    nodes: HashMap<String, NodeFull>,
    trash: Vec<TrashEntry>,
    images: HashMap<String, ImageMeta>,
    theme: String,
}

/* ---------- paths ---------- */
// 测试可用环境变量 PYRAKB_VAULT_ROOT 覆盖根目录；正式环境为 ~/Documents/PyraKB
fn vault_base() -> PathBuf {
    if let Ok(p) = std::env::var("PYRAKB_VAULT_ROOT") {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join("Documents").join("PyraKB")
}
fn vault_root() -> PathBuf {
    vault_base()
}
fn pyrakb_dir() -> PathBuf {
    vault_root().join(".pyrakb")
}
fn state_path() -> PathBuf {
    pyrakb_dir().join("state.json")
}

fn read_state() -> Persisted {
    match fs::read_to_string(state_path()) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => Persisted::default(),
    }
}
fn write_state(p: &Persisted) {
    let _ = fs::create_dir_all(pyrakb_dir());
    if let Ok(s) = serde_json::to_string_pretty(p) {
        let _ = fs::write(state_path(), s);
    }
}

// 按节点树计算磁盘文件夹路径
fn path_of(id: &str, map: &HashMap<String, NodeFull>) -> Option<PathBuf> {
    let mut chain = Vec::new();
    let mut cur = map.get(id).cloned();
    while let Some(m) = cur {
        chain.push(m.title.clone());
        cur = match &m.parent_id {
            Some(p) => map.get(p).cloned(),
            None => None,
        };
    }
    if chain.is_empty() {
        return None;
    }
    chain.reverse();
    let mut p = vault_root();
    for t in chain {
        p = p.join(t);
    }
    Some(p)
}

fn is_descendant(id: &str, ancestor: &str, map: &HashMap<String, NodeFull>) -> bool {
    let mut cur = map.get(id).and_then(|n| n.parent_id.clone());
    while let Some(p) = cur {
        if p == ancestor {
            return true;
        }
        cur = map.get(&p).and_then(|n| n.parent_id.clone());
    }
    false
}

fn depth_of(id: &str, map: &HashMap<String, NodeFull>) -> usize {
    let mut d = 0;
    let mut cur = map.get(id).and_then(|n| n.parent_id.clone());
    while let Some(p) = cur {
        d += 1;
        cur = map.get(&p).and_then(|n| n.parent_id.clone());
    }
    d
}

/* ---------- 纯逻辑（可单测） ---------- */
/* ---------- 标签 frontmatter 读写 ---------- */
// 解析 _content.md 顶部的 YAML frontmatter，返回 (tags, 正文)；无 frontmatter 时 tags 为空、正文为全文
fn parse_frontmatter(s: &str) -> (Vec<String>, String) {
    let s = s.trim_start();
    if s.starts_with("---\n") || s.starts_with("---\r\n") {
        if let Some(end) = s[4..].find("\n---") {
            let fm = &s[4..4 + end];
            let body = s[4 + end + 5..].trim_start_matches('\n').to_string();
            let mut tags = Vec::new();
            for line in fm.lines() {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix("tags:") {
                    let rest = rest.trim().trim_start_matches('[').trim_end_matches(']').trim();
                    if !rest.is_empty() {
                        for t in rest.split(',') {
                            let t = t.trim().trim_matches('"').trim_matches('\'').to_string();
                            if !t.is_empty() {
                                tags.push(t);
                            }
                        }
                    }
                }
            }
            return (tags, body);
        }
    }
    (Vec::new(), s.to_string())
}

// 把正文 + tags 序列化为带 YAML frontmatter 头部的 _content.md 内容
fn serialize_with_tags(body: &str, tags: &[String]) -> String {
    let mut out = String::from("---\n");
    out.push_str("tags: [");
    out.push_str(
        &tags
            .iter()
            .map(|t| format!("\"{}\"", t.replace('"', "\\\"")))
            .collect::<Vec<_>>()
            .join(", "),
    );
    out.push_str("]\n---\n\n");
    out.push_str(body.trim_end());
    out.push('\n');
    out
}

// 把正文与 tags 落盘到 _content.md：
// - 正文优先保留磁盘现有正文（避免覆盖用户手编），仅当与前端传入 content 不同才以前端为准
// - tags 以传入为准写入 frontmatter 镜像（state.json 仍为主源，frontmatter 供文件可读/可移植）
fn write_content_with_tags(cp: &PathBuf, incoming_content: &str, tags: &[String]) {
    let cur = fs::read_to_string(cp).unwrap_or_default();
    let (_, body) = parse_frontmatter(&cur);
    let body = if body != incoming_content {
        incoming_content.to_string()
    } else {
        body
    };
    let new_content = serialize_with_tags(&body, tags);
    if cur.trim_end() != new_content.trim_end() {
        let _ = fs::write(cp, new_content);
    }
}

fn load_vault_inner() -> Persisted {
    let _ = fs::create_dir_all(vault_root());
    read_state()
}

fn sync_vault_inner(
    state: &mut Persisted,
    nodes: &[NodeFull],
    images: HashMap<String, ImageMeta>,
    theme: String,
) -> Result<(), String> {
    let _ = fs::create_dir_all(vault_root());
    let incoming: HashMap<String, NodeFull> =
        nodes.iter().map(|n| (n.id.clone(), n.clone())).collect();

    // 按新树深度升序处理（父先于子），保证子树移动时目标父目录已就绪
    let mut ordered: Vec<&NodeFull> = nodes.iter().collect();
    ordered.sort_by_key(|n| depth_of(&n.id, &incoming));

    // 1) 创建 / 重命名 / 移动 + 写入 _content.md
    for n in ordered {
        let desired = match path_of(&n.id, &incoming) {
            Some(p) => p,
            None => continue,
        };
        // 旧路径存在但路径变化 → 重命名/移动文件夹
        if let Some(old_path) = path_of(&n.id, &state.nodes) {
            if old_path.exists() && old_path != desired {
                // 预建目标父目录，避免 rename 因父目录不存在而失败（拖到新父下也可靠）
                if let Some(parent) = desired.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::rename(&old_path, &desired);
            }
        }
        if !desired.exists() {
            let _ = fs::create_dir_all(&desired);
        }
        let cp = desired.join("_content.md");
        write_content_with_tags(&cp, &n.content, &n.tags);
    }

    // 2) 软删除已不存在的节点（文件夹移入回收站）
    let removed: Vec<String> = state
        .nodes
        .keys()
        .cloned()
        .filter(|id| !incoming.contains_key(id))
        .collect();
    let removed_set: std::collections::HashSet<String> = removed.iter().cloned().collect();
    for id in &removed {
        // 仅移动“顶层”被删节点：若其父也在被删集合中，父节点会连子树一起移走，跳过避免重复移动
        let parent_also_removed = state
            .nodes
            .get(id)
            .and_then(|n| n.parent_id.clone())
            .map(|p| removed_set.contains(&p))
            .unwrap_or(false);
        if parent_also_removed {
            continue;
        }
        if let Some(meta) = state.nodes.get(id).cloned() {
            let src = match path_of(id, &state.nodes) {
                Some(p) => p,
                None => continue,
            };
            if src.exists() {
                let trash_dir = pyrakb_dir().join("trash");
                let _ = fs::create_dir_all(&trash_dir);
                let dst = trash_dir.join(id);
                let _ = fs::remove_dir_all(&dst);
                let _ = fs::rename(&src, &dst);
                let mut descendants = Vec::new();
                for (oid, om) in &state.nodes {
                    if oid != id && is_descendant(oid, id, &state.nodes) {
                        descendants.push(om.clone());
                    }
                }
                state.trash.push(TrashEntry {
                    top: meta,
                    descendants,
                    deleted_at: format!(
                        "{}",
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_nanos())
                            .unwrap_or(0)
                    ),
                });
            }
        }
    }

    // 4) 生成标签索引缓存 .pyrakb/tags.index（tag -> [nodeId]），供计数/筛选/未来搜索复用
    //    须在 move incoming 之前完成
    let mut idx: HashMap<String, Vec<String>> = HashMap::new();
    for (_, n) in &incoming {
        for t in &n.tags {
            idx.entry(t.clone()).or_default().push(n.id.clone());
        }
    }
    if let Ok(s) = serde_json::to_string_pretty(&idx) {
        let _ = fs::write(pyrakb_dir().join("tags.index"), s);
    }
    // 3) 提交索引
    state.nodes = incoming;
    state.images = images;
    state.theme = theme;
    write_state(state);
    Ok(())
}

/* ---------- commands ---------- */
#[tauri::command]
fn load_vault() -> Persisted {
    load_vault_inner()
}

#[tauri::command]
fn sync_vault(
    nodes: Vec<NodeFull>,
    images: HashMap<String, ImageMeta>,
    theme: String,
) -> Result<(), String> {
    let mut state = read_state();
    sync_vault_inner(&mut state, &nodes, images, theme)
}

#[tauri::command]
fn reveal_vault() -> Result<(), String> {
    let _ = fs::create_dir_all(vault_root());
    std::process::Command::new("open")
        .arg(vault_root())
        .status()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn export_note(filename: String, contents: String) -> Result<String, String> {
    let dir = vault_root().join("导出");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(&filename);
    std::fs::write(&path, contents).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

/* ---------- 浏览器剪藏：接收 HTTP 服务 ---------- */
// 接收端数据结构（来自浏览器扩展的 POST /clip）
#[derive(Deserialize)]
struct ClipIn {
    title: Option<String>,
    text: String,
    url: Option<String>,
}
// 向前端 webview 派发的事件载荷
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ClipPayload {
    title: String,
    text: String,
    url: String,
}

fn first_line(s: &str) -> String {
    s.lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())
        .map(|l| l.to_string())
        .unwrap_or_else(|| "网页剪藏".to_string())
}
fn nanos() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}

// 剪藏落盘：新建一个节点并写入 state.json + 磁盘文件夹
#[tauri::command]
fn accept_clip(
    title: String,
    text: String,
    url: String,
    target_id: Option<String>,
) -> Result<NodeFull, String> {
    let mut state = read_state();
    if let Some(pid) = &target_id {
        if !state.nodes.contains_key(pid) {
            return Err("目标节点不存在".into());
        }
    }
    let id = format!("clip_{}", nanos());
    let content = if url.trim().is_empty() {
        format!("# {}\n\n{}\n", title, text)
    } else {
        format!("# {}\n\n{}\n\n> 来源：[{}]({})\n", title, text, url, url)
    };
    let order = state.nodes.len() as i64 + 1;
    let node = NodeFull {
        id: id.clone(),
        title: title.clone(),
        parent_id: target_id.clone(),
        tags: vec!["剪藏".to_string()],
        content,
        order,
    };
    // 先入索引，便于 path_of 计算完整路径
    state.nodes.insert(id.clone(), node.clone());
    if let Some(p) = path_of(&id, &state.nodes) {
        if let Some(parent) = p.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::create_dir_all(&p);
        let _ = fs::write(p.join("_content.md"), &node.content);
    }
    write_state(&state);
    Ok(node)
}

// 健康检查：浏览器扩展用来确认 App 是否在运行
async fn health_handler() -> Json<serde_json::Value> {
    Json(json!({ "ok": true }))
}

// 接收剪藏：派发 clip-incoming 事件给前端，由前端弹收录面板
async fn clip_handler(
    AxumState(app): AxumState<tauri::AppHandle>,
    Json(body): Json<ClipIn>,
) -> Json<serde_json::Value> {
    let title = match body.title {
        Some(t) if !t.trim().is_empty() => t,
        _ => first_line(&body.text),
    };
    let payload = ClipPayload {
        title,
        text: body.text,
        url: body.url.unwrap_or_default(),
    };
    let _ = app.emit("clip-incoming", &payload);
    Json(json!({ "ok": true }))
}

// 在 127.0.0.1:18735 起本地 HTTP 服务（端口被占用则顺延尝试）
async fn start_clip_server(app: tauri::AppHandle) {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);
    let router = Router::new()
        .route("/health", get(health_handler))
        .route("/clip", post(clip_handler))
        .layer(cors)
        .with_state(app);
    for port in 18735..=18740u16 {
        let addr: std::net::SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
        match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => {
                eprintln!("[clip-server] listening on {}", addr);
                if let Err(e) = axum::serve(listener, router).await {
                    eprintln!("[clip-server] serve error: {}", e);
                }
                return;
            }
            Err(_) => continue,
        }
    }
    eprintln!("[clip-server] 无法在 18735-18740 任一端口绑定，剪藏功能不可用");
}

/* ---------- entry ---------- */
fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![load_vault, sync_vault, reveal_vault, export_note, accept_clip])
        .setup(|app| {
            // 关键：必须用 WebviewUrl::App 让 Tauri 经自身协议加载前端，
            // 否则 file:// 外部页面不会注入 window.__TAURI__，invoke 全部失效。
            WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("Mini Wiki 本地知识库")
                .inner_size(1366.0, 900.0)
                .min_inner_size(960.0, 600.0)
                .resizable(true)
                .build()?;
            // 启动浏览器剪藏本地服务（仅监听 127.0.0.1）
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                start_clip_server(handle).await;
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Mini Wiki application");
}

/* ---------- tests ---------- */
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // 测试体串行化：PYRAKB_VAULT_ROOT 是进程级环境变量，并行下会互相覆盖；
    // 同时给临时目录加唯一后缀，避免残留文件干扰。
    static TEST_LOCK: Mutex<()> = Mutex::new(());
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    fn setup() -> TempWrap {
        let _guard = TEST_LOCK.lock().unwrap();
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let tmp = std::env::temp_dir().join(format!("pyrakb_test_{}_{}", std::process::id(), n));
        let _ = fs::remove_dir_all(&tmp);
        let _ = fs::create_dir_all(&tmp);
        std::env::set_var("PYRAKB_VAULT_ROOT", &tmp);
        TempWrap(tmp, _guard)
    }
    struct TempWrap(PathBuf, #[allow(dead_code)] std::sync::MutexGuard<'static, ()>);
    impl Drop for TempWrap {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn node(id: &str, title: &str, parent: Option<&str>, content: &str) -> NodeFull {
        NodeFull {
            id: id.to_string(),
            title: title.to_string(),
            parent_id: parent.map(|s| s.to_string()),
            tags: vec![],
            content: content.to_string(),
            order: 0,
        }
    }

    #[test]
    fn creates_folders_and_content_on_sync() {
        let _t = setup();
        let mut state = Persisted::default();
        let nodes = vec![
            node("r1", "产品", None, "# 产品\n\n正文"),
            node("c1", "调研", Some("r1"), "# 调研"),
        ];
        sync_vault_inner(&mut state, &nodes, HashMap::new(), "light".into()).unwrap();

        let root = vault_base();
        assert!(root.join("产品").join("_content.md").exists(), "产品/_content.md 应存在");
        assert!(root.join("产品").join("调研").join("_content.md").exists(), "产品/调研/_content.md 应存在");
        let body = fs::read_to_string(root.join("产品").join("_content.md")).unwrap();
        assert_eq!(body, "# 产品\n\n正文");
        assert!(root.join(".pyrakb").join("state.json").exists());
    }

    #[test]
    fn deletion_moves_folder_to_trash() {
        let _t = setup();
        let mut state = Persisted::default();
        let nodes = vec![node("r1", "产品", None, "x"), node("c1", "调研", Some("r1"), "y")];
        sync_vault_inner(&mut state, &nodes, HashMap::new(), "light".into()).unwrap();
        assert!(vault_base().join("产品").exists());

        // 删除 r1（连带子节点 c1）
        let remaining = vec![];
        sync_vault_inner(&mut state, &remaining, HashMap::new(), "light".into()).unwrap();

        assert!(!vault_base().join("产品").exists(), "工作区文件夹应被移走");
        assert!(vault_base().join(".pyrakb").join("trash").join("r1").exists(), "应进入回收站");
        assert!(vault_base().join(".pyrakb").join("trash").join("r1").join("调研").exists(), "子节点应一并进回收站");
    }

    #[test]
    fn rename_moves_folder_not_duplicates() {
        let _t = setup();
        let mut state = Persisted::default();
        sync_vault_inner(&mut state, &[node("r1", "产品", None, "x")], HashMap::new(), "light".into()).unwrap();
        assert!(vault_base().join("产品").exists());
        // 重命名为 产品V2
        sync_vault_inner(&mut state, &[node("r1", "产品V2", None, "x")], HashMap::new(), "light".into()).unwrap();
        assert!(vault_base().join("产品V2").exists(), "新名文件夹应存在");
        assert!(!vault_base().join("产品").exists(), "旧名文件夹应被移走");
    }

    #[test]
    fn clip_creates_node_file_and_state_entry() {
        let _t = setup();
        let mut state = Persisted::default();
        sync_vault_inner(&mut state, &[node("r1", "收件箱", None, "x")], HashMap::new(), "light".into()).unwrap();
        let new = accept_clip(
            "测试剪藏".into(),
            "这是从网页选中的文字。".into(),
            "https://example.com/page".into(),
            Some("r1".into()),
        ).expect("accept_clip 应成功");
        assert_eq!(new.title, "测试剪藏");
        assert!(new.tags.contains(&"剪藏".to_string()));
        let state2 = read_state();
        assert!(state2.nodes.contains_key(&new.id));
        let path = path_of(&new.id, &state2.nodes).expect("应能计算路径");
        assert!(path.exists(), "节点文件夹应存在");
        let content = fs::read_to_string(path.join("_content.md")).unwrap();
        assert!(content.contains("这是从网页选中的文字。"));
        assert!(content.contains("https://example.com/page"));
        assert!(content.contains("# 测试剪藏"));
    }

    #[test]
    fn move_subtree_relocates_children_on_disk() {
        let _t = setup();
        let mut state = Persisted::default();
        let nodes = vec![
            node("r1", "产品", None, "x"),
            node("c1", "调研", Some("r1"), "y"),
            node("g1", "竞品", Some("c1"), "z"),
            node("r2", "工作", None, "w"),
        ];
        sync_vault_inner(&mut state, &nodes, HashMap::new(), "light".into()).unwrap();
        // 把「调研」(c1) 从「产品」(r1) 移动到「工作」(r2)
        let moved = vec![
            node("r1", "产品", None, "x"),
            node("c1", "调研", Some("r2"), "y"),
            node("g1", "竞品", Some("c1"), "z"),
            node("r2", "工作", None, "w"),
        ];
        sync_vault_inner(&mut state, &moved, HashMap::new(), "light".into()).unwrap();
        let root = vault_base();
        assert!(
            root.join("工作").join("调研").join("竞品").join("_content.md").exists(),
            "工作/调研/竞品 应随父移动"
        );
        assert!(!root.join("产品").join("调研").exists(), "旧的 产品/调研 应被移走");
    }
}
