//! Shared utility functions for CLI commands

/// Format a byte size as a human-readable string
#[must_use]
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Safely truncate a hash string to a maximum length
///
/// Returns the entire string if it's shorter than `max_len`.
/// This avoids panics from direct slice indexing on potentially short strings.
#[must_use]
pub fn truncate_hash(hash: &str, max_len: usize) -> &str {
    let end = hash.len().min(max_len);
    &hash[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_size_bytes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1023), "1023 B");
    }

    #[test]
    fn test_format_size_kb() {
        assert_eq!(format_size(1024), "1.00 KB");
        assert_eq!(format_size(1536), "1.50 KB");
        assert_eq!(format_size(10240), "10.00 KB");
    }

    #[test]
    fn test_format_size_mb() {
        assert_eq!(format_size(1048576), "1.00 MB");
        assert_eq!(format_size(5242880), "5.00 MB");
    }

    #[test]
    fn test_format_size_gb() {
        assert_eq!(format_size(1073741824), "1.00 GB");
    }

    #[test]
    fn test_truncate_hash_normal() {
        let hash = "abcdef1234567890abcdef1234567890";
        assert_eq!(truncate_hash(hash, 16), "abcdef1234567890");
        assert_eq!(truncate_hash(hash, 8), "abcdef12");
    }

    #[test]
    fn test_truncate_hash_short() {
        let hash = "abc";
        assert_eq!(truncate_hash(hash, 16), "abc");
        assert_eq!(truncate_hash(hash, 3), "abc");
    }

    #[test]
    fn test_truncate_hash_empty() {
        assert_eq!(truncate_hash("", 16), "");
    }
}
