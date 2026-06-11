//! 上传附件校验：MIME 白名单与大小上限。

/// 单文件最大 20MB。
pub const MAX_ARTIFACT_BYTES: usize = 20 * 1024 * 1024;

const ALLOWED_MIMES: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/webp",
    "image/gif",
    "application/pdf",
    "text/plain",
    "text/markdown",
    "text/html",
    "text/css",
    "text/csv",
    "application/json",
    "application/javascript",
    "application/xml",
    "text/x-python",
    "text/x-rust",
    "text/x-sh",
];

/// 根据文件名与内容推断可入库的 MIME（未知文本回落为 `text/plain`）。
pub fn mime_for_artifact(filename: &str, bytes: &[u8]) -> String {
    let guessed = mime_guess::from_path(filename)
        .first_or_octet_stream()
        .to_string();
    if allowed_mime(&guessed) {
        return guessed;
    }
    if std::str::from_utf8(bytes).is_ok() {
        return "text/plain".to_string();
    }
    guessed
}

/// 是否可在 UI 内联预览（文本或常见图片）。
pub fn is_previewable_mime(mime: &str) -> bool {
    mime.starts_with("text/")
        || mime == "application/json"
        || mime == "application/javascript"
        || mime == "application/xml"
        || mime.starts_with("image/")
}

pub fn allowed_mime(mime: &str) -> bool {
    ALLOWED_MIMES.contains(&mime)
}

pub fn validate_artifact(mime: &str, size_bytes: usize) -> Result<(), &'static str> {
    if !allowed_mime(mime) {
        return Err("mime type not allowed");
    }
    if size_bytes == 0 {
        return Err("empty file");
    }
    if size_bytes > MAX_ARTIFACT_BYTES {
        return Err("file exceeds 20MB limit");
    }
    Ok(())
}
