#!/usr/bin/env python3
"""轮询 F1 run 至终态，落盘 run 终态 JSON 与 jobs（步级结论）JSON 作证据。

用法：poll_run.py <run_id>  （证据写到本目录 run<id>-final.json / run<id>-jobs.json）
"""
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gitea_api import api  # noqa: E402

# gitea 28 的 run.status 终态是 completed，真实结论在 conclusion
# （success/failure/cancelled/skipped/action_required…）
TERMINAL = {"completed", "success", "failure", "cancelled", "skipped"}


def main():
    run_id = sys.argv[1]
    here = os.path.dirname(os.path.abspath(__file__))
    deadline = time.time() + 46 * 60  # workflow timeout-minutes: 45，外加轮询余量
    run, last = {}, None
    while time.time() < deadline:
        st, run = api("GET", f"/api/v1/repos/opchadmin/af/actions/runs/{run_id}")
        cur = run.get("status")
        if cur != last:
            print(f"{time.strftime('%H:%M:%S')} status={cur}", flush=True)
            last = cur
        if cur in TERMINAL:
            break
        time.sleep(20)
    else:
        print("超时未到终态（46min），按现状落盘", flush=True)

    with open(os.path.join(here, f"run{run_id}-final.json"), "w", encoding="utf-8") as f:
        json.dump(run, f, ensure_ascii=False, indent=2)

    st, jobs = api("GET", f"/api/v1/repos/opchadmin/af/actions/runs/{run_id}/jobs")
    with open(os.path.join(here, f"run{run_id}-jobs.json"), "w", encoding="utf-8") as f:
        json.dump(jobs, f, ensure_ascii=False, indent=2)

    print(f"run status/conclusion: {run.get('status')}", flush=True)
    for j in jobs.get("jobs", []):
        print(f"job #{j.get('id')} {j.get('name')}: status={j.get('status')} "
              f"conclusion={j.get('conclusion')} "
              f"started={j.get('started_at')} finished={j.get('finished_at')}", flush=True)
        for s in j.get("steps") or []:
            print(f"  step [{s.get('number')}] {s.get('name')}: "
                  f"status={s.get('status')} conclusion={s.get('conclusion')}", flush=True)


if __name__ == "__main__":
    main()
