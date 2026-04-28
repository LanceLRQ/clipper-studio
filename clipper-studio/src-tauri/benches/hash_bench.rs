//! Hash & file-scan benchmarks (Phase 1 P1-S5-04).
//!
//! 目标基准（来自 plans/architecture.md 十二章）：
//! - Blake3 哈希：3h 视频 < 15s
//! - 文件扫描：100 视频 < 5s
//!
//! 运行方式：
//!   cd clipper-studio/src-tauri && cargo bench --bench hash_bench

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use clipper_studio_lib::utils::hash::blake3_file;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use tempfile::TempDir;
use tokio::runtime::Runtime;

/// 生成指定大小的临时文件，内容用伪随机字节填充。
/// 使用线性同余生成器，避免可压缩内容导致的 OS 缓存优化偏差。
fn make_file(dir: &TempDir, name: &str, size: usize) -> PathBuf {
    let path = dir.path().join(name);
    let mut f = File::create(&path).expect("create temp file");

    let mut buf = vec![0u8; 64 * 1024];
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut written = 0usize;
    while written < size {
        for chunk in buf.chunks_mut(8) {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let bytes = state.to_le_bytes();
            for (i, b) in chunk.iter_mut().enumerate() {
                *b = bytes[i];
            }
        }
        let n = (size - written).min(buf.len());
        f.write_all(&buf[..n]).expect("write temp file");
        written += n;
    }
    f.sync_all().ok();
    path
}

fn bench_hash(c: &mut Criterion) {
    let rt = Runtime::new().expect("tokio runtime");
    let dir = TempDir::new().expect("tempdir");

    let mut group = c.benchmark_group("blake3_file");
    group.sample_size(10);

    // 全量哈希分支：1MB（< 2MB 阈值）
    let small = make_file(&dir, "small_1mb.bin", 1024 * 1024);
    group.throughput(Throughput::Bytes(1024 * 1024));
    group.bench_with_input(BenchmarkId::new("full", "1MB"), &small, |b, p| {
        b.to_async(&rt)
            .iter(|| async { blake3_file(p).await.unwrap() });
    });

    // 抽样哈希分支：64MB / 512MB（模拟典型/长视频）
    for &size_mb in &[64usize, 512] {
        let path = make_file(
            &dir,
            &format!("sampled_{}mb.bin", size_mb),
            size_mb * 1024 * 1024,
        );
        // 抽样实际只读 ~2MB，不按总大小算 throughput，避免误导
        group.throughput(Throughput::Bytes(2 * 1024 * 1024));
        group.bench_with_input(
            BenchmarkId::new("sampled", format!("{}MB", size_mb)),
            &path,
            |b, p| {
                b.to_async(&rt)
                    .iter(|| async { blake3_file(p).await.unwrap() });
            },
        );
    }

    group.finish();
}

fn bench_scan(c: &mut Criterion) {
    let rt = Runtime::new().expect("tokio runtime");

    // 准备 100 个空文件（目标：100 视频 < 5s）
    let dir = TempDir::new().expect("tempdir");
    for i in 0..100 {
        let p = dir.path().join(format!("video_{:04}.mp4", i));
        File::create(&p).expect("create").write_all(b"stub").ok();
    }
    let root = dir.path().to_path_buf();

    let mut group = c.benchmark_group("file_scan");
    group.sample_size(20);
    group.throughput(Throughput::Elements(100));

    group.bench_function("walkdir_metadata_100", |b| {
        b.to_async(&rt).iter(|| {
            let root = root.clone();
            async move {
                let mut count = 0usize;
                let mut entries = tokio::fs::read_dir(&root).await.unwrap();
                while let Some(ent) = entries.next_entry().await.unwrap() {
                    let _meta = ent.metadata().await.unwrap();
                    count += 1;
                }
                count
            }
        });
    });

    group.finish();
}

criterion_group!(benches, bench_hash, bench_scan);
criterion_main!(benches);
