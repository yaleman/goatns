#!/bin/bash

# running a flamegraph of the thing

CARGO_PROFILE_RELEASE_DEBUG=true sudo cargo flamegraph --
#./target/debug/goatns