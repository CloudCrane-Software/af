#!/usr/bin/env bash
# Online periodic GC long run: 101 cycles of push -> verify-on-disk -> delete -> wait-for-reclaim
# Long-lived tags kept at cycles 0,20,40,60,80,100 and pulled back at the end.
set -u
BASE=/home/pan-ding/workspace/临时工作区/temp2/af/poc/poc-cas
CRANE=$BASE/../../bin/crane
REG=127.0.0.1:5151
REPO=gclong/img
DATA=$BASE/verify/data-a
BLOBS=$DATA/$REPO/blobs/sha256
LAYERS=$BASE/verify/layers
STATE=$BASE/verify/gc-state.tsv
RESULT=$BASE/verify-logs/gc-loop-result.log
: > "$STATE"
echo "=== GC long run start $(date -Is) ===" >> "$RESULT"

reclaim_count() { ls "$BLOBS" 2>/dev/null | wc -l; }

for i in $(seq 0 100); do
  tag=c$i
  ldir=$LAYERS/l$i
  mkdir -p "$ldir"
  printf 'cycle %d %s\n' "$i" "$(date +%s%N)" > "$ldir/hello.txt"
  tar -cf "$ldir/layer.tar" -C "$ldir" hello.txt

  # 1. push
  out=$("$CRANE" append -f "$ldir/layer.tar" --oci-empty-base -t "$REG/$REPO:$tag" --insecure 2>&1)
  if [ $? -ne 0 ]; then echo "CYCLE $i PUSH FAILED: $out" >> "$RESULT"; exit 1; fi
  mdigest=$(printf '%s' "$out" | grep -o 'sha256:[a-f0-9]*' | tail -1)
  mjson=$("$CRANE" manifest "$REG/$REPO:$tag" --insecure 2>/dev/null)
  cdigest=$(printf '%s' "$mjson" | python3 -c 'import json,sys; print(json.load(sys.stdin)["config"]["digest"])')
  ldigest=$(printf '%s' "$mjson" | python3 -c 'import json,sys; print(json.load(sys.stdin)["layers"][0]["digest"])')

  # 2. confirm blobs on disk
  ondisk=1
  for d in "${mdigest#sha256:}" "${cdigest#sha256:}" "${ldigest#sha256:}"; do
    [ -f "$BLOBS/$d" ] || { ondisk=0; echo "CYCLE $i BLOB NOT ON DISK: $d" >> "$RESULT"; }
  done

  if [ $((i % 20)) -eq 0 ]; then
    echo -e "$i\t$tag\t$mdigest\t$cdigest\t$ldigest\ton_disk=$ondisk\tKEEP" >> "$STATE"
    echo "CYCLE $i KEEP long-lived tag $tag (blobs_on_disk=$(reclaim_count))" >> "$RESULT"
    continue
  fi

  # 3. delete tag
  "$CRANE" delete "$REG/$REPO:$tag" --insecure >/dev/null 2>&1
  if [ $? -ne 0 ]; then echo "CYCLE $i DELETE FAILED" >> "$RESULT"; exit 1; fi
  del_ms=$(date +%s%3N)

  # 4. poll for online reclaim of layer+manifest blobs (config may be shared; checked at end)
  reclaimed=-1
  for t in $(seq 1 45); do
    if [ ! -f "$BLOBS/${mdigest#sha256:}" ] && [ ! -f "$BLOBS/${ldigest#sha256:}" ]; then
      now_ms=$(date +%s%3N); reclaimed=$((now_ms - del_ms)); break
    fi
    sleep 1
  done
  echo -e "$i\t$tag\t$mdigest\t$cdigest\t$ldigest\ton_disk=$ondisk\treclaim_ms=$reclaimed" >> "$STATE"
  if [ "$reclaimed" -eq -1 ]; then
    echo "CYCLE $i NOT RECLAIMED after 45s (manifest=$mdigest layer=$ldigest)" >> "$RESULT"
  fi
done

# 5. final idle wait to let GC settle, then total reclaim check
sleep 30
leftover=$(reclaim_count)
echo "=== LOOP DONE $(date -Is): leftover blob files in repo=$leftover ===" >> "$RESULT"
