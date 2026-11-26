#!/bin/zsh
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
export VICTORY_HEAP_USAGE=1
export VICTORY_NO_DISK_CACHE=1 VICTORY_HEAP_ONCE_PER_FRAME="${VICTORY_HEAP_ONCE_PER_FRAME:-1}" VICTORY_PIPELINE_WORKERS=6
export VICTORY_PRESENT_PLANE=0 VICTORY_SPD_WAVE32=1
export VICTORY_MATERIAL_LIGHT="${VICTORY_MATERIAL_LIGHT:-1}"
export VICTORY_MATERIAL_EARLY_SCENE="${VICTORY_MATERIAL_EARLY_SCENE:-$VICTORY_MATERIAL_LIGHT}"
export VICTORY_SPATIAL_UPSCALE="${VICTORY_SPATIAL_UPSCALE:-$VICTORY_MATERIAL_EARLY_SCENE}"
export VICTORY_MATERIAL_SURFACES="${VICTORY_MATERIAL_SURFACES:-$VICTORY_MATERIAL_EARLY_SCENE}"
unset VICTORY_PREVIEW_SRV VICTORY_FIDELITYFX_DIR VICTORY_FIXED_EXPOSURE
export VICTORY_KEYS='10:Enter,15:Enter,20:Enter,25:Enter,30:Enter,35:Enter,40:Enter,45:Enter,50:Enter,55:Enter,60:Enter,65:Enter,70:Enter,75:Enter,80:Enter,85:Enter'
print '正在启动第一幕材质渲染版。'
exec /opt/homebrew/bin/python3 "$workspace/picture_shader_session.py"
