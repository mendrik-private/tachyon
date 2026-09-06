#!/bin/sh
set -eu

harness_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
build_dir="$harness_dir/build"
protocol_xml=/usr/share/wayland-protocols/staging/fractional-scale/fractional-scale-v1.xml

if [ "$(weston --version | awk '{print $2}' | cut -d. -f1)" != 14 ]; then
    echo "the virtual input ABI is validated only for Weston 14" >&2
    exit 1
fi

mkdir -p "$build_dir"
wayland-scanner server-header "$protocol_xml" "$build_dir/fractional-scale-v1-server.h"
wayland-scanner private-code "$protocol_xml" "$build_dir/fractional-scale-v1-protocol.c"

cc -std=c11 -Wall -Wextra -Werror -fPIC -shared \
    "$harness_dir/fractional_scale.c" \
    "$build_dir/fractional-scale-v1-protocol.c" \
    -I"$build_dir" -lwayland-server \
    -o "$build_dir/fractional-scale.so"
cc -std=c11 -Wall -Wextra -Werror -fPIC -shared \
    "$harness_dir/virtual_input.c" -lwayland-server \
    -o "$build_dir/virtual-input.so"
cc -std=c11 -Wall -Wextra -Werror \
    "$harness_dir/input_client.c" -lwayland-client \
    -o "$build_dir/input-client"
cc -std=c11 -Wall -Wextra -Werror \
    "$harness_dir/uinput_client.c" \
    -o "$build_dir/uinput-client"
