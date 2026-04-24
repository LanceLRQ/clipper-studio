# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

ClipperStudio 是一个**桌面级**视频工作台，面向直播录播切片创作者。基于 **Tauri 2.x (Rust) + Vite 6 + React 19 + TypeScript** 技术栈。

**当前状态**：Phase 1~4 核心功能已完成（81.4%），Phase 5（大模型 & 团队协作）待开发。详见 `TASKS.md`。

**目录约定**：应用源码在 `clipper-studio/` 子目录，规划文档在 `plans/` 目录。

## 文档查阅规则

**在使用任何第三方库/框架时，必须先通过 context7 MCP 查阅最新文档**，不要依赖训练数据中的旧知识。步骤：
1. `mcp__context7__resolve-library-id` — 搜索库名，获取 library ID
2. `mcp__context7__query-docs` — 用 library ID 查询具体 API 用法

适用场景：Tauri API、vidstack、TanStack Router/Query、shadcn/ui、sea-orm、mpegts.js 等所有外部依赖。

## PR 创建前置检查（强制）

**当用户要求创建 Pull Request 时，必须先在本地完整运行 `.github/workflows/ci.yml` 里定义的所有 lint / typecheck / test 步骤，全部通过后才允许创建 PR。任何一项失败都必须先修复，修复完成后重跑到绿为止。**

本仓库 CI（`clipper-studio/` 子目录）的等价本地命令：

| CI 步骤 | 本地命令 | 目录 |
|---------|---------|------|
| TypeScript check | `pnpm exec tsc --noEmit` | `clipper-studio/` |
| ESLint | `pnpm lint` | `clipper-studio/` |
| Rust fmt check | `cargo fmt --all -- --check` | `clipper-studio/src-tauri/` |
| Rust clippy（CI 不阻塞，本地建议跑） | `cargo clippy --features builtin-plugins --all-targets` | `clipper-studio/src-tauri/` |
| Rust test | `cargo test --features builtin-plugins --all-targets` | `clipper-studio/src-tauri/` |

**流程要求**：
1. 用户说"创建 PR / 发起 PR / open a PR"等 → 先串行跑上述所有命令
2. 任何一个失败 → 报告具体失败点 + 修复 → 重跑所有命令
3. 全部通过后再执行 `gh pr create`
4. 在 PR 描述中简要标注"本地 CI 已通过"

## Tech Stack

| 层 | 技术 |
|---|------|
| 桌面框架 | Tauri 2.x (Rust) |
| 前端 | Vite 6 + React 19 + TypeScript |
| UI 库 | shadcn/ui + Tailwind CSS 4 |
| 路由 | TanStack Router |
| 数据请求 | TanStack Query |
| 状态管理 | Zustand（仅 UI 瞬态） |
| 播放器 | vidstack |
| 数据库 | SQLite (rusqlite / sea-orm, WAL 模式) |

## Architecture

3 层 12 个子系统架构：
- **表现层**（Frontend）：①视频工作台UI ②任务中心UI ③设置/插件UI
- **核心层**（Rust）：④媒体处理 ⑤资源管理 ⑥任务调度 ⑦字幕 ⑧弹幕 ⑨插件 ⑩数据 ⑪桌面
- **外部服务层**：⑫外部服务对接（通过⑨间接管理）

**核心约束**：
- 前后端唯一通信方式：**Tauri IPC**（不使用 HTTP/WebSocket）
- ⑩数据持久层是唯一真相源（子系统通过 SQLite 交换数据）
- ⑥任务调度器是唯一异步入口（IPC handler 禁止阻塞）
- ①前端只做展示（业务逻辑和 I/O 在 Rust 侧）

## Project Structure

```
clipper-studio/                 # 项目根目录
├── clipper-studio/             # 应用源码
│   ├── src-tauri/src/          # Rust 后端
│   │   ├── commands/           # Tauri IPC Commands
│   │   ├── core/               # 核心业务
│   │   ├── db/                 # 数据库层（migration + models）
│   │   ├── shell/              # 桌面集成（tray）
│   │   └── utils/              # 工具（ffmpeg.rs / time.rs）
│   ├── src/                    # React 前端
│   │   ├── components/         # React 组件（ui/）
│   │   ├── lib/                # 工具函数
│   │   └── ...
│   ├── package.json
│   └── vite.config.ts
├── plans/                      # 规划文档
├── CLAUDE.md                   # Claude Code 主索引
├── TASKS.md                    # 开发任务清单
└── LICENSE
```

## Development Commands

```bash
cd clipper-studio             # 先进入应用源码目录
pnpm dev                      # 前端开发（热更新）
pnpm tauri dev                # Tauri 开发（前端 + Rust 同时启动）
pnpm tauri build              # 构建发布包
cd src-tauri && cargo build   # 仅编译 Rust
cd src-tauri && cargo check   # 检查 Rust 编译
cd src-tauri && cargo test    # Rust 测试
```

## Key Design Decisions

### 前端状态管理边界

| 状态类型 | 管理者 | 示例 |
|---------|--------|------|
| URL 状态 | TanStack Router | 页面、筛选、分页 |
| 服务端数据 | TanStack Query | 视频列表、字幕、任务 |
| UI 瞬态 | Zustand | 播放器时间、侧边栏折叠 |

### 字幕绝对时间

`subtitle_segments` 的 `start_ms`/`end_ms` 使用 **Unix 毫秒时间戳**（绝对时间），不是文件内相对偏移。ASR 结果入库时转换为绝对时间，播放器展示时再转回文件内相对时间。

### 插件通信

服务级插件（ASR/LLM/录制）用 HTTP，工具级插件（弹幕转换/导出）用 stdio。业务代码通过 `PluginTransport` trait 统一调用，不区分底层通信方式。

### 已确定的技术决策

| 决策 | 方案 |
|------|------|
| FFmpeg 跨平台分发 | 依赖管理器按需下载 + config.toml 手动配置 + 系统 PATH fallback |
| SQLite ORM | 混合方案 — sea-orm 建模/migration，复杂查询用 raw SQL |
| vidstack | vidstack 1.x React 不成熟，暂用原生 `<video>` |
| 音量包络线 | PCM 方案（FFmpeg f32le → Rust RMS） |
| 插件通信协议 | HTTP / Stdio / Builtin 三层架构 |

## Coding Conventions

### Rust
- 错误处理：核心模块用 `thiserror`，应用层用 `anyhow`，Command 返回 `Result<T, String>`
- 禁止生产代码中 `.unwrap()`
- 日志使用 `tracing` crate
- FFmpeg 操作通过 `FFmpegCommand` 构建器，不直接拼接命令
- 批量数据库写入必须使用事务

### TypeScript/React
- `invoke` 调用封装到 `src/services/` 层，组件通过 hooks 使用
- 服务端数据只通过 TanStack Query 缓存，不存 Zustand
- 使用 shadcn/ui + Tailwind CSS，不写自定义 CSS 类名

### 通用
- 注释语言与现有代码保持一致
- 禁止版本标识注释、AI 模型标识注释
- 禁止 optimize/fix/improved 等冗余词汇命名
