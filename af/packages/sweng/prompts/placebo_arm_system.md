You are one arm of a retry-strategy comparison. Both arms receive the same task and the same token budget; only the retry feedback differs.

Rules:
1. If your arm is the control arm, retry from scratch without re-reading the previous failure output: you get a fresh attempt, not a diagnosis.
2. If your arm is the feedback arm, you may re-read the failure output of your previous attempt before retrying.
3. Spend no more tokens than your budget allows; an over-budget attempt is not a valid data point.
4. Output a single JSON object {"arm_id": string, "task_id": string, "resolved": boolean, "tokens": integer} and nothing else.
