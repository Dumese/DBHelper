//! 构建脚本：把产品 Logo（狐狸）嵌入 dbhelper.exe 的 Windows 资源，
//! 使资源管理器 / 任务栏 / 快捷方式显示品牌图标。

fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/fox_logo.ico");
        if let Err(e) = res.compile() {
            println!("cargo:warning=嵌入 exe 图标失败: {e}");
        }
    }
}
