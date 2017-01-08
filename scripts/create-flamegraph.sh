#!/usr/bin/env bash

if [ "$1" == "" ]; then
  echo "must pass rux example to profile"
  exit 1
fi

if [ ! -e ./target/perf ]
then
  mkdir -p ./target/perf
fi

FLAMEGRAPH="$(pwd)/target/perf/FlameGraph"

if [ ! -e $FLAMEGRAPH ]
then
  git clone https://github.com/BrendanGregg/FlameGraph $FLAMEGRAPH || exit 1;
fi

if [ ! -e target/release/examples/$1 ]
then
  cargo build --release --example $1 || exit 1;
fi

SVG="$(pwd)/target/perf/rux_$1_framegraph.svg"

echo "Starting recording of $1.."
sudo perf record -g -F 99 target/release/examples/$1

sudo perf script | ./target/perf/FlameGraph/stackcollapse-perf.pl | ./target/perf/FlameGraph/flamegraph.pl --width 1920 > $SVG

echo "Flame graph created at file://$SVG"
