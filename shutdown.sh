#!/bin/bash

dig @localhost -p 15353 -c CHAOS shutdown +time=1 +tries=1
