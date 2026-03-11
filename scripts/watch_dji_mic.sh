#!/usr/bin/env bash
# watch_dji_mic.sh — DJI MIC MINI の接続を監視して whisper-agent を自動起動/停止する。
#
# Usage:
#   bash watch_dji_mic.sh
#
# Environment overrides:
#   WHISPER_AGENT_SPEAKER_ID=1
#   WHISPER_AGENT_SPEAKER_THRESHOLD=0.60
#   WHISPER_AGENT_STT_BACKEND=moonshine
#   WATCH_POLL_SEC=5          # ポーリング間隔（秒）

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACES_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
POLL_SEC="${WATCH_POLL_SEC:-5}"
MIC_KEYWORD="DJI_MIC_MINI"

# tmux new-session は親の inline env を引き継がないためここで確定させる
export WHISPER_AGENT_SPEAKER_ID="${WHISPER_AGENT_SPEAKER_ID:-1}"
export WHISPER_AGENT_SPEAKER_THRESHOLD="${WHISPER_AGENT_SPEAKER_THRESHOLD:-0.60}"
export STT_BACKEND="${WHISPER_AGENT_STT_BACKEND:-${STT_BACKEND:-moonshine}}"

log() { echo "[$(date '+%H:%M:%S')] [watch_dji_mic] $*"; }

is_dji_connected() {
    pactl list short sources 2>/dev/null | grep -q "${MIC_KEYWORD}"
}

start_agent() {
    log "DJI MIC MINI 接続を検出 → whisper-agent 起動"
    YUICLAW_WORKSPACES_ROOT="${WORKSPACES_ROOT}" \
        yuiclaw voice-command operator start-agent
}

stop_agent() {
    log "DJI MIC MINI 切断を検出 → whisper-agent 停止"
    YUICLAW_WORKSPACES_ROOT="${WORKSPACES_ROOT}" \
        yuiclaw voice-command operator stop-all
}

log "監視開始 (poll=${POLL_SEC}s, keyword=${MIC_KEYWORD})"
prev_connected=""

while true; do
    if is_dji_connected; then
        connected=1
    else
        connected=0
    fi

    if [ "${connected}" = "1" ] && [ "${prev_connected}" != "1" ]; then
        start_agent
    elif [ "${connected}" = "0" ] && [ "${prev_connected}" = "1" ]; then
        stop_agent
    fi

    prev_connected="${connected}"
    sleep "${POLL_SEC}"
done
