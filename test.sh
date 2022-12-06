#!/bin/bash

cnt=0

while true; do
    echo -n "stdout "
    flog -n 1
    echo -n "stderr " >&2
    flog -n 1 >&2
    sleep 0.3
    cnt=$((cnt+1))
    # if [ $cnt -eq 20 ]; then
    #     exit 1
    # fi
done