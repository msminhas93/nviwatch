pub mod system;

pub fn format_memory_size(bytes: u64) -> String {
    const GB: u64 = 1024 * 1024 * 1024;
    const MB: u64 = 1024 * 1024;

    if bytes >= 10 * GB {
        format!("{:.2}GB", bytes as f64 / GB as f64)
    } else {
        format!("{}MB", bytes / MB)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_small() {
        assert_eq!(format_memory_size(1024 * 1024), "1MB");
        assert_eq!(format_memory_size(2 * 1024 * 1024), "2MB");
        assert_eq!(format_memory_size(512 * 1024 * 1024), "512MB");
        assert_eq!(format_memory_size(8 * 1024 * 1024 * 1024), "8192MB");
    }

    #[test]
    fn test_memory_large() {
        assert_eq!(format_memory_size(10 * 1024 * 1024 * 1024), "10.00GB");
        assert_eq!(format_memory_size(12 * 1024 * 1024 * 1024), "12.00GB");
        assert_eq!(format_memory_size(16 * 1024 * 1024 * 1024), "16.00GB");
        assert_eq!(format_memory_size(24 * 1024 * 1024 * 1024), "24.00GB");
    }

    #[test]
    fn test_memory_edge() {
        assert_eq!(format_memory_size(0), "0MB");
        assert_eq!(format_memory_size(1024 * 1024 - 1), "0MB");
        assert_eq!(format_memory_size(1024 * 1024), "1MB");
        assert_eq!(format_memory_size(10 * 1024 * 1024 * 1024 - 1), "10239MB");
        assert_eq!(format_memory_size(10 * 1024 * 1024 * 1024), "10.00GB");
    }

    #[test]
    fn test_memory_precision() {
        let bytes_10_5_gb = (10.5 * 1024.0 * 1024.0 * 1024.0) as u64;
        assert_eq!(format_memory_size(bytes_10_5_gb), "10.50GB");
        let bytes_11_25_gb = (11.25 * 1024.0 * 1024.0 * 1024.0) as u64;
        assert_eq!(format_memory_size(bytes_11_25_gb), "11.25GB");
    }
}
