//! 本机文件夹选择。macOS 无窗口服务进程不能用 `rfd` 的非主线程 API，改用 `osascript`。

use std::path::PathBuf;

/// 阻塞式选择文件夹；用户取消返回 `None`。
pub fn pick_directory_blocking() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        pick_directory_macos()
    }
    #[cfg(all(not(target_os = "macos"), feature = "native-dialog"))]
    {
        return rfd::FileDialog::new()
            .set_title("选择项目文件夹")
            .pick_folder();
    }
    #[cfg(all(not(target_os = "macos"), not(feature = "native-dialog")))]
    {
        None
    }
}

#[cfg(target_os = "macos")]
fn pick_directory_macos() -> Option<PathBuf> {
    use std::process::Command;

    // -128：用户在系统对话框中点「取消」
    let output = Command::new("osascript")
        .arg("-e")
        .arg(
            r#"try
    POSIX path of (choose folder with prompt "选择项目文件夹")
on error number -128
    return ""
end try"#,
        )
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        return None;
    }
    Some(PathBuf::from(path))
}
