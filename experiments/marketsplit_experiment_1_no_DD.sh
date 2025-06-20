#!/bin/bash

# Define variables
INPUT_DIR="benchmark_suite/market_split"   # Replace with your directory path
OUTPUT_FOLDER="experiment_1"
if [ -n "$2" ]; then
    OUTPUT_FOLDER=$2
fi
OUTPUT_DIR="experiments/${OUTPUT_FOLDER}/market_split_no_DD" # Replace with your desired output file
SKIP=false
if [ -n "$3" ]; then
    SKIP=false
fi

MODEL="$INPUT_DIR/market_split.mzn"
# Create the output directory if it doesn't exist
mkdir -p "$OUTPUT_DIR"
# Loop through all files in the input directory
for INPUT_FILE in "$INPUT_DIR"/*.dzn; do
  if [ -f "$INPUT_FILE" ]; then
    if $SKIP && (grep -q "s5\|u5" "$INPUT_FILE"); then
        echo "Skipping $INPUT_FILE due to s5 or u5 in the file."
        continue
    fi
    BASENAME=$(basename "$INPUT_FILE")             # Extract filename
    OUTPUT_FILE="$OUTPUT_DIR/$BASENAME.txt"        # Define output file path (customize extension if needed)

    echo "Processing $INPUT_FILE -> $OUTPUT_FILE"

    # Perform your operation (Y) here. Replace 'cat' with your actual command.
    minizinc --time-limit 3600000 --solver "minizinc/pumpkin.msc" $MODEL $INPUT_FILE -s  > $OUTPUT_FILE

  fi
done

echo "All files processed. Outputs saved to $OUTPUT_DIR"
python3 "experiments/extract_fzn_stats.py" $OUTPUT_DIR "experiments/${OUTPUT_FOLDER}/marketsplit_dd_disabled.csv"
