#!/bin/sh
# Capture the README screenshots from scripted scenes (debug build): the
# editor opened on a real shot (RSNAP_DEMO_IMAGE) with the sample marks, and
# the home screen. Only rsnap's own window is captured, at 2x on Retina.
# The images in docs/img come from the renderer itself:
#   cargo run --release -p rsnap-core --example demo -- in.png out.png Dusk rgrip
set -e
cd "$(dirname "$0")/.."
out=docs/screenshots
mkdir -p "$out" target
cargo build -q -p rsnap
swiftc -O scripts/winid.swift -o target/winid 2>/dev/null
shot=${RSNAP_SHOT:-../rjot/docs/screenshots/light.png}

scene() { # name, then env assignments
  name=$1; shift
  ready=$(mktemp -u)
  env RSNAP_DEMO_READY="$ready" "$@" target/debug/rsnap >/dev/null 2>&1 &
  pid=$!
  i=0
  while [ ! -f "$ready" ] && [ $i -lt 300 ]; do sleep 0.1; i=$((i + 1)); done
  target/winid --park 4 4
  sleep 0.4
  wid=$(target/winid $pid | cut -d" " -f1)
  screencapture -x -o -l "$wid" "$out/$name.png"
  echo "$out/$name.png"
  kill $pid 2>/dev/null || true
  wait $pid 2>/dev/null || true
  rm -f "$ready"
}

scene editor RSNAP_DEMO_IMAGE="$shot" RSNAP_DEMO_ANNOTATE=1
scene home
