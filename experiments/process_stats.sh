EXPERIMENT_FOLDER="experiment_1_maxwidth_2048"

OUTPUT_DIR="experiments/${EXPERIMENT_FOLDER}/market_split_SRV" # Replace with your desired output file
mkdir -p "$OUTPUT_DIR"
python3 "experiments/extract_fzn_stats.py" $OUTPUT_DIR "experiments/${EXPERIMENT_FOLDER}/marketsplit_srv.csv"

OUTPUT_DIR="experiments/${EXPERIMENT_FOLDER}/market_split" # Replace with your desired output file
mkdir -p "$OUTPUT_DIR"
python3 "experiments/extract_fzn_stats.py" $OUTPUT_DIR "experiments/${EXPERIMENT_FOLDER}/marketsplit_dd_enabled.csv"

OUTPUT_DIR="experiments/${EXPERIMENT_FOLDER}/market_split_no_DD" # Replace with your desired output file
mkdir -p "$OUTPUT_DIR"
python3 "experiments/extract_fzn_stats.py" $OUTPUT_DIR "experiments/${EXPERIMENT_FOLDER}/marketsplit_dd_disabled.csv"

OUTPUT_DIR="experiments/${EXPERIMENT_FOLDER}/market_split_SRV_Branch_first" # Replace with your desired output file
mkdir -p "$OUTPUT_DIR"
python3 "experiments/extract_fzn_stats.py" $OUTPUT_DIR "experiments/${EXPERIMENT_FOLDER}/marketsplit_srv_branch_first.csv"

OUTPUT_DIR="experiments/${EXPERIMENT_FOLDER}/QCP" # Replace with your desired output file
mkdir -p "$OUTPUT_DIR"
python3 "experiments/extract_fzn_stats.py" $OUTPUT_DIR "experiments/${EXPERIMENT_FOLDER}/qcp_dd_enabled.csv"

OUTPUT_DIR="experiments/${EXPERIMENT_FOLDER}/QCP_no_DD" # Replace with your desired output file
mkdir -p "$OUTPUT_DIR"
python3 "experiments/extract_fzn_stats.py" $OUTPUT_DIR "experiments/${EXPERIMENT_FOLDER}/qcp_disabled.csv"

OUTPUT_DIR="experiments/${EXPERIMENT_FOLDER}/QCP_SRV" # Replace with your desired output file
mkdir -p "$OUTPUT_DIR"
python3 "experiments/extract_fzn_stats.py" $OUTPUT_DIR "experiments/${EXPERIMENT_FOLDER}/qcp_srv.csv"

OUTPUT_DIR="experiments/${EXPERIMENT_FOLDER}/QCP_SRV_Branch_first" # Replace with your desired output file
mkdir -p "$OUTPUT_DIR"
python3 "experiments/extract_fzn_stats.py" $OUTPUT_DIR "experiments/${EXPERIMENT_FOLDER}/qcp_srv_branch_first.csv"
