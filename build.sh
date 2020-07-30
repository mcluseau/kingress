#! /bin/sh
exec docker build --build-arg "GOPROXY=$GOPROXY" "$@" .
