#!/bin/bash
# ===========================================================
#   LianWall Installation Script
#   https://github.com/Yueosa/lianwall
# ===========================================================
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/Yueosa/lianwall/main/install.sh | bash
#
# Or with custom install directory:
#   curl -fsSL https://raw.githubusercontent.com/Yueosa/lianwall/main/install.sh | bash -s -- --prefix ~/.local/bin
#
# ===========================================================
#
# 脚本做的事：
# 1. 检测系统架构 (只支持 x86_64 Linux)
# 2. 从 GitHub Release 下载两个二进制
# 3. 安装到 ~/.local/bin/（默认）
# 4. 提醒用户添加 PATH（如果需要）
# 5. 检查依赖（swww, mpvpaper）
#
# 安装后的目录结构：
# ~/.local/bin/
# ├── lianwall      # CLI
# └── lianwalld     # Daemon
#
# ===========================================================

set -e

# =========================
# 🎨 颜色定义
# =========================
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

info()    { echo -e "${CYAN}ℹ️  $*${NC}"; }
success() { echo -e "${GREEN}✅ $*${NC}"; }
warn()    { echo -e "${YELLOW}⚠️  $*${NC}"; }
error()   { echo -e "${RED}❌ $*${NC}" >&2; }

# =========================
# 📦 配置
# =========================
VERSION="5.0.0"
REPO="Yueosa/lianwall"
INSTALL_DIR="${HOME}/.local/bin"

# 解析参数
while [[ $# -gt 0 ]]; do
    case "$1" in
        --prefix)
            INSTALL_DIR="$2"
            shift 2
            ;;
        --system)
            INSTALL_DIR="/usr/local/bin"
            shift
            ;;
        --version)
            VERSION="$2"
            shift 2
            ;;
        -h|--help)
            echo "LianWall Installer"
            echo ""
            echo "Usage: install.sh [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --prefix DIR    Install to custom directory (default: ~/.local/bin)"
            echo "  --system        Install to /usr/local/bin (requires sudo)"
            echo "  --version VER   Install specific version (default: ${VERSION})"
            echo "  -h, --help      Show this help"
            exit 0
            ;;
        *)
            error "Unknown option: $1"
            exit 1
            ;;
    esac
done

# =========================
# 🔍 检测系统
# =========================
ARCH=$(uname -m)
if [[ "$ARCH" != "x86_64" ]]; then
    error "Unsupported architecture: $ARCH. Only x86_64 is supported."
    exit 1
fi

OS=$(uname -s)
if [[ "$OS" != "Linux" ]]; then
    error "Unsupported OS: $OS. Only Linux is supported."
    exit 1
fi

# =========================
# 📥 下载安装
# =========================
BASE_URL="https://github.com/${REPO}/releases/download/v${VERSION}"

echo ""
info "LianWall Installer v${VERSION}"
info "Install directory: ${INSTALL_DIR}"
echo ""

# 创建目录
mkdir -p "$INSTALL_DIR"

# 检查是否需要 sudo
SUDO=""
if [[ ! -w "$INSTALL_DIR" ]]; then
    if command -v sudo &> /dev/null; then
        SUDO="sudo"
        warn "Install directory requires elevated privileges"
    else
        error "Cannot write to $INSTALL_DIR and sudo is not available"
        exit 1
    fi
fi

# 下载函数
download() {
    local name=$1
    local url="${BASE_URL}/${name}_${VERSION}_linux_x86_64"
    local dest="${INSTALL_DIR}/${name}"
    
    info "Downloading ${name}..."
    
    if command -v curl &> /dev/null; then
        $SUDO curl -fsSL "$url" -o "$dest"
    elif command -v wget &> /dev/null; then
        $SUDO wget -q "$url" -O "$dest"
    else
        error "Neither curl nor wget found. Please install one of them."
        exit 1
    fi
    
    $SUDO chmod +x "$dest"
    success "Installed ${name} to ${dest}"
}

# 下载两个二进制
download "lianwalld"
download "lianwall"

# =========================
# ✅ 完成
# =========================
echo ""
success "Installation complete!"
echo ""

# 检查 PATH
if [[ ":$PATH:" != *":${INSTALL_DIR}:"* ]]; then
    warn "Note: ${INSTALL_DIR} is not in your PATH"
    echo ""
    echo "Add this line to your ~/.bashrc or ~/.zshrc:"
    echo ""
    echo "  export PATH=\"\$PATH:${INSTALL_DIR}\""
    echo ""
fi

# 检查依赖
echo "Checking dependencies..."
MISSING_DEPS=""

if ! command -v swww &> /dev/null; then
    MISSING_DEPS="${MISSING_DEPS} swww"
fi

if ! command -v mpvpaper &> /dev/null; then
    MISSING_DEPS="${MISSING_DEPS} mpvpaper"
fi

if [[ -n "$MISSING_DEPS" ]]; then
    warn "Missing optional dependencies:${MISSING_DEPS}"
    echo "  - swww: required for image wallpapers"
    echo "  - mpvpaper: required for video wallpapers"
    echo ""
fi

echo "Quick start:"
echo "  1. Start daemon:  lianwall start"
echo "  2. Check status:  lianwall status"
echo "  3. Next wallpaper: lianwall next"
echo ""
echo "For more info: lianwall --help"
