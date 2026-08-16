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
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct NodeFull {
    id: String,
    title: String,
    parent_id: Option<String>,
    tags: Vec<String>,
    content: String,
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
        let cur = fs::read_to_string(&cp).unwrap_or_default();
        if cur != n.content {
            let _ = fs::write(&cp, &n.content);
        }
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

/* ---------- entry ---------- */
fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![load_vault, sync_vault, reveal_vault, export_note])
        .setup(|app| {
            // 关键：必须用 WebviewUrl::App 让 Tauri 经自身协议加载前端，
            // 否则 file:// 外部页面不会注入 window.__TAURI__，invoke 全部失效。
            WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("PyraKB 本地知识库")
                .inner_size(1366.0, 900.0)
                .min_inner_size(960.0, 600.0)
                .resizable(true)
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running PyraKB application");
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
