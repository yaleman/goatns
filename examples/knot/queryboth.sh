#!/bin/bash

# shellcheck disable=SC2068
dig @127.0.0.1 -p 15353 $@
# shellcheck disable=SC2068
dig @127.0.0.1 -p 25353 $@