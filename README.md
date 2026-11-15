# erm

`erm` is a record/replay HTTP proxy for building reliable mocks around complex, slow, or unreliable upstream services.

The project aims to be simple to operate, easy to inspect, correct in how it captures HTTP traffic, and performant enough to sit in front of real development and test workloads.


## Why

Mocking HTTP services is often easy for simple JSON APIs and painful for everything else: streaming responses, flaky services, expensive calls, unusual headers, large bodies, and timing-sensitive behavior.

`erm` is intended to make those cases practical by recording real traffic once and replaying it later from editable, inspectable files.

## CI acceptance

Require the stable `CI acceptance` status check from GitHub Actions in the `main`
branch protection rule or ruleset. This job runs for pull requests, merge groups,
and pushes to `main`. It succeeds only when every active CI job reports `success`.
Failed, cancelled, skipped, or missing jobs block acceptance.

When adding or removing a job in `.github/workflows/ci.yml`, update
`acceptance.needs` in the same change. Keep job declarations as unquoted IDs at
two-space indentation and the gate's `needs` as an inline list. The checker
rejects unsupported declarations and job-level `continue-on-error` settings.
Existing step anchors remain supported through actionlint's YAML validation.

Run `python3 scripts/check_ci_gate.py` and
`python3 -m unittest discover -s scripts -p 'test_check_ci_gate.py'` to verify
coverage and rejection cases locally. Run `actionlint` to validate the workflow.
