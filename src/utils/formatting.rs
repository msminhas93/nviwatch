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
    fn test_format_memory_size_small() {
        // Test small memory sizes (less than 10GB)
        assert_eq!(format_memory_size(1024 * 1024), "1MB"); // 1MB
        assert_eq!(format_memory_size(2 * 1024 * 1024), "2MB"); // 2MB
        assert_eq!(format_memory_size(512 * 1024 * 1024), "512MB"); // 512MB
        assert_eq!(format_memory_size(8 * 1024 * 1024 * 1024), "8192MB"); // 8GB
    }

    #[test]
    fn test_format_memory_size_large() {
        // Test large memory sizes (10GB and above)
        assert_eq!(format_memory_size(10 * 1024 * 1024 * 1024), "10.00GB"); // 10GB
        assert_eq!(format_memory_size(12 * 1024 * 1024 * 1024), "12.00GB"); // 12GB
        assert_eq!(format_memory_size(16 * 1024 * 1024 * 1024), "16.00GB"); // 16GB
        assert_eq!(format_memory_size(24 * 1024 * 1024 * 1024), "24.00GB"); // 24GB
    }

    #[test]
    fn test_format_memory_size_edge_cases() {
        // Test edge cases
        assert_eq!(format_memory_size(0), "0MB"); // 0 bytes
        assert_eq!(format_memory_size(1024 * 1024 - 1), "0MB"); // Just under 1MB
        assert_eq!(format_memory_size(1024 * 1024), "1MB"); // Exactly 1MB
        assert_eq!(format_memory_size(10 * 1024 * 1024 * 1024 - 1), "10239MB"); // Just under 10GB
        assert_eq!(format_memory_size(10 * 1024 * 1024 * 1024), "10.00GB"); // Exactly 10GB
    }

    #[test]
    fn test_format_memory_size_decimal_precision() {
        // Test decimal precision for GB values
        let bytes_10_5_gb = (10.5 * 1024.0 * 1024.0 * 1024.0) as u64;
        assert_eq!(format_memory_size(bytes_10_5_gb), "10.50GB");
        
        let bytes_11_25_gb = (11.25 * 1024.0 * 1024.0 * 1024.0) as u64;
        assert_eq!(format_memory_size(bytes_11_25_gb), "11.25GB");
    }
}
