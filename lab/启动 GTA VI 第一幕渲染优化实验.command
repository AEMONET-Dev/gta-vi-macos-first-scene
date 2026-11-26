#!/bin/zsh
set -eu
export VICTORY_PICTURE_EXPERIMENT=1
export VICTORY_SPLIT_LARGE_UPLOADS="${VICTORY_SPLIT_LARGE_UPLOADS:-0}"
exec "${0:A:h}/启动 GTA VI 第一幕材质渲染.command"
