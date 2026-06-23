# VDL Deploy — RustDesk Server (voduyluc fork)

Thư mục này chứa script cài đặt và công cụ quản lý customization của fork `voduyluc/rustdesk-server`.

## Nội dung

| File | Mô tả |
|---|---|
| `install.sh` | Cài đặt RustDesk server bằng cách clone repo và build từ source |
| `sync-upstream.sh` | Đồng bộ code mới từ repo gốc `rustdesk/rustdesk-server` |
| `regen-patches.sh` | Tái tạo patch backup sau khi thêm feature mới |
| `patches/` | Bản backup các customization dưới dạng `.patch` file |

---

## Cài đặt server (`install.sh`)

Script này thay thế script gốc của [techahold/rustdeskinstall](https://github.com/techahold/rustdeskinstall). Thay vì download file zip pre-built, script sẽ **clone repo và build trực tiếp trên server**.

### Điểm khác so với script gốc

- Cài Rust toolchain (`rustup`) nếu chưa có
- `git clone voduyluc/rustdesk-server` rồi `cargo build --release`
- Systemd unit cho `hbbs` bao gồm env vars cho **Management API**:
  - `API_PORT` — port Management API (mặc định: `21119`)
  - `API_KEY` — key bảo mật API (để trống = không cần auth)
  - `ENABLE_WHITELIST` — bật whitelist (`Y`/`N`, mặc định `N`)
- Option `--branch` để chỉ định git branch cần build

### Cách chạy

```bash
# Chạy trực tiếp từ GitHub
curl -fsSL https://raw.githubusercontent.com/voduyluc/rustdesk-server/claude/client-id-source-management-5vi87b/vdl_deploy/install.sh | bash

# Hoặc với options
bash install.sh --resolveip --skip-http
bash install.sh --resolvedns "your.domain.com" --branch "main"
```

### Cấu hình sau cài đặt

Để đặt `API_KEY` hoặc bật whitelist, sửa file systemd:

```bash
sudo systemctl edit rustdesksignal.service
```

Thêm vào:
```ini
[Service]
Environment=API_KEY=your-secret-key
Environment=ENABLE_WHITELIST=Y
```

```bash
sudo systemctl daemon-reload && sudo systemctl restart rustdesksignal.service
```

---

## Management API

Khi server chạy, Management API tự động bật trên port `API_PORT` (mặc định `21119`).

### Endpoints

| Method | Endpoint | Mô tả |
|---|---|---|
| `GET` | `/api/peers` | Danh sách tất cả peers (online/offline, disabled, last_online) |
| `POST` | `/api/peers/:id/disable` | Vô hiệu hóa peer |
| `POST` | `/api/peers/:id/enable` | Kích hoạt lại peer |
| `GET` | `/api/whitelist` | Danh sách whitelist |
| `POST` | `/api/whitelist` | Thêm ID vào whitelist |
| `DELETE` | `/api/whitelist/:id` | Xóa ID khỏi whitelist |

### Xác thực

Truyền header `X-Api-Key` nếu đã đặt `API_KEY`:

```bash
# Xem danh sách peers
curl http://your-server:21119/api/peers -H "X-Api-Key: your-secret-key"

# Vô hiệu hóa một peer
curl -X POST http://your-server:21119/api/peers/abc123/disable \
     -H "X-Api-Key: your-secret-key"

# Thêm vào whitelist
curl -X POST http://your-server:21119/api/whitelist \
     -H "X-Api-Key: your-secret-key" \
     -H "Content-Type: application/json" \
     -d '{"id":"abc123","note":"máy văn phòng"}'
```

---

## Đồng bộ upstream (`sync-upstream.sh`)

### Tại sao cần cơ chế này?

Các customization của fork này nằm trên đầu các commits của repo gốc:

```
upstream (rustdesk/rustdesk-server):
  ...─── 815c728 ──────────────────────────────────
                 ↑                                  ↑ (upstream có update mới)
fork này:        └── a9c8c0f (feature) ── 906432d (install.sh)
```

Khi upstream ra bản mới, cần sync để nhận bugfix/security update trong khi vẫn giữ customization.

**Cơ chế primary là `git rebase`**: re-apply các commits của mình lên đầu upstream mới nhất.

```
Sau rebase:
  ...─── 815c728 ─── [upstream mới] ─── a9c8c0f ─── 906432d
```

Vì các commits của fork chỉ **thêm code** (không sửa code gốc), rebase thường không conflict.

### Workflow

**1. Kiểm tra upstream có gì mới:**

```bash
bash vdl_deploy/sync-upstream.sh --check
```

Output mẫu:
```
Commits upstream chưa có trong branch của bạn:
abc1234 fix(security): update dependency X
def5678 feat: add new protocol support

Branch của bạn đang thiếu 2 commit(s) từ upstream.
```

**2. Sync bằng rebase (khuyến nghị):**

```bash
bash vdl_deploy/sync-upstream.sh
git push --force-with-lease origin claude/client-id-source-management-5vi87b
```

> `--force-with-lease` thay vì `--force` để tránh ghi đè commit của người khác.

**3. Nếu rebase gặp conflict:**

```bash
# Xem file bị conflict
git status

# Sửa thủ công từng file (tìm <<<<<<< / ======= / >>>>>>>)
# Sau đó:
git add <file-đã-sửa>
git rebase --continue

# Hoặc hủy và dùng fallback:
git rebase --abort
bash vdl_deploy/sync-upstream.sh --patch
```

**4. Fallback: apply patch thủ công**

Dùng khi rebase không khả thi (ví dụ upstream refactor lớn):

```bash
bash vdl_deploy/sync-upstream.sh --patch
git push --force-with-lease origin claude/client-id-source-management-5vi87b
```

---

## Cập nhật patches sau khi thêm feature (`regen-patches.sh`)

Mỗi khi thêm feature mới và commit, cần tái tạo patch backup:

```bash
bash vdl_deploy/regen-patches.sh
git add vdl_deploy/patches/
git commit -m "chore: regenerate patches"
git push
```

Script tự động:
- Tìm điểm phân kỳ với upstream (`git merge-base`)
- Tạo lại tất cả `.patch` files từ commits của fork
- Bỏ qua các file runtime như `db_v2.sqlite3`

---

## Tóm tắt quy trình thường ngày

```
┌─────────────────────────────────────────────────────┐
│  Upstream ra version mới?                           │
│                                                     │
│  1. bash vdl_deploy/sync-upstream.sh --check        │
│  2. bash vdl_deploy/sync-upstream.sh                │
│  3. git push --force-with-lease origin <branch>     │
│  4. Rebuild trên server: git pull && cargo build    │
└─────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────┐
│  Thêm feature mới?                                  │
│                                                     │
│  1. Viết code, commit như bình thường               │
│  2. bash vdl_deploy/regen-patches.sh                │
│  3. git add vdl_deploy/patches/ && git commit       │
│  4. git push                                        │
└─────────────────────────────────────────────────────┘
```
