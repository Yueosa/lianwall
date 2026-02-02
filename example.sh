#!/bin/bash

# ==== 配置区 ====
WALL_DIR="$HOME/Videos/Wallpaper"           # 壁纸视频目录
OUTPUT_LAYER="HDMI-A-1"                     # mpvpaper 输出层
INTERVAL_MIN=10                             # 自动换壁纸间隔（分钟）
MPVPID_FILE="/tmp/mpvpaper_wall.pid"       # 存储 mpvpaper 进程 PID
SCRIPT_PID_FILE="/tmp/mpvpaper_wall_script.pid" # 存储脚本 PID
# =================

# 获取屏幕分辨率
read SCREEN_W SCREEN_H <<< $(hyprctl monitors | grep -A 2 "$OUTPUT_LAYER" | grep -oP "\d+x\d+" | head -n 1 | tr "x" " ")

# 随机选一张 mp4
pick_wallpaper() {
    find "$WALL_DIR" -type f -name "*.mp4" | shuf -n1
}

# 播放壁纸（铺满屏幕，静音，无限循环，不留黑边）
play_wallpaper() {
    local FILE="$1"
    # 杀掉已有的 mpvpaper
    [ -f "$MPVPID_FILE" ] && kill $(cat "$MPVPID_FILE") 2>/dev/null
    
    # 核心优化：
    # 1. -p: 当窗口全屏或遮挡时暂停，极大地节省资源
    # 2. hwdec=auto: 开启显卡硬解
    # 3. video-unscaled=yes 或 video-zoom: 尽量避免用 --vf 滤镜
    
    mpvpaper -p -o "--no-audio --loop=inf --hwdec=auto --video-zoom=0 --panscan=1.0" "$OUTPUT_LAYER" "$FILE" &
    
    echo $! > "$MPVPID_FILE"
}


# 主循环
start_loop() {
    echo $$ > "$SCRIPT_PID_FILE"
    while true; do
        FILE=$(pick_wallpaper)
        play_wallpaper "$FILE"
        sleep $(($INTERVAL_MIN * 60))
    done
}

# 换下一张壁纸
next_wallpaper() {
    FILE=$(pick_wallpaper)
    play_wallpaper "$FILE"
    echo "已换壁纸：$FILE"
}

# 停止脚本
stop_all() {
    [ -f "$MPVPID_FILE" ] && kill $(cat "$MPVPID_FILE") 2>/dev/null
    [ -f "$SCRIPT_PID_FILE" ] && kill $(cat "$SCRIPT_PID_FILE") 2>/dev/null
    rm -f "$MPVPID_FILE" "$SCRIPT_PID_FILE"
    echo "已停止 mpvpaper 和脚本"
}

# ==== 参数处理 ====
case "$1" in
    start)
        echo "开始循环播放壁纸..."
        start_loop
        ;;
    next)
        next_wallpaper
        ;;
    stop)
        stop_all
        ;;
    *)
        echo "用法: $0 {start|next|stop}"
        ;;
esac

