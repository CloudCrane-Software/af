"""External approval client: sends a message into a waiting workflow via DBOSClient.

Usage: python approve.py <workflow_id> <message> [topic]
"""
import sys

from dbos import DBOSClient

from common import SYSTEM_DB_URL

client = DBOSClient(system_database_url=SYSTEM_DB_URL, dbos_system_schema="dbos")

wf_id = sys.argv[1]
msg = sys.argv[2]
topic = sys.argv[3] if len(sys.argv) > 3 else "approval"
client.send(wf_id, msg, topic)
print(f"[approve] sent to {wf_id} topic={topic} msg={msg!r}")
st = client.get_workflow_status(wf_id)
print(f"[approve] status now: {st.status if st else None}")
