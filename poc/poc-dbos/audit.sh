#!/usr/bin/env bash
# Audit queries against the DBOS system tables (Postgres 18.6 @ /tmp socket, port 5544)
PSQL="/usr/lib/postgresql/18/bin/psql -h /tmp -p 5544 -U dbos -d dbos"
case "${1:-all}" in
  tables)
    $PSQL -c "select table_name from information_schema.tables where table_schema='dbos' order by table_name" ;;
  wf)
    $PSQL -c "select workflow_uuid, status, name, recovery_attempts, executor_id, app_version, to_timestamp(created_at/1000) as created, to_timestamp(updated_at/1000) as updated, left(output,60) as output, left(coalesce(error,''),60) as error from dbos.workflow_status order by created_at" ;;
  steps)
    $PSQL -c "select workflow_uuid, function_id, function_name, to_timestamp(started_at_epoch_ms/1000) as started, to_timestamp(completed_at_epoch_ms/1000) as completed, left(output,50) as output, left(coalesce(error,''),50) as error from dbos.operation_outputs order by workflow_uuid, function_id" ;;
  wf-inputs)
    $PSQL -c "select workflow_uuid, left(inputs,80) as inputs from dbos.workflow_input order by workflow_uuid" ;;
  notif)
    $PSQL -c "select destination_uuid, topic, left(message,40) as message, to_timestamp(created_at_epoch_ms/1000) as created, consumed from dbos.notifications order by created_at_epoch_ms" ;;
  events)
    $PSQL -c "select workflow_uuid, key, left(value,40) as value from dbos.workflow_events order by workflow_uuid" ;;
  *)
    echo "usage: audit.sh {tables|wf|steps|wf-inputs|notif|events}" ;;
esac
