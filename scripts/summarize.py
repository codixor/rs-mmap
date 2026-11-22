"""Summarize paired compare.rs CSVs. Run with one or more CSV paths."""
import csv
import math
import statistics
import sys
from collections import defaultdict
from pathlib import Path


def summarize(path):
    groups = defaultdict(list)
    with path.open(encoding="utf-8-sig", newline="") as stream:
        for row in csv.DictReader(stream):
            groups[row["case"]].append(row)
    if len(groups) != 5:
        raise ValueError(f"{path}: expected five benchmark cases")
    print(path.name)
    print("case,rs_ns_per_op,memmap2_ns_per_op,median_paired_speedup,interval_low,interval_high")
    for name, rows in groups.items():
        rows.sort(key=lambda r: int(r["round"]))
        if ([int(r["round"]) for r in rows] != list(range(1, 25))
                or sum(r["first"] == "rs" for r in rows) != 12):
            raise ValueError(f"{name}: expected 24 balanced pairs")
        ours = [int(r["rs_ns"]) / int(r["iterations"]) for r in rows]
        upstream = [int(r["memmap2_ns"]) / int(r["iterations"]) for r in rows]
        ratios = sorted(b / a for a, b in zip(ours, upstream))
        # Two-sided distribution-free interval for the population median.
        n = len(ratios)
        tail = 0
        for k in range(n // 2):
            tail += math.comb(n, k)
            if 2 * tail / 2**n > 0.05:
                break
            lower = k
        print(f"{name},{statistics.median(ours):.1f},{statistics.median(upstream):.1f},"
              f"{statistics.median(ratios):.3f},{ratios[lower]:.3f},{ratios[n-lower-1]:.3f}")


if __name__ == "__main__":
    for arg in sys.argv[1:]:
        summarize(Path(arg))
