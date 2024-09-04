extern crate clap;
extern crate crossterm;
extern crate nix;
extern crate nvml_wrapper as nvml;
extern crate prettytable;
extern crate procfs;
extern crate textwrap;

use clap::{Arg, Command};
use crossterm::{
    execute,
    terminal::{Clear, ClearType},
};
use nix::unistd::{Uid, User};
use nvml::enum_wrappers::device::TemperatureSensor;
use nvml::struct_wrappers::device::ProcessInfo;
use nvml::Nvml;
use prettytable::{format, Cell, Row, Table};
use procfs::process::Process;
use std::collections::HashMap;
use std::error::Error;
use std::io::stdout;
use std::thread::sleep;
use std::time::Duration;
use textwrap::fill;

// Define a struct to hold GPU information
struct GpuInfo {
    index: usize,
    name: String,
    temperature: u32,
    utilization: u32,
    memory_used: u64,
    memory_total: u64,
    user_memory: String, // New field for user's aggregate memory usage
}

fn main() -> Result<(), Box<dyn Error>> {
    // Parse command-line arguments
    let matches = Command::new("gpu-info-rs")
        .version("0.1.0")
        .author("Your Name <your.email@example.com>")
        .about("Displays GPU information in a tabular format")
        .arg(
            Arg::new("watch")
                .short('w')
                .long("watch")
                .value_name("MILLISECONDS")
                .help("Refresh interval in milliseconds")
                .required(false),
        )
        .get_matches();

    // Get the watch interval if specified
    let watch_interval = matches
        .get_one::<String>("watch")
        .map(|s| s.parse::<u64>().expect("Invalid number"));

    // Initialize NVML
    let nvml = Nvml::init()?;

    loop {
        // Get the number of available devices
        let device_count = nvml.device_count()?;

        // Create a vector to store GPU information
        let mut gpu_infos = Vec::new();

        // Loop through all devices
        for index in 0..device_count {
            let device = nvml.device_by_index(index)?;

            let name = device.name()?;
            let temperature = device.temperature(TemperatureSensor::Gpu)?;
            let utilization = device.utilization_rates()?.gpu;
            let memory = device.memory_info()?;

            // Get the list of processes running on the GPU
            let processes: Vec<ProcessInfo> = device.running_compute_processes()?;

            // Aggregate memory usage by user
            let mut user_memory_map: HashMap<String, u64> = HashMap::new();
            for process in processes {
                let pid = process.pid;
                let used_memory = match process.used_gpu_memory {
                    nvml::enums::device::UsedGpuMemory::Used(bytes) => bytes / 1_048_576, // Convert bytes to MB
                    nvml::enums::device::UsedGpuMemory::Unavailable => 0,
                };

                if let Some(username) = get_user_by_pid(pid) {
                    *user_memory_map.entry(username).or_insert(0) += used_memory;
                }
            }

            // Sort users by memory usage in descending order
            let mut sorted_users: Vec<_> = user_memory_map.iter().collect();
            sorted_users.sort_by(|a, b| b.1.cmp(a.1));

            // Format user memory usage
            let user_memory: String = sorted_users
                .iter()
                .map(|(user, &mem)| format!("{}({}M)", user, mem))
                .collect::<Vec<String>>()
                .join(" ");

            // Wrap the user memory string to a maximum width of 30 characters
            let wrapped_user_memory = fill(&user_memory, 30);

            gpu_infos.push(GpuInfo {
                index: index as usize,
                name,
                temperature,
                utilization,
                memory_used: memory.used / 1_048_576, // Convert bytes to MB
                memory_total: memory.total / 1_048_576, // Convert bytes to MB
                user_memory: wrapped_user_memory,
            });
        }

        // Clear the terminal screen
        execute!(stdout(), Clear(ClearType::All))?;

        // Create a table
        let mut table = Table::new();
        table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);

        // Add header row
        table.add_row(Row::new(vec![
            Cell::new("GPU").style_spec("Fb"),
            Cell::new("Name").style_spec("Fb"),
            Cell::new("Temp").style_spec("Fb"),
            Cell::new("Util").style_spec("Fb"),
            Cell::new("Memory").style_spec("Fb"),
            Cell::new("User Mem").style_spec("Fb"), // New column for user memory
        ]));

        // Add GPU information rows
        for gpu in gpu_infos {
            table.add_row(Row::new(vec![
                Cell::new(&gpu.index.to_string()).style_spec("Fg"),
                Cell::new(&gpu.name).style_spec("Fy"),
                Cell::new(&format!("{}°C", gpu.temperature)).style_spec("Fr"),
                Cell::new(&format!("{}%", gpu.utilization)).style_spec("Fc"),
                Cell::new(&format!("{}/{}MB", gpu.memory_used, gpu.memory_total)).style_spec("Fm"),
                Cell::new(&gpu.user_memory).style_spec("Fb"), // Display user memory
            ]));
        }

        // Print the table
        table.printstd();

        // Check if watch interval is specified
        if let Some(interval) = watch_interval {
            sleep(Duration::from_millis(interval));
        } else {
            break;
        }
    }

    Ok(())
}

fn get_user_by_pid(pid: u32) -> Option<String> {
    if let Ok(process) = Process::new(pid as i32) {
        if let Ok(uid) = process.uid() {
            if let Ok(Some(user)) = User::from_uid(Uid::from_raw(uid)) {
                return Some(user.name);
            }
        }
    }
    None
}
