#!/usr/bin/env bash
# Run a build inside a cgroup with a memory ceiling.
#
# rustc on phonix-web legitimately needs ~8.5 GB (measured; it is 45k lines
# with 730 `view!` and 116 `#[server]` expansions). On a 10 GB VM that leaves
# nothing, and on 2026-09-04 a second memory user turned it into a swap storm:
# all 12 GB of swap went to zero, the kernel OOM killer fired too late, and the
# session was lost.
#
# Inside a scope the ceiling is reached first. The kernel kills that rustc,
# cargo reports a failed build, and the machine stays up.
#
#     tools/capped-build.sh cargo leptos build
#     PHONIX_MEM_MAX=6G tools/capped-build.sh cargo test --workspace
set -euo pipefail

if [ $# -eq 0 ]; then
    echo "usage: ${0##*/} <command> [args...]" >&2
    exit 64
fi

# Swap is capped hardest: the failure mode here is a swap storm, not a clean
# OOM. 2 GB is enough to ride out a spike and too little to thrash for minutes.
exec systemd-run --user --scope -q --collect \
    -p MemoryHigh="${PHONIX_MEM_HIGH:-8.8G}" \
    -p MemoryMax="${PHONIX_MEM_MAX:-9.2G}" \
    -p MemorySwapMax="${PHONIX_MEM_SWAP_MAX:-2G}" \
    -- "$@"
