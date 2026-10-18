"""DBOS PoC app: workflows + steps, serve/recover process.

Usage:
  python app.py serve <spec> [<spec>...] [--wait N]
      spec = <workflow_func>:<tag>            e.g. wf_3step:crash1
  python app.py wait [seconds]   # host + recover pending workflows, then exit
  python app.py status           # print workflow_status rows
All workflows park at a gate file until artifacts/gates/<name>.open exists,
so the harness controls kill timing; open gates before starting for baseline runs.
"""
import os
import sys
import time

from dbos import DBOS, DBOSConfig, SetWorkflowID

from common import SYSTEM_DB_URL, fx_append, gate_wait, hot_version

config: DBOSConfig = {
    "name": "poc-dbos",
    "system_database_url": SYSTEM_DB_URL,
}
_v = os.environ.get("POC_APP_VERSION")
if _v:
    config["application_version"] = _v  # pin version (else auto MD5 of workflow sources)
dbos = DBOS(config=config)


# ---------- steps ----------

@DBOS.step()
def step_a(tag: str, label: str) -> str:
    """Generic step: appends an EXEC line to the side-effect log."""
    fx_append("fx.log", f"EXEC {tag} {label} pid={os.getpid()}")
    time.sleep(0.2)
    suffix = "-v2" if (label == "hot-s1" and hot_version() == "2") else ""
    return f"{label}-ok{suffix}"


@DBOS.step()
def step_charge(tag: str) -> str:
    """Step with 3 sub-appends 0.6s apart, so it can be killed midway."""
    for i in range(1, 4):
        fx_append("fx.log", f"CHARGE {tag} line={i}/3 pid={os.getpid()}")
        time.sleep(0.6)
    return "charged"


@DBOS.step()
def step_new_hot(tag: str) -> str:
    """Step that only exists in code version 2 (inserted between s1 and s2)."""
    fx_append("fx.log", f"EXEC {tag} hot-s1b-NEW pid={os.getpid()}")
    return "hot-s1b-new-ok"


@DBOS.step()
def step_boom(tag: str) -> str:
    fx_append("fx.log", f"BOOM {tag} pid={os.getpid()}")
    raise RuntimeError(f"intentional failure in {tag}")


# ---------- workflows ----------

@DBOS.workflow()
def wf_3step(tag: str) -> str:
    step_a(tag, "step1")
    step_a(tag, "step2")
    gate_wait(f"{tag}.after-step2")
    step_a(tag, "step3")
    return f"{tag}-done"


@DBOS.workflow()
def wf_charge(tag: str) -> str:
    step_a(tag, "step1")
    r = step_charge(tag)
    step_a(tag, "step3")
    return f"{tag}-done charge={r}"


@DBOS.workflow()
def wf_timer(tag: str) -> str:
    step_a(tag, "before-sleep")
    gate_wait(f"{tag}.pre-sleep")
    DBOS.sleep(3)
    step_a(tag, "after-sleep")
    return f"{tag}-timer-done"


@DBOS.workflow()
def wf_approve(tag: str) -> str:
    step_a(tag, "received")
    fx_append("fx.log", f"APPROVAL-WAIT {tag} pid={os.getpid()}")
    msg = DBOS.recv("approval", timeout_seconds=180)
    fx_append("fx.log", f"APPROVAL-RECV {tag} msg={msg!r}")
    if isinstance(msg, str) and msg.startswith("approved"):
        who = msg.split(":", 1)[1] if ":" in msg else "?"
        step_a(tag, f"settled-by={who}")
        return f"{tag}-approved-by={who}"
    return f"{tag}-rejected({msg!r})"


@DBOS.workflow()
def wf_fail(tag: str) -> str:
    step_a(tag, "ok")
    step_boom(tag)
    return f"{tag}-unreachable"


@DBOS.workflow()
def wf_hot(tag: str) -> str:
    """Layout depends on hot_version(): v1 = s1,s2,[gate],s3; v2 inserts a new
    step between s1/s2 and changes s1's return value + s3's label."""
    v = hot_version()
    s1 = step_a(tag, "hot-s1")
    s1b = "n/a"
    if v == "2":
        s1b = step_new_hot(tag)  # NEW step, shifts function ids after it
    s2 = step_a(tag, "hot-s2")
    gate_wait(f"{tag}.hot-gate")
    label3 = "hot-s3-CHANGED" if v == "2" else "hot-s3"
    s3 = step_a(tag, label3)
    return f"{tag}-hot-done s1={s1} s1b={s1b} s2={s2} s3={s3}"


WORKFLOWS = {
    "wf_3step": wf_3step,
    "wf_charge": wf_charge,
    "wf_timer": wf_timer,
    "wf_approve": wf_approve,
    "wf_fail": wf_fail,
    "wf_hot": wf_hot,
}

TERMINAL = ("SUCCESS", "ERROR", "CANCELLED")


def main() -> None:
    cmd = sys.argv[1]
    dbos.launch()  # schema migration + recovery of PENDING workflows

    if cmd == "serve":
        rest = sys.argv[2:]
        wait_s = 240.0
        if "--wait" in rest:
            i = rest.index("--wait")
            wait_s = float(rest[i + 1])
            rest = rest[:i] + rest[i + 2 :]
        handles = []
        for spec in rest:
            fname, tag = spec.split(":")
            fn = WORKFLOWS[fname]
            wf_id = f"{fname}-{tag}"
            with SetWorkflowID(wf_id):
                h = DBOS.start_workflow(fn, tag)
            fx_append("fx.log", f"START {wf_id} pid={os.getpid()}")
            handles.append((wf_id, h))
            print(f"[app] started {wf_id}", flush=True)
        deadline = time.time() + wait_s
        while time.time() < deadline:
            if all(h.get_status().status in TERMINAL for _, h in handles):
                break
            time.sleep(0.5)
        for wf_id, h in handles:
            st = h.get_status()
            print(f"[app] {wf_id} status={st.status} error={st.error} result={h.get_result()!r}", flush=True)
        dbos.destroy()
        sys.exit(0)

    if cmd == "wait":
        wait_s = float(sys.argv[2]) if len(sys.argv) > 2 else 240.0
        deadline = time.time() + wait_s
        # poll the system table until every workflow is terminal
        # (recovery re-enqueues: status goes PENDING -> ENQUEUED -> PENDING, so
        #  waiting on PENDING=0 alone would exit before the recovered run finishes)
        import subprocess

        while time.time() < deadline:
            out = subprocess.run(
                ["/usr/lib/postgresql/18/bin/psql", "-h", "/tmp", "-p", "5544", "-U", "dbos",
                 "-d", "dbos", "-tAc",
                 "select count(*) from dbos.workflow_status "
                 "where status not in ('SUCCESS','ERROR','CANCELLED')"],
                capture_output=True, text=True).stdout.strip()
            if out == "0":
                break
            time.sleep(0.5)
        print(f"[app] wait done: all workflows terminal (or timeout)", flush=True)
        dbos.destroy()
        sys.exit(0)

    if cmd == "status":
        import subprocess

        out = subprocess.run(
            ["/usr/lib/postgresql/18/bin/psql", "-h", "/tmp", "-p", "5544", "-U", "dbos",
             "-d", "dbos", "-c",
             "select workflow_uuid, status, name, recovery_attempts, created_at, updated_at "
             "from dbos.workflow_status order by created_at"],
            capture_output=True, text=True)
        print(out.stdout)
        dbos.destroy()
        sys.exit(0)

    print(f"unknown cmd {cmd}", file=sys.stderr)
    sys.exit(2)


if __name__ == "__main__":
    main()
