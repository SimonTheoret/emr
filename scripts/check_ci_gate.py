"""Check CI job coverage and require every dependency result to be success.

This is a strict scanner for job declarations, not a YAML parser. Keep jobs as
two-space-indented, unquoted IDs with block bodies, and acceptance.needs as an
inline list. actionlint validates the complete workflow, including YAML anchors.
Unsupported declaration syntax fails instead of silently omitting jobs.
"""

import argparse
import json
import os
from pathlib import Path
import re
import sys


WORKFLOW = Path(__file__).resolve().parents[1] / ".github/workflows/ci.yml"
JOB_ID = r"[A-Za-z_][A-Za-z0-9_-]*"


def required_jobs(workflow):
    jobs = {}
    current = None
    in_jobs = False
    for line in workflow.splitlines():
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        if "\t" in line:
            raise ValueError("workflow must use spaces for indentation")
        if line == "jobs:":
            if in_jobs:
                raise ValueError("duplicate jobs section")
            in_jobs = True
            continue
        if not in_jobs:
            continue
        match = re.fullmatch(rf"  ({JOB_ID}):(?:\s+#.*)?", line)
        if match:
            current = match[1]
            if current in jobs:
                raise ValueError(f"duplicate job {current}")
            jobs[current] = []
        elif current is None or not line.startswith("    "):
            raise ValueError(f"unsupported jobs syntax: {line}")
        else:
            jobs[current].append(line)

    if "acceptance" not in jobs:
        raise ValueError("missing acceptance job")
    for job, lines in jobs.items():
        properties = []
        for line in lines:
            if line.startswith("     "):
                if not properties:
                    raise ValueError(f"job {job} properties must use four-space indentation")
                continue
            match = re.match(r"    ([A-Za-z_][A-Za-z0-9_-]*):(?:\s|$)", line)
            if not match:
                raise ValueError(f"unsupported job property: {line}")
            properties.append(match[1])
        if len(properties) != len(set(properties)):
            raise ValueError(f"duplicate property in job {job}")
        if "continue-on-error" in properties:
            raise ValueError(f"job {job} must not use continue-on-error")
    gate = jobs["acceptance"]
    if "    if: always()" not in gate:
        raise ValueError("acceptance must use if: always()")
    needs = [line for line in gate if line.startswith("    needs:")]
    if len(needs) != 1:
        raise ValueError("acceptance must declare one inline needs list")
    match = re.fullmatch(r"    needs: \[([^\]]*)\]", needs[0])
    if not match:
        raise ValueError("acceptance.needs must be an inline list of job IDs")
    dependencies = [job.strip() for job in match[1].split(",")]
    if any(not re.fullmatch(JOB_ID, job) for job in dependencies):
        raise ValueError("acceptance.needs must contain plain job IDs")
    expected = set(jobs) - {"acceptance"}
    if not expected or len(set(dependencies)) != len(dependencies):
        raise ValueError("acceptance.needs must contain distinct required jobs")
    if set(dependencies) != expected:
        raise ValueError(
            f"acceptance.needs coverage mismatch; missing={sorted(expected - set(dependencies))}, "
            f"unexpected={sorted(set(dependencies) - expected)}"
        )
    return expected


def check_results(expected, results):
    if not isinstance(results, dict) or set(results) != expected:
        raise ValueError("needs results must contain exactly the required job IDs")
    unsuccessful = {
        job: result.get("result") if isinstance(result, dict) else None
        for job, result in results.items()
        if not isinstance(result, dict) or result.get("result") != "success"
    }
    if unsuccessful:
        raise ValueError(f"CI jobs did not succeed: {unsuccessful}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--results", action="store_true", help="validate NEEDS_JSON")
    args = parser.parse_args()
    try:
        expected = required_jobs(WORKFLOW.read_text())
        if args.results:
            check_results(expected, json.loads(os.environ["NEEDS_JSON"]))
    except (ValueError, KeyError, OSError) as error:
        print(f"CI acceptance failed: {error}", file=sys.stderr)
        return 1
    print("CI acceptance checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
