pub fn format_memory_size(bytes: u64) -> String {
    const GB: u64 = 1024 * 1024 * 1024;
    const MB: u64 = 1024 * 1024;

    if bytes >= 10 * GB {
        format!("{:.2}GB", bytes as f64 / GB as f64)
    } else {
        format!("{}MB", bytes / MB)
    }
}
