# erm

`erm` is a record/replay HTTP proxy for building reliable mocks around complex, slow, or unreliable upstream services.

The project aims to be simple to operate, easy to inspect, correct in how it captures HTTP traffic, and performant enough to sit in front of real development and test workloads.


## Why

Mocking HTTP services is often easy for simple JSON APIs and painful for everything else: streaming responses, flaky services, expensive calls, unusual headers, large bodies, and timing-sensitive behavior.

`erm` is intended to make those cases practical by recording real traffic once and replaying it later from editable, inspectable files.

## Automation

Repository maintenance uses an issue-driven, credential-separated Codex workflow. See [docs/agent-workflow.md](docs/agent-workflow.md) for lifecycle states, approval commands, CI/merge policy, security boundaries, setup, and recovery.

## Local issue workflow

Run the issue-to-PR workflow locally with authenticated `gh` and `codex` CLIs:

```sh
python3 scripts/local_agent_workflow.py 123
```

The script triages the issue, proposes a plan for approval, creates a dedicated
worktree, implements and validates the change, performs a fresh Codex review, and
asks again before committing, pushing, and opening a pull request. Use `--quick`
to skip Nix and Docker builds, or `--no-publish` to stop with local reviewed changes.
