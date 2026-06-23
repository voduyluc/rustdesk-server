#!/bin/bash
# =============================================================================
# sync-upstream.sh
# Đồng bộ code mới nhất từ rustdesk/rustdesk-server (upstream) và
# tự động re-apply các customization của voduyluc lên trên.
#
# Cơ chế: git rebase (primary) + patch files (fallback)
#
# Cách dùng:
#   bash vdl_deploy/sync-upstream.sh             # rebase bình thường
#   bash vdl_deploy/sync-upstream.sh --patch     # dùng patch thay vì rebase
#   bash vdl_deploy/sync-upstream.sh --check     # chỉ xem upstream có gì mới
# =============================================================================

set -e

UPSTREAM_REMOTE="upstream"
UPSTREAM_REPO="https://github.com/rustdesk/rustdesk-server"
UPSTREAM_BRANCH="master"
OUR_BRANCH="claude/client-id-source-management-5vi87b"
PATCHES_DIR="$(dirname "$0")/patches"

# Màu sắc
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

MODE="rebase"
if [[ "$1" == "--patch" ]]; then MODE="patch"; fi
if [[ "$1" == "--check" ]]; then MODE="check"; fi

echo -e "${BLUE}=== VDL RustDesk Upstream Sync ===${NC}"
echo ""

# ── Kiểm tra upstream remote ──────────────────────────────────────────────────
if ! git remote get-url "${UPSTREAM_REMOTE}" &>/dev/null; then
    echo -e "${YELLOW}Chưa có remote '${UPSTREAM_REMOTE}', đang thêm...${NC}"
    git remote add "${UPSTREAM_REMOTE}" "${UPSTREAM_REPO}"
    echo -e "${GREEN}Đã thêm: ${UPSTREAM_REMOTE} → ${UPSTREAM_REPO}${NC}"
fi

# ── Fetch upstream ─────────────────────────────────────────────────────────────
echo "Fetching ${UPSTREAM_REMOTE}/${UPSTREAM_BRANCH}..."
git fetch "${UPSTREAM_REMOTE}" "${UPSTREAM_BRANCH}"

# ── Check mode: chỉ hiển thị những commit mới ─────────────────────────────────
if [[ "$MODE" == "check" ]]; then
    echo ""
    echo -e "${BLUE}Commits upstream chưa có trong branch của bạn:${NC}"
    git log --oneline HEAD.."${UPSTREAM_REMOTE}/${UPSTREAM_BRANCH}"
    echo ""
    BEHIND=$(git rev-list --count HEAD.."${UPSTREAM_REMOTE}/${UPSTREAM_BRANCH}")
    if [[ "$BEHIND" -eq 0 ]]; then
        echo -e "${GREEN}Branch của bạn đã cập nhật với upstream.${NC}"
    else
        echo -e "${YELLOW}Branch của bạn đang thiếu ${BEHIND} commit(s) từ upstream.${NC}"
        echo "Chạy: bash vdl_deploy/sync-upstream.sh  để đồng bộ"
    fi
    exit 0
fi

# Kiểm tra có gì mới không
BEHIND=$(git rev-list --count HEAD.."${UPSTREAM_REMOTE}/${UPSTREAM_BRANCH}")
if [[ "$BEHIND" -eq 0 ]]; then
    echo -e "${GREEN}Branch đã cập nhật với upstream. Không cần sync.${NC}"
    exit 0
fi

echo -e "${YELLOW}Upstream có ${BEHIND} commit mới. Bắt đầu sync...${NC}"
echo ""

# ── Đảm bảo working tree sạch ─────────────────────────────────────────────────
if ! git diff --quiet || ! git diff --cached --quiet; then
    echo -e "${RED}ERROR: Working tree có thay đổi chưa commit.${NC}"
    echo "Vui lòng commit hoặc stash trước khi sync:"
    echo "  git stash"
    echo "  bash vdl_deploy/sync-upstream.sh"
    echo "  git stash pop"
    exit 1
fi

# ── Đảm bảo đúng branch ───────────────────────────────────────────────────────
CURRENT_BRANCH=$(git branch --show-current)
if [[ "$CURRENT_BRANCH" != "$OUR_BRANCH" ]]; then
    echo -e "${YELLOW}Chuyển sang branch ${OUR_BRANCH}...${NC}"
    git checkout "${OUR_BRANCH}"
fi

# =============================================================================
# MODE 1: REBASE (khuyến nghị)
# Re-apply các commits của mình lên đầu upstream mới nhất
# =============================================================================
if [[ "$MODE" == "rebase" ]]; then
    echo -e "${BLUE}[Mode: Rebase]${NC}"
    echo "git rebase ${UPSTREAM_REMOTE}/${UPSTREAM_BRANCH}"
    echo ""

    if git rebase "${UPSTREAM_REMOTE}/${UPSTREAM_BRANCH}"; then
        echo ""
        echo -e "${GREEN}✓ Rebase thành công!${NC}"
        echo ""
        echo "Các commits của bạn đã được re-apply:"
        # Đếm commit từ upstream head
        UPSTREAM_HEAD=$(git rev-parse "${UPSTREAM_REMOTE}/${UPSTREAM_BRANCH}")
        git log --oneline "${UPSTREAM_HEAD}..HEAD"
        echo ""
        echo -e "${YELLOW}Nhớ push với --force-with-lease vì rebase viết lại lịch sử:${NC}"
        echo "  git push --force-with-lease origin ${OUR_BRANCH}"
    else
        echo ""
        echo -e "${RED}✗ Rebase gặp conflict! Giải quyết thủ công:${NC}"
        echo ""
        echo "  1. Xem file bị conflict:"
        echo "     git status"
        echo ""
        echo "  2. Sửa từng file conflict (tìm <<<<<<< / ======= / >>>>>>>)"
        echo ""
        echo "  3. Tiếp tục rebase:"
        echo "     git add <file-đã-sửa>"
        echo "     git rebase --continue"
        echo ""
        echo "  4. Hoặc hủy và dùng fallback patch mode:"
        echo "     git rebase --abort"
        echo "     bash vdl_deploy/sync-upstream.sh --patch"
        exit 1
    fi

# =============================================================================
# MODE 2: PATCH (fallback khi rebase không khả thi)
# Merge upstream rồi apply lại patches từ vdl_deploy/patches/
# =============================================================================
elif [[ "$MODE" == "patch" ]]; then
    echo -e "${BLUE}[Mode: Patch fallback]${NC}"
    echo ""

    # Kiểm tra patch files tồn tại
    PATCH_FILES=("${PATCHES_DIR}"/*.patch)
    if [[ ! -e "${PATCH_FILES[0]}" ]]; then
        echo -e "${RED}ERROR: Không tìm thấy patch files trong ${PATCHES_DIR}/${NC}"
        echo "Chạy lệnh sau để tạo lại patches:"
        echo "  bash vdl_deploy/regen-patches.sh"
        exit 1
    fi

    # Reset về upstream HEAD
    echo "Reset về ${UPSTREAM_REMOTE}/${UPSTREAM_BRANCH}..."
    git reset --hard "${UPSTREAM_REMOTE}/${UPSTREAM_BRANCH}"

    # Apply từng patch
    echo ""
    echo "Applying patches..."
    for patch in "${PATCHES_DIR}"/*.patch; do
        echo -n "  Applying $(basename "${patch}")... "
        if git apply --check "${patch}" 2>/dev/null; then
            git apply "${patch}"
            echo -e "${GREEN}OK${NC}"
        else
            echo -e "${RED}CONFLICT${NC}"
            echo ""
            echo -e "${RED}Patch ${patch} không apply được tự động.${NC}"
            echo "Thử apply với fuzz:"
            git apply --reject --whitespace=fix "${patch}" || true
            echo ""
            echo "Các file .rej chứa phần không apply được:"
            find . -name "*.rej" 2>/dev/null
            echo ""
            echo "Sửa thủ công rồi chạy:"
            echo "  git add ."
            echo "  git commit -m 'chore: re-apply vdl patches after upstream sync'"
            exit 1
        fi
    done

    # Commit patches đã apply
    git add src/ vdl_deploy/
    git commit -m "chore: re-apply vdl customizations after upstream sync"
    echo ""
    echo -e "${GREEN}✓ Patches applied thành công!${NC}"
    echo -e "${YELLOW}Push với --force-with-lease:${NC}"
    echo "  git push --force-with-lease origin ${OUR_BRANCH}"
fi

echo ""
echo "Hoàn tất. Kiểm tra lại:"
echo "  git log --oneline -8"
