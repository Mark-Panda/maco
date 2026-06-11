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
];

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
