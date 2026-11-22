#!/usr/bin/env bash
# Dev runner: lima-server (rebuilt on change by cargo watch) + Vite UI with hot reload.
# Running it again replaces a previous instance.
#
#   UI:  http://<host>:5173   (proxies /api and /media to the server)
#   API: http://<host>:6767
set -u
cd "$(dirname "$0")/.."

PORTS=(6767 5173)

port_pids() {
  ss -ltnpH "sport = :$1" | grep -o 'pid=[0-9]*' | cut -d= -f2 | sort -u
}

ports_busy() {
  local p
  for p in "${PORTS[@]}"; do
    [ -n "$(port_pids "$p")" ] && return 0
  done
  return 1
}

# Stops a previous dev run. Killing its vite/lima-server makes the old runner
# exit and clean up the rest (cargo watch). Anything else on our ports is left alone.
stop_previous() {
  local p pid cmd
  ports_busy || return 0
  for p in "${PORTS[@]}"; do
    for pid in $(port_pids "$p"); do
      cmd=$(ps -o args= -p "$pid" 2>/dev/null) || continue
      case "$cmd" in
        *lima-server* | *vite*)
          echo "Stopping previous dev process $pid: $cmd"
          kill -TERM "$pid" 2>/dev/null
          ;;
        *)
          echo "Port $p is used by an unrelated process, not touching it: $cmd" >&2
          exit 1
          ;;
      esac
    done
  done
  for _ in $(seq 50); do
    ports_busy || return 0
    sleep 0.2
  done
  echo "Previous dev processes did not release ports ${PORTS[*]}" >&2
  exit 1
}

stop_previous

# Only stop our own children on exit (never the whole process group).
cleanup() {
  trap - EXIT
  kill $(jobs -p) 2>/dev/null
  wait 2>/dev/null
}
trap cleanup EXIT
trap 'exit 130' INT TERM

cargo watch -q -w crates -w migrations -x 'run -p lima-server' &

(
  cd ui &&
    pnpm install --frozen-lockfile --silent &&
    # exec so the job pid is vite itself and it receives our TERM directly.
    exec ./node_modules/.bin/vite --host 0.0.0.0 --port 5173 --strictPort
) &

# If either side dies, stop the other one too.
wait -n
