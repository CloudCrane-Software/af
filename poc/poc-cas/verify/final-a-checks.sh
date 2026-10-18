#!/usr/bin/env bash
# Final A checks: pull long-lived tags, exact blob-set accounting
set -u
B=/home/pan-ding/workspace/临时工作区/temp2/af/poc/poc-cas
CRANE=$B/../../bin/crane
REG=127.0.0.1:5151
REPO=gclong/img
BLOBS=$B/verify/data-a/$REPO/blobs/sha256
OUT=$B/verify-logs/gc-final-checks.log
PULLDIR=$B/verify/pullback
mkdir -p $PULLDIR
: > $OUT

echo "=== 1. pull back all long-lived tags (digest-verified download)" >> $OUT
for tag in c0 c20 c40 c60 c80 c100; do
  if $CRANE pull $REG/$REPO:$tag $PULLDIR/$tag.tar --insecure >> $OUT 2>&1; then
    echo "PULL $tag OK" >> $OUT
  else
    echo "PULL $tag FAILED" >> $OUT
  fi
done
grep -E 'PULL' $OUT

echo "=== 2. expected blob set (union over surviving tags) vs on-disk set" >> $OUT
for tag in c0 c20 c40 c60 c80 c100; do
  $CRANE manifest $REG/$REPO:$tag --insecure 2>/dev/null | python3 -c '
import json,sys
m=json.load(sys.stdin)
print(m["config"]["digest"].split(":")[1])
for l in m["layers"]: print(l["digest"].split(":")[1])
'
  $CRANE digest $REG/$REPO:$tag --insecure 2>/dev/null | cut -d: -f2
done | sort -u > $B/verify/expected-blobs.txt
ls $BLOBS 2>/dev/null | sort -u > $B/verify/ondisk-blobs.txt
echo "expected(unique reachable from 6 tags): $(wc -l < $B/verify/expected-blobs.txt)" >> $OUT
echo "on-disk:                                $(wc -l < $B/verify/ondisk-blobs.txt)" >> $OUT
if diff $B/verify/expected-blobs.txt $B/verify/ondisk-blobs.txt > $B/verify/blobset.diff 2>&1; then
  echo "BLOB SET EXACT MATCH" >> $OUT
else
  echo "BLOB SET MISMATCH:" >> $OUT
  cat $B/verify/blobset.diff >> $OUT
fi
cat $B/verify-logs/gc-final-checks.log
