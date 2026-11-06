"""Shared helpers for the DBOS PoC: side-effect files, gate files, config."""
import os
import time

BASE = os.path.dirname(os.path.abspath(__file__))
ART = os.path.join(BASE, "artifacts")
GATES = os.path.join(ART, "gates")
os.makedirs(ART, exist_ok=True)
os.makedirs(GATES, exist_ok=True)

# DBOS system database: local Postgres 18.6 (unix socket in /tmp, port 5544)
SYSTEM_DB_URL = "postgresql://dbos@/dbos?host=/tmp&port=5544"


def fx_append(fname: str, line: str) -> None:
    """Append a side-effect record (the 'external system' trace)."""
    with open(os.path.join(ART, fname), "a") as f:
        f.write(f"{line} t={time.time():.3f}\n")
        f.flush()
        os.fsync(f.fileno())


def fx_lines(fname: str) -> list[str]:
    p = os.path.join(ART, fname)
    if not os.path.exists(p):
        return []
    with open(p) as f:
        return [l.rstrip("\n") for l in f if l.strip()]


def gate_wait(name: str, timeout: float = 600.0) -> None:
    """Block until artifacts/gates/<name>.open exists. Pure Python: consumes no DBOS function_id."""
    p = os.path.join(GATES, f"{name}.open")
    deadline = time.time() + timeout
    fx_append("gates.log", f"GATE-ENTER {name} pid={os.getpid()}")
    while not os.path.exists(p):
        if time.time() > deadline:
            raise TimeoutError(f"gate {name} timed out")
        time.sleep(0.2)
    fx_append("gates.log", f"GATE-PASS {name} pid={os.getpid()}")


def gate_open(name: str) -> None:
    with open(os.path.join(GATES, f"{name}.open"), "w") as f:
        f.write(str(time.time()))


def hot_version() -> str:
    """Which code layout wf_hot should run: '1' (original) or '2' (changed)."""
    p = os.path.join(ART, "hotver")
    if os.path.exists(p):
        return open(p).read().strip() or "1"
    return "1"
