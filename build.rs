fn main() {
    println!("cargo:rerun-if-changed=assets/icon.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");

    // 把应用图标嵌入 Windows 二进制：任务栏、资源管理器与托盘都能显示 logo。
    embed_resource::compile("assets/icon.rc", embed_resource::NONE)
        .manifest_required()
        .expect("无法将应用图标嵌入 Windows 二进制");
}
