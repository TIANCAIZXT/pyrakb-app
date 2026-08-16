fn main() {
    // 让 cargo 在嵌入的前端 HTML 变化时重编本 crate，
    // 否则 include_str! 不追踪依赖，改 src/index.html 不会重新嵌入。
    println!("cargo:rerun-if-changed=../src/index.html");
    tauri_build::build()
}
