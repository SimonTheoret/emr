#!/usr/bin/env python3
"""Deterministic merged-main recovery helpers."""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path

from agent_policy import PolicyError, namespaced_labels, replace_namespaced_labels
from agentctl import BOT_ACTOR, GitHub, POLICY, ROLLBACK_MARKER, _label_values, _push, dispatch, dispatch_ci


VERIFICATION_JOBS = {
    "cargo fmt --check",
    "cargo clippy",
    "cargo test",
    "cargo build --all-targets",
    "nix build",
    "nix build .#dockerImage",
}
def recover(github: GitHub, event_path: Path, root: Path) -> None:
    event = json.loads(event_path.read_text(encoding="utf-8"))
    run = event["workflow_run"]
    if run["conclusion"] != "failure" or run["event"] != "push":
        return
    attempt = int(run["run_attempt"])
    if attempt == 1:
        github.repo_request("POST", f"/actions/runs/{run['id']}/rerun-failed-jobs")
        print("Requested the single permitted failed-job rerun.")
        return

    classification, evidence = classify(github, int(run["id"]))
    if classification == "infrastructure":
        notify(github, run["head_sha"], "Main CI failed repeatedly, but the failure appears infrastructural; no automatic revert was attempted.", evidence)
        return
    create_revert(github, run["head_sha"], root, evidence)


def classify(github: GitHub, run_id: int) -> tuple[str, list[str]]:
    response = github.repo_request("GET", f"/actions/runs/{run_id}/jobs?filter=latest&per_page=100")
    failed = [job for job in response["jobs"] if job["conclusion"] == "failure"]
    evidence: list[str] = []
    if not failed:
        return "infrastructure", ["No failed verification job was present in the repeated run."]
    for job in failed:
        failed_steps = [step["name"] for step in job.get("steps", []) if step["conclusion"] == "failure"]
        evidence.append(f"{job['name']}: {', '.join(failed_steps) or 'unknown step'}")
        if job["name"] not in VERIFICATION_JOBS:
            return "infrastructure", evidence
        if not failed_steps or any(
            step.startswith("Set up")
            or "checkout" in step.lower()
            or "install-nix" in step.lower()
            or step == "Complete job"
            for step in failed_steps
        ):
            return "infrastructure", evidence
    return "code", evidence


def create_revert(github: GitHub, failed_sha: str, root: Path, evidence: list[str]) -> None:
    if not re.fullmatch(r"[0-9a-f]{40}", failed_sha):
        raise PolicyError("invalid failed main SHA")
    pulls = github.repo_request("GET", f"/commits/{failed_sha}/pulls")
    source_pr = pulls[0] if pulls else None
    issue_number = 0
    if (
        source_pr
        and source_pr["user"]["login"] == BOT_ACTOR
        and source_pr["head"]["repo"]["full_name"] == github.repo
        and source_pr["head"]["ref"].startswith("agent/issue-")
    ):
        match = re.search(r"<!-- agent-pr:(\{.*?\}) -->", source_pr.get("body") or "")
        if match:
            issue_number = int(json.loads(match.group(1))["issue"])

    subprocess.run(["git", "switch", "--detach", failed_sha], cwd=root, check=True)
    title = subprocess.run(
        ["git", "show", "-s", "--format=%s", failed_sha], cwd=root, check=True,
        text=True, stdout=subprocess.PIPE,
    ).stdout.strip()
    branch = f"agent/revert-{failed_sha[:12]}"
    subprocess.run(["git", "switch", "-c", branch], cwd=root, check=True)
    revert = subprocess.run(
        ["git", "-c", "core.hooksPath=/dev/null", "-c", "commit.gpgSign=false", "revert", "--no-edit", failed_sha],
        cwd=root, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
    )
    if revert.returncode != 0:
        subprocess.run(["git", "revert", "--abort"], cwd=root, check=False)
        notify(github, failed_sha, "Automatic rollback conflicted; automation stopped.", evidence + [revert.stdout[-1000:]])
        raise PolicyError("rollback conflict")

    head_tree = _git(root, "rev-parse", "HEAD^{tree}")
    parent_tree = _git(root, "rev-parse", f"{failed_sha}^{{commit}}^{{tree}}")
    if head_tree != parent_tree:
        raise PolicyError("revert did not restore the exact parent tree")
    _push(root, branch)
    marker = {
        "failed_sha": failed_sha,
        "parent_tree": parent_tree,
        "issue": issue_number,
        "source_pr": source_pr["number"] if source_pr else 0,
    }
    body = (
        f"Automated exact inverse of failed main commit `{failed_sha}` after one failed-job rerun.\n\n"
        "Failure evidence:\n" + "\n".join(f"- {item}" for item in evidence) +
        f"\n\n{ROLLBACK_MARKER}{json.dumps(marker, sort_keys=True)} -->"
    )
    existing = github.repo_request("GET", f"/pulls?state=open&head={github.repo.split('/')[0]}:{branch}")
    if not existing:
        pr = github.repo_request("POST", "/pulls", {
            "title": f"revert: {title}"[:200], "head": branch,
            "base": POLICY.raw["repository"]["default_branch"], "body": body,
        })
    else:
        pr = existing[0]
    head_sha = github.repo_request("GET", f"/pulls/{pr['number']}")["head"]["sha"]
    dispatch_ci(github, branch, int(pr["number"]), "rollback", head_sha)
    from agentctl import merge_policy
    merge_policy(github, int(pr["number"]))


def validate(body: str, root: Path) -> None:
    match = re.search(r"<!-- agent-rollback:(\{.*?\}) -->", body)
    if not match:
        raise PolicyError("rollback pull request is missing recovery metadata")
    marker = json.loads(match.group(1))
    failed_sha = marker["failed_sha"]
    expected_parent_tree = marker["parent_tree"]
    subprocess.run(["git", "cat-file", "-e", f"{failed_sha}^{{commit}}"], cwd=root, check=True)
    actual_parent_tree = _git(root, "rev-parse", f"{failed_sha}^{{commit}}^{{tree}}")
    head_tree = _git(root, "rev-parse", "HEAD^{tree}")
    if actual_parent_tree != expected_parent_tree or head_tree != expected_parent_tree:
        raise PolicyError("rollback is not the exact inverse tree")


def validate_pr(github: GitHub, pr_number: int, root: Path) -> None:
    pr = github.repo_request("GET", f"/pulls/{pr_number}")
    if (
        pr["user"]["login"] != BOT_ACTOR
        or pr["head"]["repo"]["full_name"] != github.repo
        or not pr["head"]["ref"].startswith("agent/revert-")
    ):
        raise PolicyError("rollback validation is restricted to recovery-created pull requests")
    validate(pr.get("body") or "", root)


def followup(github: GitHub, event_path: Path) -> None:
    event = json.loads(event_path.read_text(encoding="utf-8"))
    pr = event["pull_request"]
    if not pr.get("merged"):
        return
    body = pr.get("body") or ""
    rollback_match = re.search(r"<!-- agent-rollback:(\{.*?\}) -->", body)
    if rollback_match:
        if pr["user"]["login"] != BOT_ACTOR or not pr["head"]["ref"].startswith("agent/revert-"):
            raise PolicyError("untrusted rollback metadata")
        marker = json.loads(rollback_match.group(1))
        issue_number = int(marker.get("issue") or 0)
        evidence = f"Merged change failed repeated main verification and was reverted by #{pr['number']} from `{marker['failed_sha']}`."
        if issue_number:
            issue = github.repo_request("GET", f"/issues/{issue_number}")
            labels = _label_values(issue)
            replacements = namespaced_labels(labels["priority"], labels["blast"], labels["risk"], "planning", POLICY)
            github.repo_request("PATCH", f"/issues/{issue_number}", {"state": "open"})
            github.repo_request("PUT", f"/issues/{issue_number}/labels", {
                "labels": replace_namespaced_labels([label["name"] for label in issue["labels"]], replacements)
            })
            github.repo_request("POST", f"/issues/{issue_number}/comments", {"body": evidence})
            dispatch(github, "agent-plan", {"issue": issue_number})
        else:
            github.repo_request("POST", "/issues", {
                "title": f"Investigate reverted main failure {marker['failed_sha'][:12]}",
                "body": evidence,
                "labels": ["agent/priority:high", "agent/blast:big", "agent/risk:high", "agent/state:needs-triage"],
            })
        return

    agent_match = re.search(r"<!-- agent-pr:(\{.*?\}) -->", body)
    if agent_match:
        issue_number = int(json.loads(agent_match.group(1))["issue"])
        issue = github.repo_request("GET", f"/issues/{issue_number}")
        labels = _label_values(issue)
        replacements = namespaced_labels(labels["priority"], labels["blast"], labels["risk"], "done", POLICY)
        github.repo_request("PUT", f"/issues/{issue_number}/labels", {
            "labels": replace_namespaced_labels([label["name"] for label in issue["labels"]], replacements)
        })


def notify(github: GitHub, failed_sha: str, message: str, evidence: list[str]) -> None:
    pulls = github.repo_request("GET", f"/commits/{failed_sha}/pulls")
    body = message + "\n\n" + "\n".join(f"- {item}" for item in evidence)
    if pulls:
        github.repo_request("POST", f"/issues/{pulls[0]['number']}/comments", {"body": body})
    else:
        github.repo_request("POST", "/issues", {"title": f"Main CI recovery blocked at {failed_sha[:12]}", "body": body})


def _git(root: Path, *args: str) -> str:
    return subprocess.run(
        ["git", *args], cwd=root, check=True, text=True, stdout=subprocess.PIPE
    ).stdout.strip()


def main() -> int:
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="command", required=True)
    recover_parser = commands.add_parser("recover")
    recover_parser.add_argument("--event", type=Path, required=True)
    recover_parser.add_argument("--root", type=Path, default=Path.cwd())
    validate_parser = commands.add_parser("validate")
    validate_parser.add_argument("--body-env", required=True)
    validate_parser.add_argument("--root", type=Path, default=Path.cwd())
    validate_pr_parser = commands.add_parser("validate-pr")
    validate_pr_parser.add_argument("--pr", type=int, required=True)
    validate_pr_parser.add_argument("--root", type=Path, default=Path.cwd())
    followup_parser = commands.add_parser("followup")
    followup_parser.add_argument("--event", type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.command == "recover":
            recover(GitHub(), args.event, args.root)
        elif args.command == "validate":
            validate(os.environ[args.body_env], args.root)
        elif args.command == "validate-pr":
            validate_pr(GitHub(), args.pr, args.root)
        else:
            followup(GitHub(), args.event)
    except (PolicyError, KeyError, subprocess.CalledProcessError, json.JSONDecodeError) as error:
        print(f"recovery failure: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
