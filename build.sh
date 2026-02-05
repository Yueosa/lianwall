#!/bin/bash
# ===========================================================
# 做了这些事
# 1. 从 Cargo.toml 读取版本号
# 2. cargo build --release 编译两个包
# 3. 复制产物到 build/5.0.0/ 目录
# 4. 生成 sha256 校验和
# ===========================================================
# 产出文件：
# build/5.0.0/
# ├── lianwall_5.0.0_linux_x86_64      # CLI
# ├── lianwalld_5.0.0_linux_x86_64     # Daemon  
# └── checksums_5.0.0.txt              # 校验和
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
error()   { echo -e "${RED}❌ $*${NC}"; }

# =========================
# 📦 版本信息
# =========================
# 从 workspace Cargo.toml 读取版本（daemon 版本）
DAEMON_VERSION=$(grep -A5 '\[workspace.package\]' Cargo.toml | grep '^version' | head -1 | sed -E 's/version = "(.*)"/\1/')

# 如果失败，从 daemon 的 Cargo.toml 读取
if [[ -z "$DAEMON_VERSION" ]]; then
    DAEMON_VERSION=$(grep '^version' crates/lianwall-daemon/Cargo.toml | head -1 | sed -E 's/version = "(.*)"/\1/')
fi

# CLI 和 daemon 使用相同版本（5.0.0）
VERSION="${DAEMON_VERSION:-5.0.0}"

info "Building LianWall v${VERSION}"
echo ""

# =========================
# 🔨 编译
# =========================
DEST="build/${VERSION}"
mkdir -p "$DEST"

info "Building release binaries..."

cargo build --release --package lianwall-cli --package lianwall-daemon

# 复制产物
cp target/release/lianwall "$DEST/lianwall_${VERSION}_linux_x86_64"
cp target/release/lianwalld "$DEST/lianwalld_${VERSION}_linux_x86_64"

success "Build complete!"
echo ""

# =========================
# 📊 结果
# =========================
echo "Artifacts in ${DEST}:"
ls -lh "$DEST"
echo ""

# 生成 sha256
info "Generating checksums..."
cd "$DEST"
sha256sum "lianwall_${VERSION}_linux_x86_64" "lianwalld_${VERSION}_linux_x86_64" > "checksums_${VERSION}.txt"
cat "checksums_${VERSION}.txt"
cd - > /dev/null

echo ""
success "Ready for release!"
echo ""
echo "Files to upload to GitHub Release:"
echo "  - ${DEST}/lianwall_${VERSION}_linux_x86_64"
echo "  - ${DEST}/lianwalld_${VERSION}_linux_x86_64"
echo "  - ${DEST}/checksums_${VERSION}.txt"
