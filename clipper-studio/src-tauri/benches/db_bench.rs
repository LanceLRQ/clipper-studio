//! Database benchmarks (Phase 1 P1-S5-04).
//!
//! 目标基准（来自 plans/architecture.md 十二章）：
//! - 视频列表查询 < 50ms
//!
//! 全部使用 in-memory SQLite（`:memory:`），不污染磁盘。
//!
//! 运行方式：
//!   cd clipper-studio/src-tauri && cargo bench --bench db_bench

use std::path::Path;

use clipper_studio_lib::db::Database;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, Statement};
use tokio::runtime::Runtime;

async fn fresh_db() -> Database {
    let db = Database::connect(Path::new(":memory:")).await.unwrap();
    db.run_migrations().await.unwrap();
    // 一个 workspace 用于关联视频
    db.conn()
        .execute_unprepared(
            "INSERT INTO workspaces (name, path, adapter_id) VALUES ('bench', '/bench', 'generic')",
        )
        .await
        .unwrap();
    db
}

/// 批量插入 N 条 video 记录，包在单个事务中（与生产入库路径一致）。
async fn seed_videos(conn: &DatabaseConnection, n: usize) {
    conn.execute_unprepared("BEGIN").await.unwrap();
    for i in 0..n {
        let sql = format!(
            "INSERT INTO videos (file_path, file_name, file_size, duration_ms, workspace_id, has_subtitle, has_danmaku, recorded_at) \
             VALUES ('/bench/v_{i:06}.mp4', 'v_{i:06}.mp4', {sz}, {dur}, 1, {sub}, {dan}, '2026-04-{day:02} 12:00:00')",
            i = i,
            sz = 1_000_000_000u64 + i as u64,
            dur = 3_600_000 + (i as u64 % 7200),
            sub = i % 2,
            dan = (i + 1) % 2,
            day = 1 + (i % 28),
        );
        conn.execute_unprepared(&sql).await.unwrap();
    }
    conn.execute_unprepared("COMMIT").await.unwrap();
}

fn bench_insert(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("db_insert");
    group.sample_size(10);
    group.throughput(Throughput::Elements(1000));

    group.bench_function("videos_1000_in_txn", |b| {
        b.to_async(&rt).iter_with_setup(
            || rt.block_on(async { fresh_db().await }),
            |db| async move {
                seed_videos(db.conn(), 1000).await;
            },
        );
    });

    group.finish();
}

fn bench_query_list(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("db_query_videos_list");
    group.sample_size(20);

    for &dataset in &[1_000usize, 10_000] {
        let db = rt.block_on(async {
            let db = fresh_db().await;
            seed_videos(db.conn(), dataset).await;
            db
        });

        group.bench_with_input(
            BenchmarkId::new("list_top50_by_recorded", dataset),
            &dataset,
            |b, _| {
                b.to_async(&rt).iter(|| async {
                    let rows = db
                        .conn()
                        .query_all(Statement::from_string(
                            DatabaseBackend::Sqlite,
                            "SELECT id, file_name, file_size, duration_ms, has_subtitle, has_danmaku \
                             FROM videos WHERE workspace_id = 1 \
                             ORDER BY recorded_at DESC LIMIT 50"
                                .to_string(),
                        ))
                        .await
                        .unwrap();
                    rows.len()
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("list_with_flags_filter", dataset),
            &dataset,
            |b, _| {
                b.to_async(&rt).iter(|| async {
                    let rows = db
                        .conn()
                        .query_all(Statement::from_string(
                            DatabaseBackend::Sqlite,
                            "SELECT id, file_name FROM videos \
                             WHERE workspace_id = 1 AND has_subtitle = 1 AND has_danmaku = 0 \
                             ORDER BY recorded_at DESC LIMIT 50"
                                .to_string(),
                        ))
                        .await
                        .unwrap();
                    rows.len()
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("count_total", dataset),
            &dataset,
            |b, _| {
                b.to_async(&rt).iter(|| async {
                    let row = db
                        .conn()
                        .query_one(Statement::from_string(
                            DatabaseBackend::Sqlite,
                            "SELECT COUNT(*) AS c FROM videos WHERE workspace_id = 1".to_string(),
                        ))
                        .await
                        .unwrap()
                        .unwrap();
                    let _c: i64 = row.try_get("", "c").unwrap();
                });
            },
        );
    }

    group.finish();
}

fn bench_settings_kv(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let db = rt.block_on(async { fresh_db().await });

    let mut group = c.benchmark_group("db_settings_kv");
    group.sample_size(50);

    group.bench_function("upsert_single", |b| {
        b.to_async(&rt).iter(|| async {
            db.conn()
                .execute_unprepared(
                    "INSERT OR REPLACE INTO settings_kv (key, value) VALUES ('bench_key', 'bench_value')",
                )
                .await
                .unwrap();
        });
    });

    group.bench_function("read_single", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = db
                .conn()
                .query_one(Statement::from_string(
                    DatabaseBackend::Sqlite,
                    "SELECT value FROM settings_kv WHERE key = 'bench_key'".to_string(),
                ))
                .await
                .unwrap();
        });
    });

    group.finish();
}

criterion_group!(benches, bench_insert, bench_query_list, bench_settings_kv);
criterion_main!(benches);
