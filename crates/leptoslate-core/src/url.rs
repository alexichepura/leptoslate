//! Small URL safety policy used by links and pasted media.

/// Whether a link target is safe to keep as an anchor.
///
/// Relative links, fragments, queries, `http(s)`, `mailto:` and `tel:` are
/// accepted. Executable and opaque schemes are rejected.
pub fn is_safe_href(href: &str) -> bool {
    let href = href.trim();
    if href.is_empty() {
        return false;
    }
    let lower = href.to_ascii_lowercase();
    if lower.starts_with("javascript:")
        || lower.starts_with("data:")
        || lower.starts_with("vbscript:")
    {
        return false;
    }
    if href.starts_with("//")
        || lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("mailto:")
        || lower.starts_with("tel:")
    {
        return true;
    }
    href.starts_with('/') || href.starts_with('#') || href.starts_with('?') || !href.contains(':')
}

/// Whether a pasted image source is acceptable.
///
/// Relative, HTTP(S), blob and inline raster/SVG image sources are allowed;
/// executable or non-image data URLs are rejected.
pub fn is_safe_image_src(src: &str) -> bool {
    let src = src.trim();
    if src.is_empty() {
        return false;
    }
    let lower = src.to_ascii_lowercase();
    if lower.starts_with("data:image/") {
        return true;
    }
    if lower.starts_with("data:")
        || lower.starts_with("javascript:")
        || lower.starts_with("vbscript:")
    {
        return false;
    }
    src.starts_with("//")
        || lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("blob:")
        || src.starts_with('/')
        || src.starts_with("./")
        || src.starts_with("../")
        || !src.contains(':')
}
