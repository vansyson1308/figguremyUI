#!/usr/bin/env bash
# Start the desktop app under a virtual X display with its control channel on 127.0.0.1:$1.
# Usage: run_app.sh <port> <binary> [app args...]
# The two X keyboard libraries the app needs are not installed on this machine; they are read
# from an unpacked copy in the scratchpad instead.
set -euo pipefail
PORT="$1"; BIN="$2"; shift 2
XL=/tmp/claude-0/-home-user-figguremyUI/39d89997-e289-5eb4-a16b-402a776ef6a8/scratchpad/xlibs/extracted/usr/lib/x86_64-linux-gnu
# winit looks for the unversioned name.
[ -e "$XL/libxkbcommon-x11.so" ] || ln -s libxkbcommon-x11.so.0 "$XL/libxkbcommon-x11.so"
export LD_LIBRARY_PATH="$XL${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export LIBGL_ALWAYS_SOFTWARE=1
# EGL (for the GL backend) is unpacked there too; point the loader at Mesa's vendor file.
export __EGL_VENDOR_LIBRARY_DIRS="${XL%/usr/lib/x86_64-linux-gnu}/usr/share/glvnd/egl_vendor.d"
export WGPU_BACKEND=${WGPU_BACKEND:-gl}
exec xvfb-run -a -s "-screen 0 1440x900x24" "$BIN" --control "$PORT" "$@"
