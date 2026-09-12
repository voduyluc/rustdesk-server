#!/bin/bash
# =============================================================================
# regen-patches.sh
# Tái tạo patch files từ các commits hiện tại của branch.
# Chạy sau khi thêm feature mới để cập nhật backup patches.
#
# Cách dùng:
#   bash vdl_deploy/regen-patches.sh
# =============================================================================

set -e

PATCHES_DIR="$(dirname "$0")/patches"
UPSTREAM_REMOTE="upstream"
UPSTREAM_BRANCH="master"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo "=== Regenerating VDL patch files ==="
echo ""

# Kiểm tra upstream remote
if ! git remote get-url "${UPSTREAM_REMOTE}" &>/dev/null; then
    echo -e "${RED}ERROR: Remote '${UPSTREAM_REMOTE}' chưa tồn tại.${NC}"
    echo "Thêm upstream:"
    echo "  git remote add upstream https://github.com/rustdesk/rustdesk-server"
    exit 1
fi

# Tìm điểm phân kỳ (merge base) với upstream
git fetch "${UPSTREAM_REMOTE}" "${UPSTREAM_BRANCH}" --quiet
UPSTREAM_HEAD=$(git rev-parse "${UPSTREAM_REMOTE}/${UPSTREAM_BRANCH}")
MERGE_BASE=$(git merge-base HEAD "${UPSTREAM_HEAD}")

echo "Upstream HEAD : $(git log --oneline -1 ${UPSTREAM_HEAD})"
echo "Merge base    : $(git log --oneline -1 ${MERGE_BASE})"
echo ""

# Đếm số commit của mình
OUR_COMMITS=$(git rev-list --count "${MERGE_BASE}..HEAD")
if [[ "$OUR_COMMITS" -eq 0 ]]; then
    echo -e "${YELLOW}Không có commit nào của mình ở trên upstream.${NC}"
    exit 0
fi

echo "Commits cần tạo patch (${OUR_COMMITS} commits):"
git log --oneline "${MERGE_BASE}..HEAD"
echo ""

# Xóa patches cũ
rm -f "${PATCHES_DIR}"/*.patch
mkdir -p "${PATCHES_DIR}"

# Tạo patch mới (bỏ qua các file runtime như db_v2.sqlite3)
git format-patch "${MERGE_BASE}..HEAD" \
    --output-directory "${PATCHES_DIR}" \
    -- src/ vdl_deploy/install.sh

echo ""
echo -e "${GREEN}Patches mới:${NC}"
ls -1 "${PATCHES_DIR}"/*.patch 2>/dev/null | while read f; do
    echo "  $(basename $f)"
done

echo ""
echo -e "${GREEN}✓ Tái tạo hoàn tất. Nhớ commit patches mới:${NC}"
echo "  git add vdl_deploy/patches/"
echo "  git commit -m 'chore: regenerate vdl patches'"
