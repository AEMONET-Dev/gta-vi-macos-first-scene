#!/bin/zsh
set -eu

workspace="${0:A:h}"
game="$workspace/game/game_scarlett_master.exe"
if [[ ! -r "$game" ]]; then
    print -u2 '请先接上 Netac 外接盘，再打开这个启动器。'
    exit 1
fi
if /usr/bin/pgrep -qf '^C:.*victory\.exe run --execute'; then
    print -u2 '测试游戏窗口已经开启，请切回当前窗口。'
    exit 1
fi
run_label="$(/bin/date +%Y%m%d-%H%M%S)"
print '正在启动 GTA VI 的 Series X 兼容性测试。游戏窗口出现后，用回车键确认。'
exec /opt/homebrew/bin/python3 "$workspace/run_test.py" victory run --console series-x --seconds 0 --test-window --label "$run_label"
