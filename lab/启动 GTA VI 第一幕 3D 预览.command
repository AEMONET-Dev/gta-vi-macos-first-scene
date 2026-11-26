#!/bin/zsh
set -eu

workspace="${0:A:h}"
if [[ ! -r "$workspace/game/game_scarlett_master.exe" ]]; then
    print -u2 '请先接上存有游戏的外接盘，再打开这个启动器。'
    exit 1
fi
if /usr/bin/pgrep -qf '^C:.*victory\.exe run --execute'; then
    print -u2 '游戏测试进程已经启动，请先关闭它，再打开第一幕预览。'
    exit 1
fi

export VICTORY_NO_DISK_CACHE=1
export VICTORY_PIPELINE_WORKERS=6
export VICTORY_PRESENT_PLANE=1
export VICTORY_PREVIEW_SRV=fdce2a0a9fb1521a:1:24
label="scene-preview-$(/bin/date +%Y%m%d-%H%M%S)"
print '启动第一幕场景预览。进入第一幕后，每张画面可能停留十几秒。'
exec /opt/homebrew/bin/python3 "$workspace/run_test.py" victory run --gptk4 \
    --console series-s --test-window --victory-dir 'victory\color-trace' --seconds 0 --label "$label"
