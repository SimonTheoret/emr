# Agent-centric GitHub workflow

This repository uses GitHub issues as the durable state and audit log for an Actions-only Codex maintenance workflow. Agents never receive a GitHub write credential. A later deterministic job validates their structured output or patch before making GitHub changes.

## Lifecycle

Every managed issue has exactly one label from each namespace:

- `agent/priority:{low,medium,high,extreme}` controls scheduling. The dispatcher orders extreme, high, medium, then low, with FIFO ordering inside a priority.
- `agent/blast:{small,medium,big,huge}` describes how broadly behavior may change.
- `agent/risk:{low,medium,high,extreme}` describes implementation and operational risk.
- `agent/state:*` records the current lifecycle state.

The normal state flow is:

```text
needs-triage -> needs-info -> planning -> awaiting-approval -> queued
                                                        queued -> implementing -> reviewing -> done
                                                                            \-> repairing --/
Any active state may be stopped at blocked by its owning controller.
```

The triage rubric is deliberately conservative:

| Dimension | Low/small | Medium | High/big | Extreme/huge |
| --- | --- | --- | --- | --- |
| Priority | backlog | normal | workflow or release blocker | active incident |
| Blast | localized | multiple internals | public or core behavior | repository or ecosystem-wide |
| Risk | reversible and tested | contained behavior | compatibility, concurrency, persistence, or security | irreversible or severe compromise |

When triage asks questions, only the issue author or a configured maintainer can resume processing. Their answer causes a fresh triage run; text from all issue authors and comments remains untrusted model input.

## Plans and commands

A planning agent posts a versioned, decision-complete plan. Its 16-character plan ID is the SHA-256 prefix of the revision, normalized plan text, and current `main` SHA. Any edit to the plan or movement of `main` therefore invalidates an old approval.

Plan approval is skipped only when all of these are true:

- blast radius is small;
- risk is low;
- the author is in `trusted_authors`; and
- no planned path matches `.github/**`, `.agents/**`, or `AGENTS.md`.

Otherwise `SimonTheoret` must comment:

```text
/approve-plan <plan-id>
```

An untrusted author can explicitly admit a plan with `/run-agent <plan-id>`, but this does not replace a required maintainer plan approval. Approving a gated plan also records admission. Only exact, whole-comment commands are accepted.

For a maintainer-authored pull request, the maintainer can replace a GitHub review with:

```text
/approve-pr <40-character-current-head-sha>
```

The command is rejected for contributor-authored pull requests and after the head changes.

## Scheduling, implementation, and repair

The serialized dispatcher fills at most four slots, using priority and FIFO ordering. Central policy is in [`.github/agent-policy.toml`](../.github/agent-policy.toml). Model, reasoning effort, action SHA, Codex version, actors, protected paths, patch limits, and repair limit are changed only through an approved protected-path pull request.

An implementation agent receives an exact base SHA and works in a checkout with workspace permission, no GitHub write permission, and no GitHub write token. Its complete binary patch and structured test evidence cross the job boundary as a one-day artifact. The publisher then:

1. checks out the exact approved base with persisted credentials disabled;
2. rejects oversized or malformed patches, traversal, renames with ambiguous paths, symlinks, submodules, `.gitmodules`, symlink-parent escapes, unplanned paths, and unapproved protected paths;
3. runs `git apply --check` and applies the patch without running repository code or hooks;
4. creates or updates `agent/issue-<number>-<slug>`; and
5. opens a PR with the issue, plan ID, base, repair number, test evidence, and `Closes #<issue>`.

CI or agent-review failures are consolidated into one repair request per head SHA. There are at most three repair runs after the initial implementation. A fourth failure changes the issue to `blocked` and mentions the maintainer.

GitHub suppresses ordinary workflow events caused by the repository `GITHUB_TOKEN`. After the publisher creates or updates a bot PR, it therefore sends an `agent-ci` repository dispatch. The trusted default-branch CI workflow checks out the exact head SHA in credential-free verification jobs, then a separate job verifies that SHA still belongs to the expected bot-authored PR and publishes the required statuses. It never loads workflow YAML from the generated branch.

## CI, review, and merge

Pull requests run these required checks:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`, including workflow policy unit tests
- `cargo build --all-targets`
- `nix build`
- `rollback-validate` (a no-op except on recovery PRs)
- `agent-review`
- the `merge-policy` commit status

Pushes to `main` repeat Cargo and Nix checks and additionally build `.#dockerImage`. No artifact is published.

For a same-repository `agent/issue-*` PR, `agent-review` receives a fresh context consisting only of the ticket, approved plan metadata, PR diff, checked-out code, and repository instructions. It receives no implementation transcript. The specialist cap is zero for small/low, two for medium, four for big/high, and six for huge/extreme. A blocking verdict fails the check and triggers repair; a clean verdict requests review from `SimonTheoret`. Non-agent and fork PRs run normal CI but skip the agent lifecycle.

`merge-policy` verifies either a `SimonTheoret` approval attached to the current head or a valid maintainer-authored `/approve-pr` command. It also queries every review thread and fails while a conversation is unresolved. Rollback PRs use deterministic inverse-tree validation in place of human approval. On success the controller enables squash auto-merge; GitHub branch protection holds the merge until all required checks pass.

## Authentication and security boundaries

Codex uses GitHub-to-OpenAI workload identity federation, not a stored API key. Create these non-secret Actions variables:

| Variable | Meaning |
| --- | --- |
| `OPENAI_WIF_AUDIENCE` | Exact audience accepted by the OpenAI OIDC provider |
| `OPENAI_READ_FEDERATION_RULE_ID` | Codex federation rule for triage, planning, and review |
| `OPENAI_IMPLEMENT_FEDERATION_RULE_ID` | Separate Codex federation rule for implementation |

In the OpenAI Admin Portal, use dedicated least-privilege service accounts. Require the issuer `https://token.actions.githubusercontent.com`, repository `SimonTheoret/emr`, the configured audience, and exact `workflow_ref` values on `refs/heads/main`:

- read rule: `.github/workflows/agent-triage.yml`, `.github/workflows/agent-plan.yml`, and `.github/workflows/ci.yml`;
- implementation rule: `.github/workflows/agent-implement.yml` only.

The workflow requests an OIDC token only in a trusted preparation step of a job with `id-token: write`, writes it to a mode-0600 directory outside the checkout, denies that directory in the selected Codex filesystem profile, and supplies `OPENAI_FEDERATION_RULE_ID` plus `OPENAI_IDENTITY_TOKEN_FILE`. The Codex step explicitly blanks GitHub's OIDC request variables, and the shell-environment policy excludes both GitHub and OpenAI launch credentials from model-controlled child processes. The official Codex Action is commit-pinned and uses `drop-sudo`. All other external actions are also full-SHA pinned.

Credentialed publisher jobs never run generated repository code. They check out trusted `main` controller scripts or an approved base, validate data, disable Git hooks, and expose `GITHUB_TOKEN` only to the publication step. Checkouts use `persist-credentials: false`.

## Main failure recovery

After a failed `main` CI run, `Main recovery` reruns failed jobs once. A repeated failure in a verification step is classified as code/build/test failure; setup, checkout, missing-job, and other unknown failures are treated as infrastructure failures and only alert the maintainer.

For a repeated deterministic failure, recovery checks out the controller from the pre-failure parent, creates a new `agent/revert-<sha>` PR containing `git revert` of the squash commit, and verifies that the resulting tree exactly equals the failed commit's parent tree. CI independently repeats that inverse-tree test. The rollback PR is auto-merged after required checks. The original PR remains closed. After merge, the original issue is reopened, supplied with failure evidence, returned to planning (because the base changed), and dispatched toward a replacement implementation. Conflicts, a non-exact inverse, or rollback CI failure stop automation and notify the maintainer.

## Initial setup and rollout

1. Confirm the repository name in `agent-policy.toml`, configure the three Actions variables, and create the two exact-claim OpenAI federation rules.
2. Run `Agent administration` once to create/update all labels before enabling issue forms.
3. Enable repository auto-merge, automatic head-branch deletion, and the Actions setting that allows GitHub Actions to create pull requests.
4. Protect `main`: block direct pushes, force pushes, and deletion; require pull requests, resolved conversations, and the checks listed above. Do not allow bypasses for the Actions bot.
5. Validate in a disposable public repository before enabling `full` rollout.

Rollout is controlled by `rollout.stage`:

- `shadow-triage` posts triage and stops;
- `planning` adds plans and approvals but does not dispatch implementations;
- `full` enables implementation, review, merge, and recovery.

Disposable-repository acceptance tests must cover clarification pause/resume, trusted small/low auto-approval, protected or risky approval, untrusted-author admission, malformed agent JSON, stale and unauthorized commands, prompt-injection fixtures, fork PRs, CI and review repairs, repair exhaustion, current-head maintainer approval, a repeated main failure, exact revert, issue reopening, and replacement planning.

## Operator recovery

- Missing or rejected OIDC claims: compare issuer, audience, repository, and `workflow_ref` locally without logging the raw token. Confirm the correct federation-rule variable is present.
- Stale plan: rerun planning; never reuse the old plan ID.
- Blocked issue: inspect the audit comments and open a revised plan before returning it to `queued`.
- Publisher rejection: treat the patch as untrusted evidence. Do not apply it manually in the credentialed job; revise the plan or implementation.
- Rollback conflict or failed rollback CI: leave the rollback PR open, disable auto-merge for it, and resolve under maintainer control.

## References

- [OpenAI: using skills and GitHub Actions for OSS maintenance](https://developers.openai.com/blog/skills-agents-sdk)
- [OpenAI: GitHub Actions workload identity federation](https://developers.openai.com/api/docs/guides/workload-identity-federation/github-actions)
- [OpenAI: Codex federation rule reference](https://developers.openai.com/api/docs/guides/workload-identity-federation/federation-rules)
- [GitHub: reverting a pull request](https://docs.github.com/en/pull-requests/how-tos/merge-and-close-pull-requests/reverting-a-pull-request)
