//! 构建脚本：把 release 版 dbhelper.exe 嵌入安装程序。
//!
//! `cargo build --release` 会先产出 `target/release/dbhelper.exe`；
//! 再构建本 crate 时将其复制到 OUT_DIR 供 `include_bytes!` 使用。
//! 若尚未构建主程序，则写入一个占位文件并给出警告，保证安装器本身可编译。

use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let target = out_dir.join("dbhelper.exe");

    // workspace/target/release/dbhelper.exe
    let candidates = [
        manifest.join("../target/release/dbhelper.exe"),
        manifest.join("../target/x86_64-pc-windows-msvc/release/dbhelper.exe"),
        manifest.join("payload/dbhelper.exe"),
    ];

    let mut found = false;
    for candidate in candidates {
        if candidate.exists() {
            if std::fs::copy(&candidate, &target).is_ok() {
                found = true;
                println!("cargo:warning=已嵌入 {}", candidate.display());
                break;
            }
        }
    }
    if !found {
        println!("cargo:warning=未找到 dbhelper.exe，安装器将嵌入占位负载；请先 cargo build --release -p dbhelper");
        let _ = std::fs::write(
            &target,
            b"DBHELPER_PAYLOAD_MISSING: build `dbhelper` in release mode first.\r\n",
        );
    }
    println!("cargo:rerun-if-changed=../target/release/dbhelper.exe");
}
