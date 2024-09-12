import psutil
import time
import csv
from tqdm import tqdm 

def get_process_usage(process_names):
    usage_data = {}
    for proc in psutil.process_iter(['name', 'cpu_percent', 'memory_percent', 'memory_info']):
        if proc.info['name'] in process_names:
            try:
                proc.cpu_percent(interval=0.1)  # Initialize CPU measurement
                time.sleep(0.1)  # Wait a bit for more accurate measurement
                cpu_percent = proc.cpu_percent(interval=0)
                mem_percent = proc.memory_percent()
                mem_absolute = proc.memory_info().rss  # Resident Set Size in bytes
                usage_data[proc.info['name']] = {
                    'cpu': cpu_percent,
                    'memory_percent': mem_percent,
                    'memory_absolute': mem_absolute
                }
            except (psutil.NoSuchProcess, psutil.AccessDenied, psutil.ZombieProcess):
                pass
    return usage_data

def monitor_processes(process_names):
    samples = 600
    interval = 0.02 # Calculate interval between samples
    
    print(f"Monitoring processes: {', '.join(process_names)}")
    print(f"Taking {samples} samples at intervals of {interval:.2f} seconds")
    
    data_records = []
    try:
        for _ in tqdm(range(samples)):
            usage = get_process_usage(process_names)
            timestamp = time.strftime('%Y-%m-%d %H:%M:%S')
            for name, data in usage.items():
                data_records.append([
                    timestamp, 
                    name, 
                    data['cpu'], 
                    data['memory_percent'],
                    data['memory_absolute']
                ])
            time.sleep(interval)
    except KeyboardInterrupt:
        print("\nMonitoring stopped early.")


 # Write data to CSV 
    csv_filename = f'process_usage_{int(time.time())}.csv'
    with open(csv_filename, mode='w', newline='') as file:
        writer = csv.writer(file)
        writer.writerow([
            'Timestamp', 
            'Process Name', 
            'CPU Usage (%)', 
            'Memory Usage (%)', 
            'Memory Usage (bytes)'
        ])
        writer.writerows(data_records)
    print(f"Data written to {csv_filename} with {len(data_records)} records.")


if __name__ == "__main__":
    processes_to_monitor = ['nviwatch', 'nvtop', 'nvitop', 'gpustat']
    monitor_processes(processes_to_monitor)