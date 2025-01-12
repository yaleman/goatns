#!/bin/bash


docker run --rm -it \
    --volume "$(pwd)/examples/knot/config:/config" \
    --volume "type=bind,src=$(pwd)/examples/knot/storage,target=/storage" \
    --volume "type=tmpfs,target=/rundir" \
    -p 25353:25353/tcp \
    -p 25353:25353/udp \
    --name knot \
    cznic/knot:latest \
    knotd