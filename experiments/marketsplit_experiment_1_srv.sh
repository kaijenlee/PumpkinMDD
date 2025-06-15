#!/bin/bash
DD_WIDTH=128
if [ -n "$1" ]; then
    DD_WIDTH=$1
fi
# Define variables
INPUT_DIR="benchmark_suite/market_split"   # Replace with your directory path
OUTPUT_FOLDER="experiment_1"
if [ -n "$2" ]; then
    OUTPUT_FOLDER=$2
fi
OUTPUT_DIR="experiments/${OUTPUT_FOLDER}/market_split_SRV" # Replace with your desired output file
MODEL="$INPUT_DIR/market_split.mzn"
# Create the output directory if it doesn't exist
mkdir -p "$OUTPUT_DIR"
# Loop through all files in the input directory
for INPUT_FILE in "$INPUT_DIR"/*.dzn; do
  if [ -f "$INPUT_FILE" ]; then
    BASENAME=$(basename "$INPUT_FILE")             # Extract filename
    OUTPUT_FILE="$OUTPUT_DIR/$BASENAME.txt"        # Define output file path (customize extension if needed)

    echo "Processing $INPUT_FILE -> $OUTPUT_FILE"

    # Perform your operation (Y) here. Replace 'cat' with your actual command.
    minizinc --time-limit 3600000 --solver "minizinc/pumpkin.msc" $MODEL $INPUT_FILE -s --dd-enable --dd-srv-enable --dd-max-width $DD_WIDTH> $OUTPUT_FILE

  fi
done

echo "All files processed. Outputs saved to $OUTPUT_DIR"
python3 "experiments/extract_fzn_stats.py" $OUTPUT_DIR "experiments/experiment_1/marketsplit_srv.csv"
