# ClipperStudio 开发指南

> 面向核心代码贡献者，描述本地开发、构建、测试与基准测试的完整流程。
> 插件开发请参考 [contribute.md](./contribute.md)。
> 最后更新：2026-04-25

---

## 1. 环境准备

### 1.1 必要工具

| 工具 | 版本 | 说明 |
|------|------|------|
| Node.js | >= 20 | 前端构建运行时 |
| pnpm | 10.x | 包管理器（仓库已锁定 `pnpm@10.28.2`） |
| Rust | stable（>= 1.77） | 由 `rust-toolchain.toml` 锁定 |
| Tauri CLI | 2.x | 通过 `pnpm tauri ...` 调用，无需全局安装 |
| FFmpeg / FFprobe | 任意 | 启动后通过应用内"依赖管理器"安装，或放入 `src-tauri/bin/`，或加入系统 PATH |

### 1.2 各平台系统依赖

**macOS**
```bash
xcode-select --install
```

**Windows**
- 安装 [Visual Studio Build Tools](https://visualstudio.microsoft.com/downloads/) 并勾选 "Desktop development with C++"
- 安装 [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/)（Win11 自带）

**Linux（Ubuntu / Debian）**
```bash
sudo apt-get update
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev \
  libappindicator3-dev \
  librsvg2-dev \
  patchelf \
  libssl-dev
```

### 1.3 克隆与初始化

```bash
git clone https://github.com/LanceLRQ/clipper-studio.git
cd clipper-studio/clipper-studio       # 进入应用源码目录（嵌套同名）
pnpm install --frozen-lockfile
```

> ⚠️ **重要**：仓库根目录是 `clipper-studio/`，应用源码在子目录 `clipper-studio/clipper-studio/`。
> 文档和规划文件在仓库根，开发命令几乎都需要先 `cd clipper-studio`。

---

## 2. 目录结构速查

```
clipper-studio/                     # 仓库根
├── clipper-studio/                 # 应用源码根（开发命令工作目录）
│   ├── src/                        # React 19 前端
│   ├── src-tauri/
│   │   ├── src/                    # Rust 后端
│   │   ├── benches/                # criterion 基准测试
│   │   ├── tests/                  # Rust 集成测试
│   │   ├── bin/                    # FFmpeg 可选放置位置
│   │   └── Cargo.toml
│   ├── crates/                     # workspace 内置插件 crate
│   ├── plugins/                    # 第三方/外置插件（excluded from workspace）
│   └── package.json
├── docs/                           # VitePress 用户文档
├── plans/                          # 架构 / 子系统 / 规划文档
├── contribute.md                   # 插件开发指南
├── development.md                  # 本文件（核心开发指南）
└── TASKS.md                        # 开发任务清单
```

---

## 3. 日常开发命令

所有命令默认在 `clipper-studio/clipper-studio/` 目录执行（即应用源码根）。

### 3.1 启动开发模式

```bash
# 仅前端（无 Rust，纯页面调试）
pnpm dev

# Tauri 全栈开发（前端热更新 + Rust 后端）
pnpm tauri dev

# 启用内置插件 (recorder / storage)
pnpm tauri:dev:builtin
```

### 3.2 构建发布包

```bash
# 标准构建
pnpm tauri build

# 启用内置插件
pnpm tauri:build:builtin
```

产物位置：`src-tauri/target/release/bundle/`（按平台分目录）。

### 3.3 仅编译 / 检查 Rust

```bash
cd src-tauri
cargo check                                # 快速类型检查
cargo build --features builtin-plugins     # 完整编译
```

---

## 4. 测试

### 4.1 前端

```bash
pnpm exec tsc --noEmit                # TypeScript 类型检查
pnpm lint                             # ESLint
```

> 当前前端无单元测试框架（未来按需引入 Vitest）。

### 4.2 Rust 单元 / 集成测试

```bash
cd src-tauri
cargo test --features builtin-plugins --all-targets
```

集成测试位于 `src-tauri/tests/`：

| 文件 | 覆盖内容 |
|------|---------|
| `db_migration_test.rs` | 数据库迁移、索引、默认数据 |
| `media_server_test.rs` | 本地媒体服务器（HTTP Range） |
| `video_commands_test.rs` | 视频相关 IPC Command |
| `workspace_commands_test.rs` | 工作区相关 IPC Command |

跑单个测试：
```bash
cargo test --features builtin-plugins --test db_migration_test
cargo test --features builtin-plugins test_migration_creates_all_tables
```

### 4.3 Rust 格式与 lint

```bash
cd src-tauri
cargo fmt --all                              # 自动格式化
cargo fmt --all -- --check                   # 检查（CI 用）
cargo clippy --features builtin-plugins --all-targets
```

---

## 5. 基准测试（Criterion）

性能基准对应 `plans/architecture.md` 第十二章性能目标，通过 [Criterion](https://github.com/bheisler/criterion.rs) 跑。基准源码位于 `src-tauri/benches/`。

### 5.1 运行基准

```bash
cd src-tauri

# 哈希 + 文件扫描
cargo bench --bench hash_bench

# 数据库（in-memory SQLite）
cargo bench --bench db_bench

# 跑所有基准
cargo bench
```

> 第一次运行会编译 release 模式，预计 3~5 分钟。后续增量极快。

### 5.2 基准目录结构

```
src-tauri/benches/
├── hash_bench.rs        # Blake3 哈希 + 文件扫描
└── db_bench.rs          # 数据库查询 / 写入 / KV
```

### 5.3 基准项与目标

#### `hash_bench`

| Bench ID | 内容 | 目标（来自 architecture.md） |
|----------|------|---------------------------|
| `blake3_file/full/1MB` | 1MB 文件全量哈希（< 2MB 阈值） | 验证小文件路径 |
| `blake3_file/sampled/64MB` | 64MB 文件 8 段抽样哈希 | 典型录播片段基线 |
| `blake3_file/sampled/512MB` | 512MB 文件 8 段抽样哈希 | 3h 视频 < 15s |
| `file_scan/walkdir_metadata_100` | 100 个文件目录扫描 + metadata | 100 视频 < 5s |

> 抽样哈希实际只读 ~2MB（8 段 × 256KB），与文件总大小无关。

#### `db_bench`

| Bench ID | 内容 | 目标 |
|----------|------|------|
| `db_insert/videos_1000_in_txn` | 单事务批量插入 1000 行 | 入库吞吐基线 |
| `db_query_videos_list/list_top50_by_recorded` (1k / 10k) | `ORDER BY recorded_at DESC LIMIT 50` | 视频列表 < 50ms |
| `db_query_videos_list/list_with_flags_filter` | 字幕 / 弹幕标志过滤 | 复合索引验证 |
| `db_query_videos_list/count_total` | `COUNT(*)` 统计 | 工作区面板 |
| `db_settings_kv/upsert_single` | `INSERT OR REPLACE` 单条 KV | 设置面板高频路径 |
| `db_settings_kv/read_single` | 单条 KV 读取 | — |

### 5.4 查看 HTML 报告

```bash
open src-tauri/target/criterion/report/index.html       # macOS
xdg-open src-tauri/target/criterion/report/index.html   # Linux
start src-tauri/target/criterion/report/index.html      # Windows PowerShell
```

报告包含每个 bench 的耗时分布图、PDF、迭代趋势对比，多次运行可看到回归。

### 5.5 添加新基准

1. 在 `src-tauri/benches/` 新建 `xxx_bench.rs`
2. 在 `src-tauri/Cargo.toml` 注册：
   ```toml
   [[bench]]
   name = "xxx_bench"
   harness = false
   ```
3. 文件骨架：
   ```rust
   use criterion::{criterion_group, criterion_main, Criterion};

   fn bench_xxx(c: &mut Criterion) {
       c.bench_function("xxx", |b| b.iter(|| { /* ... */ }));
   }

   criterion_group!(benches, bench_xxx);
   criterion_main!(benches);
   ```
4. 异步基准请使用 `b.to_async(&runtime).iter(|| async { ... })`

---

## 6. PR 创建前置检查（强制）

**当你准备提 PR 时，必须先在本地完整跑通 `.github/workflows/ci.yml` 中的所有步骤，全部通过后才允许创建 PR。**

### 6.1 等价本地命令

| CI 步骤 | 本地命令 | 工作目录 |
|---------|---------|---------|
| TypeScript 检查 | `pnpm exec tsc --noEmit` | `clipper-studio/` |
| ESLint | `pnpm lint` | `clipper-studio/` |
| Rust fmt 检查 | `cargo fmt --all -- --check` | `clipper-studio/src-tauri/` |
| Rust clippy（CI 不阻塞，本地建议跑） | `cargo clippy --features builtin-plugins --all-targets` | `clipper-studio/src-tauri/` |
| Rust 测试 | `cargo test --features builtin-plugins --all-targets` | `clipper-studio/src-tauri/` |

### 6.2 一键脚本（可选）

把下面这段保存为 `scripts/precheck.sh`，在仓库根执行：

```bash
#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../clipper-studio"

echo "==> tsc"
pnpm exec tsc --noEmit
echo "==> eslint"
pnpm lint
echo "==> cargo fmt"
( cd src-tauri && cargo fmt --all -- --check )
echo "==> cargo clippy"
( cd src-tauri && cargo clippy --features builtin-plugins --all-targets )
echo "==> cargo test"
( cd src-tauri && cargo test --features builtin-plugins --all-targets )
echo "✅ All checks passed"
```

### 6.3 流程

1. 跑上述全部命令
2. 任何一项失败 → 修复 → 重跑全部
3. 全绿后执行 `gh pr create`，PR 描述里标注"本地 CI 已通过"

---

## 7. 调试技巧

### 7.1 Rust 日志

```bash
# 全部 debug
RUST_LOG=debug pnpm tauri dev

# 仅特定模块
RUST_LOG=clipper_studio_lib::core::scanner=trace pnpm tauri dev
```

日志输出基于 `tracing`，配置见 `src-tauri/src/lib.rs`。

### 7.2 前端 DevTools

开发模式下右键 → "检查元素" 即可。生产构建需要 `devtools` feature：
```bash
cd src-tauri
cargo build --features devtools
```

### 7.3 数据库

应用数据库默认放在 `app_data_dir/clipper-studio.db`：

| 平台 | 路径 |
|------|------|
| macOS | `~/Library/Application Support/com.lancelrq.clipper-studio/` |
| Windows | `%APPDATA%\com.lancelrq.clipper-studio\` |
| Linux | `~/.local/share/com.lancelrq.clipper-studio/` |

可用 `sqlite3` 直接打开排查：
```bash
sqlite3 ~/Library/Application\ Support/com.lancelrq.clipper-studio/clipper-studio.db
> .tables
> SELECT * FROM videos LIMIT 10;
```

### 7.4 重置数据库

直接删除 `clipper-studio.db` 即可，应用启动会重新跑迁移并初始化默认数据。

---

## 8. 编码约定（速查）

完整规范见 `plans/docs/coding-rules.md`，这里只列开发时最常踩的坑：

- **Rust**：核心模块用 `thiserror`，应用层用 `anyhow`，Command 返回 `Result<T, String>`
- **Rust**：禁止生产代码 `.unwrap()`；批量 DB 写入必须事务包裹
- **TypeScript**：`invoke` 调用统一封装到 `src/services/`，组件经 hooks 调用
- **TypeScript**：服务端数据走 TanStack Query，Zustand 只存 UI 瞬态
- **通用**：注释语言与现有代码保持一致；禁止 `optimize/fix/improved` 等冗余命名；禁止版本标识 / AI 模型标识注释

---

## 9. 文档导航

| 我想了解… | 看哪份 |
|-----------|-------|
| 整体技术架构 | `plans/architecture.md` |
| 12 个子系统接口 | `plans/subsystems.md` |
| 当前开发进度 / 任务 | `TASKS.md` |
| 编码规范细则 | `plans/docs/coding-rules.md` |
| 性能基准目标 | `plans/architecture.md` 第十二章 |
| 写一个插件 | `contribute.md` |
| 用户使用方式 | `docs/`（VitePress） |
