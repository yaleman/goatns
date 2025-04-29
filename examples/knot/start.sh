#!/bin/bash

# Get the absolute path of the script
BASE_DIR="$( dirname "${BASH_SOURCE[0]}" )"



docker run --rm -it \
    --volume "${BASE_DIR}/config:/config" \
    --volume "type=bind,src=${BASE_DIR}/storage,target=/storage" \
    --volume "type=tmpfs,target=/rundir" \
    -p 25353:25353/tcp \
    -p 25353:25353/udp \
    --name knot \
    cznic/knot:latest \
    knotd