#!/bin/bash

# running a flamegraph of the thing

cargo build
sudo dtrace \
    -c './target/debug/goatns' \
    -o out.stacks \
    -n 'profile-997 /execname == "goants"/ { @[ustack(100)] = count(); }'
