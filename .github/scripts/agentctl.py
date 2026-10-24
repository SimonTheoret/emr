#!/usr/bin/env python3
"""Credential-separated GitHub workflow controller.

All model output enters through JSON files and is validated by agent_policy before
this module makes a GitHub API request. User-authored text is never evaluated by a
shell.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any, Iterable, Mapping

from agent_policy import (
    PolicyError,
    actor_may_answer,
    actor_may_approve_plan,
    can_skip_plan_approval,
    implementation_order,
    load_json_object,
    load_policy,
    may_run_repair,
    namespaced_labels,
    parse_command,
    path_is_protected,
    plan_id,
    replace_namespaced_labels,
    review_specialist_limit,
    state_transition_allowed,
    validate_patch,
    validate_plan,
    validate_review,
    validate_triage,
)


POLICY = load_policy()
BOT_ACTOR = "github-actions[bot]"
PLAN_MARKER = "<!-- agent-plan:"
APPROVAL_MARKER = "<!-- agent-plan-approval:"
REPAIR_MARKER = "<!-- agent-repair:"
ROLLBACK_MARKER = "<!-- agent-rollback:"


class GitHub:
    def __init__(self) -> None:
        self.repo = os.environ.get("GITHUB_REPOSITORY", POLICY.raw["repository"]["expected"])
        self.token = os.environ.get("GITHUB_TOKEN", "")
        self.api_url = os.environ.get("GITHUB_API_URL", "https://api.github.com")

    def request(
        self, method: str, path: str, data: Any | None = None, *, paginate: bool = False
    ) -> Any:
        if not self.token:
            raise PolicyError("GITHUB_TOKEN is required")
        url = path if path.startswith("http") else f"{self.api_url}{path}"
        results: list[Any] = []
        while url:
            body = None if data is None else json.dumps(data).encode("utf-8")
            request = urllib.request.Request(url, data=body, method=method)
            request.add_header("Accept", "application/vnd.github+json")
            request.add_header("Authorization", f"Bearer {self.token}")
            request.add_header("X-GitHub-Api-Version", "2022-11-28")
            if body is not None:
                request.add_header("Content-Type", "application/json")
            try:
                with urllib.request.urlopen(request) as response:
                    raw = response.read()
                    value = json.loads(raw) if raw else None
                    if paginate and isinstance(value, list):
                        results.extend(value)
                    links = _parse_links(response.headers.get("Link", ""))
                    url = links.get("next") if paginate else None
            except urllib.error.HTTPError as error:
                detail = error.read().decode("utf-8", errors="replace")
                raise PolicyError(f"GitHub API {method} {path} failed: {error.code} {detail}") from error
        return results if paginate else value

    def repo_request(self, method: str, suffix: str, data: Any | None = None, **kwargs: Any) -> Any:
        return self.request(method, f"/repos/{self.repo}{suffix}", data, **kwargs)

    def graphql(self, query: str, variables: dict[str, Any]) -> Any:
        return self.request("POST", "/graphql", {"query": query, "variables": variables})


def write_output(**values: Any) -> None:
    output = os.environ.get("GITHUB_OUTPUT")
    if not output:
        for key, value in values.items():
            print(f"{key}={_output_value(value)}")
        return
    with Path(output).open("a", encoding="utf-8") as output_file:
        for key, value in values.items():
            output_file.write(f"{key}={_output_value(value)}\n")


def bootstrap_labels(github: GitHub) -> None:
    colors = {
        "priority": "d93f0b", "blast": "fbca04", "risk": "b60205", "state": "1d76db"
    }
    descriptions = {
        "priority": "Agent scheduling priority",
        "blast": "Estimated scope of affected behavior",
        "risk": "Estimated implementation and operational risk",
        "state": "Current agent lifecycle state",
    }
    existing = {
        item["name"]: item
        for item in github.repo_request("GET", "/labels?per_page=100", paginate=True)
    }
    for namespace in ("priority", "blast", "risk", "state"):
        key = "states" if namespace == "state" else namespace
        for value in POLICY.raw["labels"][key]:
            name = f"agent/{namespace}:{value}"
            data = {
                "name": name,
                "color": colors[namespace],
                "description": f"{descriptions[namespace]}: {value}",
            }
            if name in existing:
                github.repo_request("PATCH", f"/labels/{urllib.parse.quote(name, safe='')}", data)
            else:
                github.repo_request("POST", "/labels", data)


def snapshot_issue(github: GitHub, issue_number: int, output: Path) -> None:
    issue = github.repo_request("GET", f"/issues/{issue_number}")
    comments = github.repo_request(
        "GET", f"/issues/{issue_number}/comments?per_page=100", paginate=True
    )
    safe = {
        "issue": {
            "number": issue["number"], "title": issue["title"], "body": issue.get("body") or "",
            "author": issue["user"]["login"], "created_at": issue["created_at"],
            "labels": [label["name"] for label in issue["labels"]],
        },
        "comments": [
            {
                "author": comment["user"]["login"],
                "created_at": comment["created_at"],
                "body": comment.get("body") or "",
            }
            for comment in comments
        ],
    }
    output.write_text(json.dumps(safe, indent=2), encoding="utf-8")


def publish_triage(github: GitHub, issue_number: int, result_path: Path) -> None:
    result = validate_triage(load_json_object(result_path), POLICY)
    issue = github.repo_request("GET", f"/issues/{issue_number}")
    state = "needs-info" if result["missing_information"] else "planning"
    labels = namespaced_labels(
        result["priority"], result["blast"], result["risk"], state, POLICY
    )
    existing = [label["name"] for label in issue["labels"]]
    github.repo_request(
        "PUT", f"/issues/{issue_number}/labels",
        {"labels": replace_namespaced_labels(existing, labels)},
    )
    questions = ""
    if result["missing_information"]:
        questions = "\n\nInformation needed:\n" + "\n".join(
            f"- {question}" for question in result["missing_information"]
        )
    github.repo_request(
        "POST", f"/issues/{issue_number}/comments",
        {"body": (
            "### Agent triage\n\n"
            f"- Priority: `{result['priority']}`\n"
            f"- Blast radius: `{result['blast']}`\n"
            f"- Risk: `{result['risk']}`\n\n"
            f"{result['rationale']}{questions}\n\n"
            "<!-- agent-triage:v1 -->"
        )},
    )
    if not result["missing_information"] and POLICY.raw["rollout"]["stage"] != "shadow-triage":
        dispatch(github, "agent-plan", {"issue": issue_number})


def publish_plan(github: GitHub, issue_number: int, result_path: Path) -> None:
    result = validate_plan(load_json_object(result_path))
    issue = github.repo_request("GET", f"/issues/{issue_number}")
    default_branch = POLICY.raw["repository"]["default_branch"]
    branch = github.repo_request("GET", f"/branches/{default_branch}")
    base_sha = branch["commit"]["sha"]
    identifier = plan_id(result["revision"], result["plan"], base_sha)
    labels = _label_values(issue)
    blast = labels["blast"]
    risk = labels["risk"]
    author = issue["user"]["login"]
    approval_required = not can_skip_plan_approval(
        blast=blast, risk=risk, author=author,
        changed_paths=result["changed_paths"], policy=POLICY,
    )
    state = "awaiting-approval" if approval_required else "queued"
    replacement = namespaced_labels(labels["priority"], blast, risk, state, POLICY)
    current = [label["name"] for label in issue["labels"]]
    github.repo_request(
        "PUT", f"/issues/{issue_number}/labels",
        {"labels": replace_namespaced_labels(current, replacement)},
    )
    marker = {
        "plan_id": identifier, "revision": result["revision"], "base_sha": base_sha,
        "approval_required": approval_required, "changed_paths": result["changed_paths"],
    }
    approval = (
        f"Approval required: `SimonTheoret` must comment `/approve-plan {identifier}`."
        if approval_required else
        "Approval skipped by the small/low-risk trusted-author policy."
    )
    body = (
        f"### Agent plan v{result['revision']}\n\n"
        f"Plan ID: `{identifier}`  \nBase: `{base_sha}`\n\n"
        f"{result['plan']}\n\n"
        "#### Expected paths\n" + "\n".join(f"- `{path}`" for path in result["changed_paths"]) +
        "\n\n#### Verification\n" + "\n".join(f"- `{test}`" for test in result["tests"]) +
        f"\n\n#### Compatibility\n\n{result['compatibility_notes']}\n\n{approval}\n\n"
        f"{PLAN_MARKER}{json.dumps(marker, sort_keys=True)} -->"
    )
    github.repo_request("POST", f"/issues/{issue_number}/comments", {"body": body})
    if state == "queued" and POLICY.raw["rollout"]["stage"] == "full":
        dispatch(github, "agent-dispatch", {})


def handle_comment(github: GitHub, event_path: Path) -> None:
    event = load_json_object(event_path)
    issue = event["issue"]
    actor = event["comment"]["user"]["login"]
    body = event["comment"].get("body") or ""
    label_names = [label["name"] for label in issue.get("labels", [])]
    if not all(any(name.startswith(f"agent/{namespace}:") for name in label_names) for namespace in ("priority", "blast", "risk", "state")):
        return
    labels = _label_values(issue)
    command = parse_command(body)

    if command and command[0] in {"approve-plan", "run-agent"}:
        comments = github.repo_request(
            "GET", f"/issues/{issue['number']}/comments?per_page=100", paginate=True
        )
        plan = _latest_marker(comments, PLAN_MARKER)
        if not plan or command[1] != plan["plan_id"]:
            raise PolicyError("approval or admission command does not match the latest plan")
        main = github.repo_request(
            "GET", f"/branches/{POLICY.raw['repository']['default_branch']}"
        )["commit"]["sha"]
        if main != plan["base_sha"]:
            raise PolicyError("plan approval is stale because the base branch changed")
        if command[0] == "approve-plan":
            if not actor_may_approve_plan(actor, POLICY):
                raise PolicyError("only a configured maintainer may approve a plan")
            _set_state(github, issue, labels, "queued")
            audit = {
                "plan_id": plan["plan_id"], "base_sha": main, "actor": actor,
                "comment_id": event["comment"]["id"],
            }
            github.repo_request(
                "POST", f"/issues/{issue['number']}/comments",
                {"body": f"Plan `{plan['plan_id']}` approved by @{actor}.\n\n{APPROVAL_MARKER}{json.dumps(audit, sort_keys=True)} -->"},
            )
            if POLICY.raw["rollout"]["stage"] == "full":
                dispatch(github, "agent-dispatch", {})
        else:
            if actor != issue["user"]["login"] and actor not in POLICY.maintainers:
                raise PolicyError("only the issue author or maintainer may admit an untrusted issue")
            github.repo_request(
                "POST", f"/issues/{issue['number']}/comments",
                {"body": f"Agent admission recorded for plan `{plan['plan_id']}` by @{actor}."},
            )
        return

    if labels.get("state") == "needs-info":
        if not actor_may_answer(actor, issue["user"]["login"], POLICY):
            raise PolicyError("only the issue author or a maintainer may answer clarification questions")
        dispatch(github, "agent-triage", {"issue": issue["number"]})


def schedule(github: GitHub) -> None:
    queued = github.repo_request("GET", "/issues?state=open&labels=agent/state%3Aqueued&per_page=100", paginate=True)
    active = github.repo_request("GET", "/issues?state=open&per_page=100", paginate=True)
    active_count = sum(
        any(label["name"] in {"agent/state:implementing", "agent/state:repairing"} for label in item["labels"])
        for item in active if "pull_request" not in item
    )
    capacity = max(0, int(POLICY.raw["agents"]["max_concurrent_implementations"]) - active_count)
    items = []
    for issue in queued:
        if "pull_request" in issue:
            continue
        values = _label_values(issue)
        items.append({
            "number": issue["number"], "priority": values["priority"],
            "created_at": issue["created_at"], "issue": issue, "labels": values,
        })
    selected = implementation_order(items)[:capacity]
    main_sha = github.repo_request(
        "GET", f"/branches/{POLICY.raw['repository']['default_branch']}"
    )["commit"]["sha"]
    for item in selected:
        comments = github.repo_request(
            "GET", f"/issues/{item['number']}/comments?per_page=100", paginate=True
        )
        plan = _latest_marker(comments, PLAN_MARKER)
        if not plan:
            continue
        approval_missing = plan.get("approval_required") and not _has_plan_approval(
            comments, plan["plan_id"], plan["base_sha"]
        )
        if plan.get("base_sha") != main_sha or approval_missing:
            _set_state(github, item["issue"], item["labels"], "planning")
            reason = "base branch changed" if plan.get("base_sha") != main_sha else "required approval is missing"
            github.repo_request(
                "POST", f"/issues/{item['number']}/comments",
                {"body": f"Queued plan `{plan.get('plan_id', 'unknown')}` invalidated: {reason}. A fresh plan is required."},
            )
            dispatch(github, "agent-plan", {"issue": item["number"]})
            continue
        _set_state(github, item["issue"], item["labels"], "implementing")
        dispatch(
            github, "agent-implement",
            {"issue": item["number"], "plan_id": plan["plan_id"], "base_sha": plan["base_sha"], "repair": 0},
        )


def publish_patch(
    github: GitHub, issue_number: int, plan_identifier: str, base_sha: str,
    patch_path: Path, result_path: Path, root: Path, repair: int,
) -> None:
    result = load_json_object(result_path)
    if set(result) != {"summary", "tests", "commit_message"}:
        raise PolicyError("invalid implementation result keys")
    if not all(isinstance(result[key], str) and result[key].strip() for key in ("summary", "commit_message")):
        raise PolicyError("implementation summary and commit message are required")
    if not isinstance(result["tests"], list):
        raise PolicyError("implementation tests must be a list")

    issue = github.repo_request("GET", f"/issues/{issue_number}")
    comments = github.repo_request(
        "GET", f"/issues/{issue_number}/comments?per_page=100", paginate=True
    )
    plan = _latest_marker(comments, PLAN_MARKER)
    if not plan or plan["plan_id"] != plan_identifier or plan["base_sha"] != base_sha:
        raise PolicyError("implementation does not match the current plan")
    current_main = github.repo_request(
        "GET", f"/branches/{POLICY.raw['repository']['default_branch']}"
    )["commit"]["sha"]
    if current_main != base_sha:
        raise PolicyError("implementation base is stale")

    paths = validate_patch(patch_path, root, POLICY)
    unplanned = [
        path for path in paths
        if not any(__import__("fnmatch").fnmatchcase(path, expected) for expected in plan["changed_paths"])
    ]
    if unplanned:
        raise PolicyError(f"patch changes paths outside the approved plan: {unplanned}")
    protected = [path for path in paths if path_is_protected(path, POLICY)]
    if protected and not _has_plan_approval(comments, plan_identifier, base_sha):
        raise PolicyError(f"protected paths require explicit approval: {protected}")

    subprocess.run(
        ["git", "-c", "core.hooksPath=/dev/null", "apply", "--index", str(patch_path.resolve())],
        cwd=root, check=True,
    )
    slug = re.sub(r"[^a-z0-9]+", "-", issue["title"].lower()).strip("-")[:36] or "change"
    branch = f"agent/issue-{issue_number}-{slug}"
    subprocess.run(["git", "switch", "-c", branch], cwd=root, check=True)
    subprocess.run(
        ["git", "-c", "core.hooksPath=/dev/null", "-c", "commit.gpgSign=false", "commit", "-m", result["commit_message"][:200]],
        cwd=root, check=True,
    )
    _push(root, branch)

    tests = "\n".join(f"- {entry}" for entry in result["tests"]) or "- Not reported"
    pr_body = (
        f"## Agent implementation\n\nIssue: #{issue_number}\nPlan ID: `{plan_identifier}`\n"
        f"Base: `{base_sha}`\nRepair run: {repair}\n\n{result['summary']}\n\n"
        f"### Test evidence\n\n{tests}\n\nCloses #{issue_number}\n\n"
        f"<!-- agent-pr:{json.dumps({'issue': issue_number, 'plan_id': plan_identifier}, sort_keys=True)} -->"
    )
    existing = github.repo_request(
        "GET", f"/pulls?state=open&head={urllib.parse.quote(github.repo.split('/')[0] + ':' + branch)}"
    )
    if existing:
        pr = existing[0]
        github.repo_request("PATCH", f"/pulls/{pr['number']}", {"body": pr_body})
    else:
        pr = github.repo_request(
            "POST", "/pulls",
            {"title": result["commit_message"][:200], "head": branch,
             "base": POLICY.raw["repository"]["default_branch"], "body": pr_body},
        )
    published_head = github.repo_request("GET", f"/pulls/{pr['number']}")["head"]["sha"]
    github.repo_request(
        "POST", f"/statuses/{published_head}",
        {"state": "failure", "context": "merge-policy", "description": "Current-head maintainer approval required"},
    )
    dispatch_ci(github, branch, int(pr["number"]), "agent", published_head)
    labels = _label_values(issue)
    _set_state(github, issue, labels, "reviewing")
    github.repo_request(
        "POST", f"/issues/{issue_number}/comments",
        {"body": f"Implementation published as #{pr['number']} from validated patch data."},
    )


def review_context(github: GitHub, pr_number: int, output: Path) -> None:
    pr = github.repo_request("GET", f"/pulls/{pr_number}")
    if (
        pr["user"]["login"] != BOT_ACTOR
        or pr["head"]["repo"]["full_name"] != github.repo
        or not pr["head"]["ref"].startswith("agent/issue-")
    ):
        raise PolicyError("pull request is not a publisher-created agent branch")
    match = re.search(r"<!-- agent-pr:(\{.*?\}) -->", pr.get("body") or "")
    if not match:
        raise PolicyError("pull request is not an agent implementation")
    metadata = json.loads(match.group(1))
    issue = github.repo_request("GET", f"/issues/{metadata['issue']}")
    comments = github.repo_request(
        "GET", f"/issues/{metadata['issue']}/comments?per_page=100", paginate=True
    )
    plan = _latest_marker(comments, PLAN_MARKER)
    files = github.repo_request("GET", f"/pulls/{pr_number}/files?per_page=100", paginate=True)
    labels = _label_values(issue)
    cap = review_specialist_limit(labels["blast"], labels["risk"])
    context = {
        "ticket": {"number": issue["number"], "title": issue["title"], "body": issue.get("body") or ""},
        "plan": plan,
        "pull_request": {"number": pr_number, "title": pr["title"], "head_sha": pr["head"]["sha"]},
        "diff": [{"filename": item["filename"], "status": item["status"], "patch": item.get("patch", "")} for item in files],
        "specialist_cap": cap,
    }
    output.write_text(json.dumps(context, indent=2), encoding="utf-8")
    write_output(specialist_cap=cap, issue=issue["number"], head_sha=pr["head"]["sha"])


def validate_review_command(result_path: Path, specialist_cap: int) -> None:
    result = validate_review(load_json_object(result_path), specialist_cap)
    write_output(verdict=result["verdict"])
    if result["verdict"] == "block":
        raise PolicyError("agent review reported blocking findings")


def publish_review(
    github: GitHub, pr_number: int, issue_number: int, result_path: Path, head_sha: str
) -> None:
    if issue_number == 0:
        pr = github.repo_request("GET", f"/pulls/{pr_number}")
        match = re.search(r"<!-- agent-pr:(\{.*?\}) -->", pr.get("body") or "")
        if not match:
            raise PolicyError("pull request is missing agent metadata")
        issue_number = int(json.loads(match.group(1))["issue"])
    issue = github.repo_request("GET", f"/issues/{issue_number}")
    labels = _label_values(issue)
    cap = review_specialist_limit(labels["blast"], labels["risk"])
    result = validate_review(load_json_object(result_path), cap)
    current = github.repo_request("GET", f"/pulls/{pr_number}")
    if current["head"]["sha"] != head_sha:
        raise PolicyError("review result is stale")
    findings = "\n".join(f"- {finding}" for finding in result["findings"]) or "- None"
    specialists = ", ".join(result["specialists"]) or "lead reviewer only"
    github.repo_request(
        "POST", f"/issues/{pr_number}/comments",
        {"body": (
            f"### Agent review: {result['verdict']}\n\n{result['summary']}\n\n"
            f"Blocking findings:\n{findings}\n\nSpecialists: {specialists}\n\n"
            f"Reviewed head: `{head_sha}`"
        )},
    )
    if result["verdict"] == "pass":
        github.repo_request(
            "POST", f"/pulls/{pr_number}/requested_reviewers",
            {"reviewers": sorted(POLICY.maintainers)},
        )
        return

    comments = github.repo_request(
        "GET", f"/issues/{issue_number}/comments?per_page=100", paginate=True
    )
    repairs = sum(
        comment.get("user", {}).get("login") == BOT_ACTOR
        and REPAIR_MARKER in (comment.get("body") or "")
        for comment in comments
    )
    if may_run_repair(repairs, POLICY):
        _set_state(github, issue, labels, "repairing")
        marker = {
            "run": repairs + 1, "pr": pr_number, "head_sha": head_sha,
            "source": "review", "findings": result["findings"],
        }
        github.repo_request(
            "POST", f"/issues/{issue_number}/comments",
            {"body": f"Repair run {repairs + 1} requested.\n\n{REPAIR_MARKER}{json.dumps(marker, sort_keys=True)} -->"},
        )
        plan = _latest_marker(comments, PLAN_MARKER)
        dispatch(github, "agent-implement", {
            "issue": issue_number, "plan_id": plan["plan_id"], "base_sha": plan["base_sha"],
            "repair": repairs + 1, "pr": pr_number,
        })
    else:
        _set_state(github, issue, labels, "blocked")
        github.repo_request(
            "POST", f"/issues/{issue_number}/comments",
            {"body": f"Agent repair limit reached. @{next(iter(POLICY.maintainers))}, manual intervention is required."},
        )


def publish_ci_failure(
    github: GitHub, pr_number: int, head_sha: str, failed_checks: str
) -> None:
    pr = github.repo_request("GET", f"/pulls/{pr_number}")
    if pr["user"]["login"] != BOT_ACTOR or pr["head"]["repo"]["full_name"] != github.repo:
        raise PolicyError("CI repair is restricted to publisher-created pull requests")
    if pr["head"]["sha"] != head_sha:
        raise PolicyError("CI failure result is stale")
    match = re.search(r"<!-- agent-pr:(\{.*?\}) -->", pr.get("body") or "")
    if not match:
        raise PolicyError("pull request is missing agent metadata")
    issue_number = int(json.loads(match.group(1))["issue"])
    issue = github.repo_request("GET", f"/issues/{issue_number}")
    comments = github.repo_request(
        "GET", f"/issues/{issue_number}/comments?per_page=100", paginate=True
    )
    duplicate = any(
        comment.get("user", {}).get("login") == BOT_ACTOR
        and REPAIR_MARKER in (comment.get("body") or "")
        and f'"head_sha": "{pr["head"]["sha"]}"' in (comment.get("body") or "")
        and '"source": "ci"' in (comment.get("body") or "")
        for comment in comments
    )
    if duplicate:
        return
    repairs = sum(
        comment.get("user", {}).get("login") == BOT_ACTOR
        and REPAIR_MARKER in (comment.get("body") or "")
        for comment in comments
    )
    labels = _label_values(issue)
    if not may_run_repair(repairs, POLICY):
        _set_state(github, issue, labels, "blocked")
        github.repo_request(
            "POST", f"/issues/{issue_number}/comments",
            {"body": f"Agent repair limit reached after CI failure. @{next(iter(POLICY.maintainers))}, manual intervention is required."},
        )
        return
    plan = _latest_marker(comments, PLAN_MARKER)
    _set_state(github, issue, labels, "repairing")
    marker = {
        "run": repairs + 1, "pr": pr_number, "head_sha": pr["head"]["sha"],
        "source": "ci", "findings": failed_checks,
    }
    github.repo_request(
        "POST", f"/issues/{issue_number}/comments",
        {"body": f"Repair run {repairs + 1} requested for failed CI checks: {failed_checks}.\n\n{REPAIR_MARKER}{json.dumps(marker, sort_keys=True)} -->"},
    )
    dispatch(github, "agent-implement", {
        "issue": issue_number, "plan_id": plan["plan_id"], "base_sha": plan["base_sha"],
        "repair": repairs + 1, "pr": pr_number,
    })


def merge_policy(github: GitHub, pr_number: int, command_actor: str = "", command_sha: str = "") -> None:
    pr = github.repo_request("GET", f"/pulls/{pr_number}")
    head_sha = pr["head"]["sha"]
    reviews = github.repo_request("GET", f"/pulls/{pr_number}/reviews?per_page=100", paginate=True)
    approved = any(
        review["state"] == "APPROVED"
        and review["user"]["login"] in POLICY.maintainers
        and review.get("commit_id") == head_sha
        for review in reviews
    )
    author = pr["user"]["login"]
    command_ok = (
        author in POLICY.maintainers
        and command_actor in POLICY.maintainers
        and command_sha == head_sha
    )
    rollback = (
        ROLLBACK_MARKER in (pr.get("body") or "")
        and pr["user"]["login"] == BOT_ACTOR
        and pr["head"]["repo"]["full_name"] == github.repo
        and pr["head"]["ref"].startswith("agent/revert-")
    )
    conversations_resolved = _conversations_resolved(github, pr_number)
    success = (approved or command_ok or rollback) and conversations_resolved
    description = (
        "Current-head maintainer approval and conversations verified"
        if success else "Maintainer approval at current head or conversation resolution missing"
    )
    github.repo_request(
        "POST", f"/statuses/{head_sha}",
        {"state": "success" if success else "failure", "context": "merge-policy", "description": description[:140]},
    )
    if success:
        _enable_auto_merge(github, pr, "SQUASH")
    else:
        raise PolicyError(description)


def merge_policy_event(github: GitHub, event_path: Path) -> None:
    event = load_json_object(event_path)
    pr_number = int((event.get("pull_request") or event["issue"])["number"])
    actor = ""
    sha = ""
    if "comment" in event:
        parsed = parse_command(event["comment"].get("body") or "")
        if not parsed or parsed[0] != "approve-pr":
            raise PolicyError("comment is not an approve-pr command")
        actor = event["comment"]["user"]["login"]
        sha = parsed[1]
    merge_policy(github, pr_number, actor, sha)


def dispatch(github: GitHub, event_type: str, payload: dict[str, Any]) -> None:
    github.repo_request("POST", "/dispatches", {"event_type": event_type, "client_payload": payload})


def dispatch_ci(github: GitHub, branch: str, pr_number: int, kind: str, head_sha: str) -> None:
    dispatch(github, "agent-ci", {
        "ref": branch, "pr": pr_number, "kind": kind, "head_sha": head_sha,
    })


def publish_ci_statuses(
    github: GitHub, head_sha: str, pr_number: int, kind: str, outcomes: Mapping[str, str]
) -> None:
    if not re.fullmatch(r"[0-9a-f]{40}", head_sha):
        raise PolicyError("invalid CI head SHA")
    pr = github.repo_request("GET", f"/pulls/{pr_number}")
    expected_prefix = "agent/issue-" if kind == "agent" else "agent/revert-"
    if (
        kind not in {"agent", "rollback"}
        or pr["head"]["sha"] != head_sha
        or pr["user"]["login"] != BOT_ACTOR
        or pr["head"]["repo"]["full_name"] != github.repo
        or not pr["head"]["ref"].startswith(expected_prefix)
    ):
        raise PolicyError("dispatched CI metadata does not match the current bot pull request")
    allowed = {
        "cargo fmt --check", "cargo clippy", "cargo test", "cargo build --all-targets",
        "nix build", "rollback-validate", "agent-review",
    }
    if set(outcomes) != allowed:
        raise PolicyError("CI status set does not match required contexts")
    for context, outcome in outcomes.items():
        success = outcome == "success"
        github.repo_request(
            "POST", f"/statuses/{head_sha}",
            {
                "state": "success" if success else "failure",
                "context": context,
                "description": ("Required check passed" if success else f"Required check was {outcome}")[:140],
            },
        )
    if kind == "rollback" and any(outcome != "success" for outcome in outcomes.values()):
        failed = ", ".join(
            f"{context}={outcome}" for context, outcome in outcomes.items() if outcome != "success"
        )
        github.repo_request(
            "POST", f"/issues/{pr_number}/comments",
            {"body": f"Rollback automation stopped because required checks did not pass: {failed}. @{next(iter(POLICY.maintainers))}, manual recovery is required."},
        )


def _set_state(github: GitHub, issue: dict[str, Any], labels: dict[str, str], state: str) -> None:
    if not state_transition_allowed(labels["state"], state):
        raise PolicyError(f"invalid state transition: {labels['state']} -> {state}")
    replacements = namespaced_labels(labels["priority"], labels["blast"], labels["risk"], state, POLICY)
    existing = [label["name"] for label in issue["labels"]]
    github.repo_request(
        "PUT", f"/issues/{issue['number']}/labels",
        {"labels": replace_namespaced_labels(existing, replacements)},
    )


def _label_values(issue: dict[str, Any]) -> dict[str, str]:
    values: dict[str, str] = {}
    for label in issue["labels"]:
        name = label["name"]
        if not name.startswith("agent/") or ":" not in name:
            continue
        namespace, value = name.removeprefix("agent/").split(":", 1)
        if namespace in values:
            raise PolicyError(f"duplicate agent label namespace: {namespace}")
        values[namespace] = value
    for namespace in ("priority", "blast", "risk", "state"):
        if namespace not in values:
            raise PolicyError(f"missing agent label namespace: {namespace}")
    return values


def _latest_marker(comments: Iterable[dict[str, Any]], prefix: str) -> dict[str, Any] | None:
    for comment in reversed(list(comments)):
        if comment.get("user", {}).get("login") != BOT_ACTOR:
            continue
        body = comment.get("body") or ""
        start = body.find(prefix)
        if start < 0:
            continue
        start += len(prefix)
        end = body.find(" -->", start)
        if end < 0:
            continue
        try:
            value = json.loads(body[start:end])
        except json.JSONDecodeError:
            continue
        if isinstance(value, dict):
            return value
    return None


def _has_plan_approval(comments: Iterable[dict[str, Any]], identifier: str, base_sha: str) -> bool:
    for comment in comments:
        marker = _latest_marker([comment], APPROVAL_MARKER)
        if (
            marker and marker.get("plan_id") == identifier and marker.get("base_sha") == base_sha
            and marker.get("actor") in POLICY.maintainers
        ):
            return True
    return False


def _push(root: Path, branch: str) -> None:
    token = os.environ.get("GITHUB_TOKEN")
    repository = os.environ.get("GITHUB_REPOSITORY")
    if not token or not repository:
        raise PolicyError("GITHUB_TOKEN and GITHUB_REPOSITORY are required to publish")
    # Checkout uses persist-credentials:false; supply the token only to this process.
    basic = __import__("base64").b64encode(f"x-access-token:{token}".encode()).decode()
    subprocess.run(
        ["git", "-c", f"http.https://github.com/.extraheader=AUTHORIZATION: basic {basic}",
         "-c", "credential.helper=", "push", "--force", "origin", f"HEAD:refs/heads/{branch}"],
        cwd=root, check=True, env={key: value for key, value in os.environ.items() if key != "GITHUB_TOKEN"},
    )


def _conversations_resolved(github: GitHub, pr_number: int) -> bool:
    owner, name = github.repo.split("/", 1)
    query = """
    query($owner:String!, $name:String!, $number:Int!, $cursor:String) {
      repository(owner:$owner, name:$name) {
        pullRequest(number:$number) {
          reviewThreads(first:100, after:$cursor) {
            nodes { isResolved }
            pageInfo { hasNextPage endCursor }
          }
        }
      }
    }
    """
    cursor = None
    while True:
        data = github.graphql(query, {"owner": owner, "name": name, "number": pr_number, "cursor": cursor})
        threads = data["data"]["repository"]["pullRequest"]["reviewThreads"]
        if any(not thread["isResolved"] for thread in threads["nodes"]):
            return False
        if not threads["pageInfo"]["hasNextPage"]:
            return True
        cursor = threads["pageInfo"]["endCursor"]


def _enable_auto_merge(github: GitHub, pr: dict[str, Any], method: str) -> None:
    if pr.get("auto_merge"):
        return
    query = """
    mutation($id:ID!, $method:PullRequestMergeMethod!) {
      enablePullRequestAutoMerge(input:{pullRequestId:$id, mergeMethod:$method}) {
        pullRequest { number }
      }
    }
    """
    github.graphql(query, {"id": pr["node_id"], "method": method})


def _parse_links(value: str) -> dict[str, str]:
    links: dict[str, str] = {}
    for item in value.split(","):
        match = re.match(r'\s*<([^>]+)>; rel="([^"]+)"', item)
        if match:
            links[match.group(2)] = match.group(1)
    return links


def _output_value(value: Any) -> str:
    if isinstance(value, bool):
        return str(value).lower()
    return str(value).replace("\n", "%0A").replace("\r", "%0D")


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser()
    commands = result.add_subparsers(dest="command", required=True)
    commands.add_parser("bootstrap-labels")
    snapshot = commands.add_parser("snapshot-issue")
    snapshot.add_argument("--issue", type=int, required=True)
    snapshot.add_argument("--output", type=Path, required=True)
    triage = commands.add_parser("publish-triage")
    triage.add_argument("--issue", type=int, required=True)
    triage.add_argument("--result", type=Path, required=True)
    plan = commands.add_parser("publish-plan")
    plan.add_argument("--issue", type=int, required=True)
    plan.add_argument("--result", type=Path, required=True)
    comment = commands.add_parser("handle-comment")
    comment.add_argument("--event", type=Path, required=True)
    commands.add_parser("schedule")
    patch = commands.add_parser("publish-patch")
    patch.add_argument("--issue", type=int, required=True)
    patch.add_argument("--plan-id", required=True)
    patch.add_argument("--base-sha", required=True)
    patch.add_argument("--patch", type=Path, required=True)
    patch.add_argument("--result", type=Path, required=True)
    patch.add_argument("--root", type=Path, required=True)
    patch.add_argument("--repair", type=int, default=0)
    context = commands.add_parser("review-context")
    context.add_argument("--pr", type=int, required=True)
    context.add_argument("--output", type=Path, required=True)
    validate = commands.add_parser("validate-review")
    validate.add_argument("--result", type=Path, required=True)
    validate.add_argument("--specialist-cap", type=int, required=True)
    publish = commands.add_parser("publish-review")
    publish.add_argument("--pr", type=int, required=True)
    publish.add_argument("--issue", type=int, default=0)
    publish.add_argument("--result", type=Path, required=True)
    publish.add_argument("--head-sha", required=True)
    merge = commands.add_parser("merge-policy")
    merge.add_argument("--pr", type=int, required=True)
    merge.add_argument("--command-actor", default="")
    merge.add_argument("--command-sha", default="")
    merge_event = commands.add_parser("merge-policy-event")
    merge_event.add_argument("--event", type=Path, required=True)
    ci_failure = commands.add_parser("publish-ci-failure")
    ci_failure.add_argument("--pr", type=int, required=True)
    ci_failure.add_argument("--head-sha", required=True)
    ci_failure.add_argument("--failed-checks", required=True)
    statuses = commands.add_parser("publish-ci-statuses")
    statuses.add_argument("--head-sha", required=True)
    statuses.add_argument("--pr", type=int, required=True)
    statuses.add_argument("--kind", choices=("agent", "rollback"), required=True)
    statuses.add_argument("--outcomes", type=Path, required=True)
    return result


def main() -> int:
    args = parser().parse_args()
    github = GitHub()
    try:
        if args.command == "bootstrap-labels":
            bootstrap_labels(github)
        elif args.command == "snapshot-issue":
            snapshot_issue(github, args.issue, args.output)
        elif args.command == "publish-triage":
            publish_triage(github, args.issue, args.result)
        elif args.command == "publish-plan":
            publish_plan(github, args.issue, args.result)
        elif args.command == "handle-comment":
            handle_comment(github, args.event)
        elif args.command == "schedule":
            schedule(github)
        elif args.command == "publish-patch":
            publish_patch(github, args.issue, args.plan_id, args.base_sha, args.patch, args.result, args.root, args.repair)
        elif args.command == "review-context":
            review_context(github, args.pr, args.output)
        elif args.command == "validate-review":
            validate_review_command(args.result, args.specialist_cap)
        elif args.command == "publish-review":
            publish_review(github, args.pr, args.issue, args.result, args.head_sha)
        elif args.command == "merge-policy":
            merge_policy(github, args.pr, args.command_actor, args.command_sha)
        elif args.command == "merge-policy-event":
            merge_policy_event(github, args.event)
        elif args.command == "publish-ci-failure":
            publish_ci_failure(github, args.pr, args.head_sha, args.failed_checks)
        elif args.command == "publish-ci-statuses":
            outcomes = load_json_object(args.outcomes)
            if not all(isinstance(key, str) and isinstance(value, str) for key, value in outcomes.items()):
                raise PolicyError("CI outcomes must be string pairs")
            publish_ci_statuses(github, args.head_sha, args.pr, args.kind, outcomes)
    except (PolicyError, KeyError, TypeError, subprocess.CalledProcessError) as error:
        print(f"agent policy failure: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
