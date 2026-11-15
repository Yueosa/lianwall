#!/bin/bash
# ===========================================================
# 做了这些事
# 1. 从 Cargo.toml 读取版本号
# 2. cargo build --release 编译两个包
# 3. 复制产物到 build/<version>/ 目录
# 4. 生成 sha256 校验和
# ===========================================================
# 产出文件：
# build/<version>/
# ├── lianwall_<version>_linux_x86_64      # CLI
# ├── lianwalld_<version>_linux_x86_64     # Daemon
# └── checksums_<version>.txt              # 校验和
# ===========================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

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
# 从 workspace Cargo.toml 读取版本（所有包共享同一版本）
VERSION=$(awk '/^\[workspace.package\]/{section=1;next} section && /^version *=/{gsub(/"/, "", $3);print $3;exit}' Cargo.toml)

# 验证版本号是否成功读取
if [[ -z "$VERSION" ]]; then
    error "Failed to read version from Cargo.toml"
    exit 1
fi

info "Building LianWall v${VERSION}"
echo ""

# =========================
# 🔨 编译
# =========================
BUILD_FEATURES=()
FLAVOR=""
if [[ "${LIANWALL_NATIVE_IMAGE:-1}" == "0" ]]; then
    BUILD_FEATURES=(--no-default-features)
    FLAVOR="-external"
    warn "External-only build; select awww/mpvpaper in configuration"
fi
DEST="build/${VERSION}${FLAVOR}"
mkdir -p "$DEST"

info "Building release binaries..."

cargo build --locked --release --package lianwall-cli --package lianwall-daemon "${BUILD_FEATURES[@]}"

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
if [[ -n "$FLAVOR" ]]; then
    success "Experimental build complete (do not publish as a stable release)"
else
    success "Ready for release!"
fi
echo ""
if [[ -n "$FLAVOR" ]]; then
    echo "Local experimental artifacts:"
else
    echo "Files to upload to GitHub Release:"
fi
echo "  - ${DEST}/lianwall_${VERSION}_linux_x86_64"
echo "  - ${DEST}/lianwalld_${VERSION}_linux_x86_64"
echo "  - ${DEST}/checksums_${VERSION}.txt"
