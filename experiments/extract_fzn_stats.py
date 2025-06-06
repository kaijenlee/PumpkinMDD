import sys
import csv
import re
import os

def extract_stats(file_path):
    stats = {
        'paths': None,
        'flatIntVars': None,
        'flatIntConstraints': None,
        'flatTime': None,
        'engineStatisticsNumDecisions': None,
        'engineStatisticsNumConflicts': None,
        'engineStatisticsNumRestarts': None,
        'engineStatisticsNumPropagations': None,
        'engineStatisticsTimeSpentInSolver': None,
        'learnedClauseStatisticsAverageConflictSize': None,
        'learnedClauseStatisticsAverageNumberOfRemovedLiteralsRecursive': None,
        'learnedClauseStatisticsAverageNumberOfRemovedLiteralsSemantic': None,
        'learnedClauseStatisticsNumUnitClausesLearned': None,
        'learnedClauseStatisticsAverageLearnedClauseLength': None,
        'learnedClauseStatisticsAverageBacktrackAmount': None,
        'learnedClauseStatisticsAverageLbd': None,
        'UNSATISFIABLE': 0,
        'ERROR': 0
    }

    try:
        with open(file_path, 'r') as f:
            for line in f:
                line = line.strip()

                if "UNSATISFIABLE" in line:
                    stats['UNSATISFIABLE'] = 1
                if "ERROR" in line:
                    stats['ERROR'] = 1

                m = re.match(r'%%%mzn-stat:\s*([^=]+)=(.*)', line)
                if m:
                    key = m.group(1).strip()
                    val = m.group(2).strip().strip('"')
                    try:
                        val = float(val) if '.' in val else int(val)
                    except ValueError:
                        pass
                    stats[key] = val
    except Exception as e:
        print(f"Failed to read {file_path}: {e}")

    return stats

def main():
    if len(sys.argv) != 3:
        print("Usage: python extract_fzn_stats.py <directory_path> <output_file>")
        sys.exit(1)

    directory = sys.argv[1]

    if not os.path.isdir(directory):
        print(f"Error: '{directory}' is not a directory.")
        sys.exit(1)

    file_stats = {}
    all_keys = set()

    for filename in sorted(os.listdir(directory)):
        full_path = os.path.join(directory, filename)
        if os.path.isfile(full_path):  # optionally filter by extension
            stats = extract_stats(full_path)
            file_name_short = filename[:-12]
            file_stats[file_name_short] = stats
            all_keys.update(stats.keys())

    all_keys = sorted(all_keys)

    with open(sys.argv[2], 'w', newline='') as csvfile:
        writer = csv.writer(csvfile)
        writer.writerow(['Filename'] + all_keys)

        for fname, stats in sorted(file_stats.items()):
            print(fname+",")
            row = [fname] + [stats.get(key, '') for key in all_keys]
            writer.writerow(row)

    print("CSV file {sys.argv[2]} generated successfully.")

if __name__ == '__main__':
    main()
