#!/usr/bin/env bash
# 从 VERSIONING.md 的「操作步骤」代码块取出命令，只校验它们存在。
# 不执行这些命令：不编译、不复制、不创建通道目录、不摘指针。

set -euo pipefail

script_dir=$(cd "$(dirname "$0")" && pwd)
doc="$script_dir/../VERSIONING.md"

if [[ ! -f "$doc" ]]; then
  printf 'dry-run: VERSIONING.md missing: %s\n' "$doc" >&2
  exit 1
fi

in_section=0
in_fence=0
declare -a cmds=()
declare -a lines=()

while IFS= read -r line || [[ -n "$line" ]]; do
  if [[ "$in_section" -eq 0 ]]; then
    if [[ "$line" == "## 操作步骤" ]]; then
      in_section=1
    fi
    continue
  fi
  if [[ "$in_fence" -eq 0 ]]; then
    if [[ "$line" == '```sh' ]]; then
      in_fence=1
    elif [[ "$line" == "## "* ]]; then
      break
    fi
    continue
  fi
  if [[ "$line" == '```' ]]; then
    break
  fi
  trimmed="${line#"${line%%[![:space:]]*}"}"
  trimmed="${trimmed%"${trimmed##*[![:space:]]}"}"
  if [[ -z "$trimmed" || "$trimmed" == \#* ]]; then
    continue
  fi
  if [[ "$trimmed" == *'|'* || "$trimmed" == *'&'* || "$trimmed" == *';'* || "$trimmed" == *'`'* ]]; then
    printf 'dry-run: 不支持的复合命令: %s\n' "$trimmed" >&2
    exit 1
  fi
  cmd=${trimmed%%[[:space:]]*}
  cmds+=("$cmd")
  lines+=("$trimmed")
done < "$doc"

if [[ "${#cmds[@]}" -eq 0 ]]; then
  printf 'dry-run: 操作步骤里没有命令\n' >&2
  exit 1
fi

missing=0
for i in "${!cmds[@]}"; do
  cmd=${cmds[$i]}
  step=${lines[$i]}
  if path=$(command -v "$cmd"); then
    printf 'dry-run: %s: %s :: %s\n' "$cmd" "$path" "$step"
  else
    printf 'dry-run: %s: MISSING :: %s\n' "$cmd" "$step" >&2
    missing=1
  fi
done

printf 'dry-run: not-executed\n'
if [[ "$missing" -ne 0 ]]; then
  printf 'dry-run: failed\n' >&2
  exit 1
fi
printf 'dry-run: ok\n'
