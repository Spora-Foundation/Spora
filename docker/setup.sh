#!/bin/bash

CUR_DIR=$(realpath "$(dirname "${BASH_SOURCE:-$0}")")

function init() {
    pushd "$CUR_DIR"
    export SHELL=/bin/bash
    docker compose up -d --build
    popd
}

init
