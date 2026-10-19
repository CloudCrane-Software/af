#!/usr/bin/env bash
# ClickHouse server 启停（单机，无副本）
set -euo pipefail
CH=/home/pan-ding/ch-mes/bin/clickhouse
CFG=/home/pan-ding/ch-mes/config.xml

start() {
  if curl -sS --max-time 2 http://127.0.0.1:5221/ping 2>/dev/null | grep -q 'Ok.'; then
    echo "already running"; return 0
  fi
  "$CH" server --config-file="$CFG" --daemon
  for i in $(seq 1 60); do
    if curl -sS --max-time 2 http://127.0.0.1:5221/ping 2>/dev/null | grep -q 'Ok.'; then
      echo "server up (http 5221 / tcp 5222)"; return 0
    fi
    sleep 1
  done
  echo "server failed to start; err log tail:"; tail -40 /home/pan-ding/ch-mes/log/server.err.log; return 1
}

stop() {
  PIDS=$(pgrep -f 'clickhouse server --config-file=/home/pan-ding/ch-mes/config.xml' || true)
  if [ -z "$PIDS" ]; then echo "no server process"; else
    kill $PIDS; sleep 2
    PIDS2=$(pgrep -f 'clickhouse server --config-file=/home/pan-ding/ch-mes/config.xml' || true)
    [ -n "$PIDS2" ] && { kill -9 $PIDS2; sleep 1; }
  fi
  # 确认端口释放
  if ss -tln | grep -E ':(5221|5222) ' ; then echo "ERROR: port still listening"; exit 1; else echo "ports 5221/5222 released"; fi
}

status() {
  curl -sS --max-time 2 http://127.0.0.1:5221/ping || true
  echo
  ss -tln | grep -E ':(5221|5222) ' || echo "no listener"
}

case "${1:-}" in
  start) start ;;
  stop) stop ;;
  status) status ;;
  *) echo "usage: $0 start|stop|status"; exit 2 ;;
esac
