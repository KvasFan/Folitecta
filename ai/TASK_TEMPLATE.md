# Task card and test record

Use one requirement of [REQUIREMENTS.md](REQUIREMENTS.md) as the main goal and cut a small,
verifiable sub-task out of it. Fill the fields with observed facts, not guesses. Keep finished
cards in the commit message or in [PROGRESS.md](PROGRESS.md); they are not files of their own.

## Before the change

```yaml
task: FOL-L036/S01            # requirement id + sub-task number (not a new requirement)
requirement: FOL-L036
related: [FOL-L051]
title: Keep the encryption of a protected file when it is saved
request: what the user actually asked for, and its limits

repository: branch, commit before the change, version
existing_changes: uncommitted work found and kept

current_state:                # re-checked on the current code, not copied from the roadmap
  path: interface -> command handler -> core function -> saved file -> reopened file
  evidence: [file:line, test names, self-test steps]
  result: already satisfied | partial | missing | cannot be determined

scope:
  included: []
  excluded: []                # other formats, profiles, signed files, ...
  inputs: []                  # fixtures (synthetic, no personal data)
  expected_result: ""
  kept_structures: []         # links, fields, tags, signatures, metadata, ...
  errors: []                  # what happens on each failure
  gates: []                   # FOL-L018/036/037/051/052 that apply, and why
  local_dependencies: []      # crates, data files (licences!)
  remote_services: []         # empty for local features

planned_tests:
  unit_and_integration: []
  self_test_step: ""
  reopened_file_check: ""
  negative_cases: []
  not_possible_and_why: []
  exit_criterion: ""
```

For a remote feature (FOL-C…) add the connector card: protocol/API and version, endpoint
(configured, never hard-coded), test environment, identity and least privilege, data sent and
secondary destinations, retention, quotas and costs, offline behaviour, remote versions and
conflicts, idempotency and uncertain outcomes, cancelling, output checks, secrets never logged.

## After the change

```yaml
task: FOL-L036/S01
commit: <hash>
build: release build used for the checks
environment: OS, window sizes, relevant versions

cases:
  - case: identifier
    fixture: name (and hash if it is a file)
    expected: written before the test
    obtained: written after the test
    reopened: how the saved file was reopened and checked
    result: pass | fail | not run

checks_run: [cargo test ..., self-test step ..., independent reopen ...]
checks_not_run: [reason]
regressions_checked: []
remaining_limits: []          # profiles still open: they stay in the roadmap
conclusion: limited to the verified profile
```

A command that runs, an HTTP success, a page that looks right or a passing link check is not a
complete functional test: for PDF changes check the saved and reopened file; for security check
data, objects and revisions; for accessibility add the semantic checks; for services check the
remote effect and the uncertain outcomes. Never write "tests passed" for tests only planned.

## States

| Axis | Values |
|---|---|
| Current-state check | not done, in progress, done |
| Result of the check | already satisfied for the profile, partial confirmed, missing in the checked path, cannot be determined |
| Work | not started, defined, in progress, implemented (to be tested), finished for the sub-task |
| Tests | not run, partial, passed for the profile, failed |
| Blocker | none, engine decision, local dependency, hardware, provider contract, credentials or permissions, other (explained) |

When the code changes after a test, the result does not carry over automatically; a regression
reopens the task and keeps its history.
