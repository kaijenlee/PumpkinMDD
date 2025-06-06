#!/bin/bash

# Define variables
INPUT_DIR="benchmark_suite/QCP"   # Replace with your directory path
OUTPUT_DIR="experiments/experiment_1/QCP_SRV" # Replace with your desired output file

# Create the output directory if it doesn't exist
mkdir -p "$OUTPUT_DIR"
# Loop through all files in the input directory
for INPUT_FILE in "$INPUT_DIR"/*.mzn; do
  if [ -f "$INPUT_FILE" ]; then
    BASENAME=$(basename "$INPUT_FILE")             # Extract filename
    OUTPUT_FILE="$OUTPUT_DIR/$BASENAME.txt"        # Define output file path (customize extension if needed)

    echo "Processing $INPUT_FILE -> $OUTPUT_FILE"

    # Perform your operation (Y) here. Replace 'cat' with your actual command.
    minizinc --time-limit 1200000 --solver "minizinc/pumpkin.msc" $INPUT_FILE -s --dd-enable --dd-srv-enable> $OUTPUT_FILE

  fi
done

echo "All files processed. Outputs saved to $OUTPUT_DIR"
python3 "experiments/extract_fzn_stats.py" $OUTPUT_DIR "experiments/experiment_1/qcp_srv.csv"