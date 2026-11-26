#!/bin/zsh
# 测试版：修补 Rosetta 会卡死的遮罩写入指令，只对 174 MiB 热点缓冲开启写入跟踪。
# 默认启动器（第一幕材质渲染）不受影响。
set -eu
workspace="${0:A:h}"
if [[ ! -r "$workspace/game/game_scarlett_master.exe" ]]; then
    print -u2 '请先接上存有游戏的外接盘。'
    exit 1
fi
if /usr/bin/pgrep -qf '^C:.*victory\.exe run --execute'; then
    print -u2 '已有游戏测试在运行，请先关闭游戏窗口。'
    exit 1
fi
print '正在启动第一幕写入跟踪测试版。'
exec /opt/homebrew/bin/python3 "$workspace/tmp/masked-tracking-084725/bench.py" candidate 0
