"""Pure policy and validation helpers for the agent GitHub workflows."""

from __future__ import annotations

import fnmatch
import hashlib
import json
import os
import re
import subprocess
import tomllib
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import Any, Iterable, Mapping, Sequence


POLICY_PATH = Path(__file__).parents[1] / "agent-policy.toml"
NAMESPACES = ("priority", "blast", "risk", "state")
PRIORITY_ORDER = {"extreme": 0, "high": 1, "medium": 2, "low": 3}
REVIEW_LIMITS = {
    ("small", "low"): 0,
    ("medium", "low"): 2,
    ("medium", "medium"): 2,
}
STATE_TRANSITIONS = {
    "needs-triage": {"needs-info", "planning", "blocked"},
    "needs-info": {"needs-info", "planning", "blocked"},
    "planning": {"awaiting-approval", "queued", "needs-info", "blocked"},
    "awaiting-approval": {"planning", "queued", "blocked"},
    "queued": {"planning", "implementing", "blocked"},
    "implementing": {"reviewing", "repairing", "blocked"},
    "reviewing": {"repairing", "done", "blocked"},
    "repairing": {"reviewing", "blocked"},
    "blocked": {"planning", "queued"},
    "done": {"planning"},
}
SHA_RE = re.compile(r"^[0-9a-f]{40}$")
PLAN_COMMAND_RE = re.compile(r"^/approve-plan\s+([0-9a-f]{16})\s*$")
RUN_COMMAND_RE = re.compile(r"^/run-agent\s+([0-9a-f]{16})\s*$")
PR_COMMAND_RE = re.compile(r"^/approve-pr\s+([0-9a-f]{40})\s*$")
DIFF_PATH_RE = re.compile(r"^diff --git a/(.+) b/(.+)$")


class PolicyError(ValueError):
    """A deterministic policy violation."""


@dataclass(frozen=True)
class Policy:
    raw: Mapping[str, Any]

    @property
    def maintainers(self) -> frozenset[str]:
        return frozenset(self.raw["actors"]["maintainers"])

    @property
    def trusted_authors(self) -> frozenset[str]:
        return frozenset(self.raw["actors"]["trusted_authors"])

    @property
    def protected_paths(self) -> tuple[str, ...]:
        return tuple(self.raw["paths"]["protected"])

    @property
    def max_repair_runs(self) -> int:
        return int(self.raw["agents"]["max_repair_runs"])

    @property
    def max_patch_bytes(self) -> int:
        return int(self.raw["limits"]["max_patch_bytes"])

    @property
    def max_changed_files(self) -> int:
        return int(self.raw["limits"]["max_changed_files"])


def load_policy(path: Path = POLICY_PATH) -> Policy:
    with path.open("rb") as policy_file:
        return Policy(tomllib.load(policy_file))


def normalize_plan_text(text: str) -> str:
    """Normalize presentation-only whitespace without changing plan wording."""
    lines = [re.sub(r"[ \t]+", " ", line.strip()) for line in text.splitlines()]
    return "\n".join(line for line in lines if line).strip()


def plan_id(revision: int, plan_text: str, base_sha: str) -> str:
    if revision < 1:
        raise PolicyError("plan revision must be positive")
    if not SHA_RE.fullmatch(base_sha):
        raise PolicyError("base SHA must be 40 lowercase hexadecimal characters")
    material = f"v{revision}\0{normalize_plan_text(plan_text)}\0{base_sha}".encode()
    return hashlib.sha256(material).hexdigest()[:16]


def namespaced_labels(
    priority: str, blast: str, risk: str, state: str, policy: Policy
) -> tuple[str, ...]:
    requested = {
        "priority": priority,
        "blast": blast,
        "risk": risk,
        "state": state,
    }
    for namespace, value in requested.items():
        key = "states" if namespace == "state" else namespace
        if value not in policy.raw["labels"][key]:
            raise PolicyError(f"invalid {namespace}: {value}")
    return tuple(f"agent/{namespace}:{value}" for namespace, value in requested.items())


def replace_namespaced_labels(
    existing: Iterable[str], replacements: Iterable[str]
) -> list[str]:
    prefixes = tuple(f"agent/{namespace}:" for namespace in NAMESPACES)
    kept = [label for label in existing if not label.startswith(prefixes)]
    result = kept + list(replacements)
    for prefix in prefixes:
        if sum(label.startswith(prefix) for label in result) != 1:
            raise PolicyError(f"expected exactly one label in namespace {prefix}")
    return sorted(set(result))


def parse_command(body: str) -> tuple[str, str] | None:
    for name, pattern in (
        ("approve-plan", PLAN_COMMAND_RE),
        ("run-agent", RUN_COMMAND_RE),
        ("approve-pr", PR_COMMAND_RE),
    ):
        match = pattern.fullmatch(body.strip())
        if match:
            return name, match.group(1)
    return None


def actor_may_approve_plan(actor: str, policy: Policy) -> bool:
    return actor in policy.maintainers


def actor_may_answer(actor: str, issue_author: str, policy: Policy) -> bool:
    return actor == issue_author or actor in policy.maintainers


def is_agent_pr(head_ref: str, head_repository: str, repository: str) -> bool:
    return head_ref.startswith("agent/issue-") and head_repository == repository


def path_is_protected(path: str, policy: Policy) -> bool:
    normalized = _safe_repo_path(path)
    return any(fnmatch.fnmatchcase(normalized, pattern) for pattern in policy.protected_paths)


def can_skip_plan_approval(
    *, blast: str, risk: str, author: str, changed_paths: Iterable[str], policy: Policy
) -> bool:
    return (
        blast == "small"
        and risk == "low"
        and author in policy.trusted_authors
        and not any(path_is_protected(path, policy) for path in changed_paths)
    )


def implementation_order(items: Sequence[Mapping[str, Any]]) -> list[Mapping[str, Any]]:
    return sorted(
        items,
        key=lambda item: (
            PRIORITY_ORDER.get(str(item["priority"]), len(PRIORITY_ORDER)),
            str(item["created_at"]),
            int(item["number"]),
        ),
    )


def may_run_repair(completed_repairs: int, policy: Policy) -> bool:
    return 0 <= completed_repairs < policy.max_repair_runs


def state_transition_allowed(current: str, target: str) -> bool:
    return target == current or target in STATE_TRANSITIONS.get(current, set())


def review_specialist_limit(blast: str, risk: str) -> int:
    if blast == "huge" or risk == "extreme":
        return 6
    if blast == "big" or risk == "high":
        return 4
    if blast == "medium" or risk == "medium":
        return 2
    return REVIEW_LIMITS.get((blast, risk), 0)


def rollback_eligible(
    *, attempt: int, failure_class: str, inverse_clean: bool, rollback_ci_passed: bool
) -> bool:
    return (
        attempt > 1
        and failure_class in {"code", "test", "build"}
        and inverse_clean
        and rollback_ci_passed
    )


def validate_triage(data: Mapping[str, Any], policy: Policy) -> dict[str, Any]:
    required = {"priority", "blast", "risk", "rationale", "missing_information"}
    _exact_keys(data, required)
    namespaced_labels(
        str(data["priority"]), str(data["blast"]), str(data["risk"]),
        "needs-info" if data["missing_information"] else "planning", policy,
    )
    if not isinstance(data["rationale"], str) or not data["rationale"].strip():
        raise PolicyError("triage rationale must be a non-empty string")
    if not isinstance(data["missing_information"], list) or not all(
        isinstance(question, str) and question.strip()
        for question in data["missing_information"]
    ):
        raise PolicyError("missing_information must contain non-empty questions")
    return dict(data)


def validate_plan(data: Mapping[str, Any]) -> dict[str, Any]:
    required = {"revision", "plan", "changed_paths", "tests", "compatibility_notes"}
    _exact_keys(data, required)
    if not isinstance(data["revision"], int) or data["revision"] < 1:
        raise PolicyError("revision must be a positive integer")
    for key in ("plan", "compatibility_notes"):
        if not isinstance(data[key], str) or not data[key].strip():
            raise PolicyError(f"{key} must be a non-empty string")
    for key in ("changed_paths", "tests"):
        if not isinstance(data[key], list) or not all(
            isinstance(value, str) and value.strip() for value in data[key]
        ):
            raise PolicyError(f"{key} must be a list of non-empty strings")
    for path in data["changed_paths"]:
        _safe_repo_path(path)
    return dict(data)


def validate_review(data: Mapping[str, Any], specialist_cap: int) -> dict[str, Any]:
    required = {"verdict", "summary", "findings", "specialists"}
    _exact_keys(data, required)
    if data["verdict"] not in {"pass", "block"}:
        raise PolicyError("review verdict must be pass or block")
    if not isinstance(data["summary"], str) or not data["summary"].strip():
        raise PolicyError("review summary must be non-empty")
    if not isinstance(data["findings"], list):
        raise PolicyError("review findings must be a list")
    if not isinstance(data["specialists"], list) or len(data["specialists"]) > specialist_cap:
        raise PolicyError("review exceeded specialist cap")
    if data["verdict"] == "pass" and data["findings"]:
        raise PolicyError("a passing review cannot contain blocking findings")
    return dict(data)


def validate_patch(patch: Path, root: Path, policy: Policy) -> list[str]:
    """Reject unsafe patch structure before a credentialed job applies it."""
    if patch.stat().st_size > policy.max_patch_bytes:
        raise PolicyError("patch exceeds configured size limit")
    text = patch.read_text(encoding="utf-8")
    if "\nnew file mode 120000\n" in f"\n{text}":
        raise PolicyError("patch creates a symbolic link")
    if "\nnew file mode 160000\n" in f"\n{text}" or "\nSubproject commit " in text:
        raise PolicyError("patch creates or updates a submodule")

    paths: list[str] = []
    for line in text.splitlines():
        match = DIFF_PATH_RE.fullmatch(line)
        if not match:
            continue
        left, right = match.groups()
        if left != right:
            raise PolicyError("renames must retain a canonical repository path")
        paths.append(_safe_repo_path(right))
    if not paths:
        raise PolicyError("patch has no file changes")
    if len(set(paths)) > policy.max_changed_files:
        raise PolicyError("patch changes too many files")
    if ".gitmodules" in paths:
        raise PolicyError("patch may not change submodule configuration")

    resolved_root = root.resolve()
    for path in paths:
        candidate = resolved_root / path
        parent = candidate.parent
        while parent != resolved_root:
            if parent.is_symlink():
                raise PolicyError(f"symlink parent is not allowed: {path}")
            parent = parent.parent
        if candidate.is_symlink():
            raise PolicyError(f"symlink target is not allowed: {path}")

    subprocess.run(
        ["git", "apply", "--check", "--index", str(patch.resolve())],
        cwd=resolved_root,
        check=True,
        stdin=subprocess.DEVNULL,
        env={**os.environ, "GIT_CONFIG_NOSYSTEM": "1"},
    )
    return sorted(set(paths))


def load_json_object(path: Path) -> dict[str, Any]:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise PolicyError(f"invalid JSON in {path}: {error}") from error
    if not isinstance(data, dict):
        raise PolicyError("agent output must be a JSON object")
    return data


def _safe_repo_path(path: str) -> str:
    if "\x00" in path or "\\" in path:
        raise PolicyError(f"unsafe repository path: {path!r}")
    pure = PurePosixPath(path)
    if pure.is_absolute() or not pure.parts or any(part in {"", ".", ".."} for part in pure.parts):
        raise PolicyError(f"unsafe repository path: {path!r}")
    return pure.as_posix()


def _exact_keys(data: Mapping[str, Any], expected: set[str]) -> None:
    actual = set(data)
    if actual != expected:
        raise PolicyError(
            f"expected keys {sorted(expected)}, got {sorted(actual)}"
        )
