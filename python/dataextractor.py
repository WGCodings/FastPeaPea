import json
import random

INPUT_FILE = "C:/Users/warre/Downloads/lichess_db_eval.jsonl/lichess_db_eval.jsonl"
OUTPUT_FILE = "bullet_format.txt"

MAX_CP = 1500  # cap evaluations at ±10000 for extreme values
SAMPLE_RATE = 60


def mate_to_cp(mate, stm):
    """Convert mate in N moves to a large centipawn value, white-relative"""
    # Positive for winning side
    cp = MAX_CP if mate > 0 else -MAX_CP
    return cp if stm == "w" else -cp


def process_eval(entry):
    """
    Given a single JSON entry, return (fen, cp, result)
    """
    fen = entry["fen"]
    stm = fen.split()[1]  # "w" or "b"
    evals = entry.get("evals", [])

    if not evals:
        return None

    # Use the first PV only
    first_eval = evals[0]
    cp = first_eval.get('pvs')[0].get('cp')

    if abs(cp) < MAX_CP and cp is not None:
        # Convert cp to result in 0.0,0.5,1.0 if desired
        # Optional: just keep as cp for training
        # Here we can optionally create a target result:
        # result = 1 if cp > 50, 0.5 if abs(cp) <= 50, 0 if cp < -50
        if cp > 50:
            result = 1.0
        elif cp < -50:
            result = 0.0
        else:
            result = 0.5

        return f"{fen} | {cp} | {result}"
    else:
        return None


def main():
    entries_written = 0
    with open(INPUT_FILE, "r", encoding="utf-8") as f_in, \
            open(OUTPUT_FILE, "w", encoding="utf-8") as f_out:
        for line_num, line in enumerate(f_in, 1):
            try:
                # Sampling: keep only 1 out of SAMPLE_RATE
                if random.randint(1, SAMPLE_RATE) != 1:
                    continue
                entry = json.loads(line)
                out_line = process_eval(entry)
                if out_line:
                    f_out.write(str(out_line) + "\n")
                    entries_written +=1
                if entries_written %1000 ==0:
                    print('Written {} entries.'.format(entries_written))
            except json.JSONDecodeError:
                   x=0
            except Exception as e:
                 x=0

    print(f"Done. Output written to {OUTPUT_FILE}")


if __name__ == "__main__":
    main()
