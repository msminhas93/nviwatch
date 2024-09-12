import pandas as pd
import matplotlib.pyplot as plt
import seaborn as sns
import glob

# Find all process usage CSV files
csv_files = glob.glob('process_usage_*.csv')

# Read and combine all CSV files
dfs = []
for file in csv_files:
    df = pd.read_csv(file)
    dfs.append(df)

if not dfs:
    print("No CSV files found or no data to combine.")
else:
    combined_df = pd.concat(dfs, ignore_index=True)

    # Convert timestamp to datetime
    combined_df['Timestamp'] = pd.to_datetime(combined_df['Timestamp'])

    # Handle duplicate timestamps by adding a small time delta
    combined_df['Timestamp'] = pd.to_datetime(combined_df['Timestamp']) + pd.to_timedelta(combined_df.groupby('Timestamp').cumcount(), unit='ms')
    
    # Create a Samples for plotting
    combined_df = combined_df.sort_values('Timestamp').reset_index(drop=True)
    combined_df['Samples'] = combined_df.index

    # Convert memory usage from bytes to MB
    combined_df['Memory Usage (MB)'] = combined_df['Memory Usage (bytes)'] / (1024 * 1024)

    # Create subplots
    fig, axs = plt.subplots(3, 1, figsize=(12, 15))
    fig.suptitle('Comparison of nviwatch, nvtop, nvitop, and gpustat usage', fontsize=16)

    # CPU Usage plot using Samples
    sns.lineplot(data=combined_df, x='Samples', y='CPU Usage (%)', 
                 hue='Process Name', ax=axs[0])
    axs[0].set_title('CPU Usage (%)')
    axs[0].set_xlabel('')

    # Memory Usage (%) plot
    sns.lineplot(data=combined_df, x='Samples', y='Memory Usage (%)', 
                 hue='Process Name', ax=axs[1])
    axs[1].set_title('Memory Usage (%)')
    axs[1].set_xlabel('')

    # Memory Usage (MB) plot
    sns.lineplot(data=combined_df, x='Samples', y='Memory Usage (MB)', 
                 hue='Process Name', ax=axs[2])
    axs[2].set_title('Memory Usage (MB)')

    # Adjust layout and save figure
    plt.tight_layout()
    plt.savefig('process_usage_comparison.png', dpi=300, bbox_inches='tight')

    # Calculate summary statistics
    summary_stats = combined_df.groupby('Process Name').agg({
        'CPU Usage (%)': ['mean', 'max'],
        'Memory Usage (%)': ['mean', 'max'],
        'Memory Usage (MB)': ['mean', 'max']
    })

    # Format the summary statistics
    summary_stats = summary_stats.sort_values(by=('CPU Usage (%)', 'mean'), ascending=True).round(6)  # Round to 6 decimal places for better alignment

    # Generate markdown table
    markdown_table = summary_stats.to_markdown()

    # Save markdown table to file
    with open('process_usage_summary.md', 'w') as f:
        f.write(markdown_table)

    # Print summary statistics
    print(summary_stats)