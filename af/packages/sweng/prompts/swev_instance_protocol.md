# How to work
1. Locate the relevant code first (grep/read files) and understand the issue before changing anything.
2. Reproduce before fixing when feasible: write or run a small reproduction and confirm it fails for the stated reason.
3. Implement the minimal fix in repository source files only.
4. Verify before submitting: run the repository's own tests for the affected area, or your reproduction; iterate until they pass.
5. Do exactly what the task asks — do not silently expand, shrink or reshape it. If information is ambiguous, first complete everything that does not depend on the answer.

# Hard boundaries
- NEVER modify test files or test configuration (any path containing "test", plus conftest.py, tox.ini, pytest.ini, setup.cfg). The grading harness owns the tests; a patch touching them is rejected.
- Do NOT edit changelogs or release notes (CHANGES.rst, docs/changes*, docs/releases*) — grading ignores documentation; editing it only burns your turn budget.
- The container has NO usable internet access: pip install/download, curl, wget and git remotes fail. Never retry a failed download — implement the fix with what is already in the container.
- A run that ends with no code changes is an automatic failure.
- Every bash command runs in a fresh non-interactive shell starting in /testbed; shell state (cd, exported variables) does NOT persist between commands. Never open editors or interactive prompts; use non-interactive flags (e.g. -y, DEBIAN_FRONTEND=noninteractive). Commands may be killed by a timeout.

# How to finish (the ONLY way)
When your fix is verified:
1. Self-check: run `git diff --stat` and inspect `git diff` restricted to the files you changed; confirm only non-test source files changed and the change is minimal and focused on the issue.
2. Submit: run a bash command that PRINTS the token TASK_COMPLETE as its FIRST output line, e.g. `echo TASK_COMPLETE`.
Mentioning TASK_COMPLETE in plain text does NOT finish the run — only the printed output of an executed command does. After the token prints, the run ends immediately and you cannot continue working.