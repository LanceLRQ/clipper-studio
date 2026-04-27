use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

/// Sample size per segment (256KB)
const SAMPLE_SIZE: u64 = 256 * 1024;

/// Number of evenly-spaced sample segments
const SAMPLE_COUNT: u64 = 8;

/// Files smaller than this use full-file hash (2MB)
const SAMPLED_HASH_THRESHOLD: u64 = SAMPLE_SIZE * SAMPLE_COUNT;

/// Compute Blake3 hash using multi-segment sampling.
///
/// For large files (especially over network/SMB), reading the entire file
/// is too slow. Instead, we sample 8 evenly-spaced segments of 256KB each:
///
/// ```text
/// |--S0--|......|--S1--|......|--S2--|......|--S3--|......|--S4--|......|--S5--|......|--S6--|......|--S7--|
/// 0                                                                                            file_size
/// ```
///
/// Total read: ~2MB regardless of file size. For an 8GB file:
/// - Full hash: read 8GB (~minutes over SMB)
/// - Sampled hash: read 2MB (~seconds over SMB)
///
/// Hash input: file_size (8 bytes LE) + S0 + S1 + ... + S7.
/// Including file_size prevents collisions between files of different sizes.
///
/// For files smaller than 2MB, falls back to full-file hash.
pub async fn blake3_file(path: &Path) -> Result<String, std::io::Error> {
    let metadata = tokio::fs::metadata(path).await?;
    let file_size = metadata.len();

    if file_size < SAMPLED_HASH_THRESHOLD {
        return blake3_file_full(path).await;
    }

    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = blake3::Hasher::new();
    let sample_size = SAMPLE_SIZE.min(file_size / SAMPLE_COUNT) as usize;
    let mut buf = vec![0u8; sample_size];

    // Include file size to differentiate files with identical sample content
    hasher.update(&file_size.to_le_bytes());

    // Calculate step between sample start positions
    // Distribute samples evenly: offset_i = i * (file_size - sample_size) / (SAMPLE_COUNT - 1)
    let max_offset = file_size - sample_size as u64;

    for i in 0..SAMPLE_COUNT {
        let offset = if SAMPLE_COUNT <= 1 {
            0
        } else {
            i * max_offset / (SAMPLE_COUNT - 1)
        };

        file.seek(std::io::SeekFrom::Start(offset)).await?;
        let n = file.read(&mut buf).await?;
        hasher.update(&buf[..n]);
    }

    Ok(hasher.finalize().to_hex().to_string())
}

/// Full-file Blake3 hash (for small files)
async fn blake3_file_full(path: &Path) -> Result<String, std::io::Error> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; 65536];

    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    Ok(hasher.finalize().to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_blake3_small_file_uses_full_hash() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::io::Write::write_all(&mut tmp.as_file().try_clone().unwrap(), b"hello world").unwrap();

        let hash = blake3_file(tmp.path()).await.unwrap();
        let expected = blake3::hash(b"hello world").to_hex().to_string();
        assert_eq!(hash, expected);
    }

    #[tokio::test]
    async fn test_blake3_large_file_uses_sampled_hash() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        // 4MB file (above 2MB threshold)
        let data = vec![0xABu8; 4 * 1024 * 1024];
        std::io::Write::write_all(&mut tmp.as_file().try_clone().unwrap(), &data).unwrap();

        let hash = blake3_file(tmp.path()).await.unwrap();
        assert!(!hash.is_empty());

        // Sampled hash should differ from full hash (due to file_size prefix)
        let full_hash = blake3::hash(&data).to_hex().to_string();
        assert_ne!(hash, full_hash);
    }

    #[tokio::test]
    async fn test_blake3_sampled_deterministic() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let data = vec![0x42u8; 4 * 1024 * 1024];
        std::io::Write::write_all(&mut tmp.as_file().try_clone().unwrap(), &data).unwrap();

        let hash1 = blake3_file(tmp.path()).await.unwrap();
        let hash2 = blake3_file(tmp.path()).await.unwrap();
        assert_eq!(hash1, hash2, "Sampled hash should be deterministic");
    }

    #[tokio::test]
    async fn test_blake3_different_sizes_different_hash() {
        let tmp1 = tempfile::NamedTempFile::new().unwrap();
        let tmp2 = tempfile::NamedTempFile::new().unwrap();
        let data1 = vec![0x00u8; 4 * 1024 * 1024];
        let data2 = vec![0x00u8; 5 * 1024 * 1024];
        std::io::Write::write_all(&mut tmp1.as_file().try_clone().unwrap(), &data1).unwrap();
        std::io::Write::write_all(&mut tmp2.as_file().try_clone().unwrap(), &data2).unwrap();

        let hash1 = blake3_file(tmp1.path()).await.unwrap();
        let hash2 = blake3_file(tmp2.path()).await.unwrap();
        assert_ne!(
            hash1, hash2,
            "Different file sizes should produce different hashes"
        );
    }

    #[tokio::test]
    async fn test_blake3_different_content_same_size() {
        // Two files of same size but different content at the head (always sampled)
        let size = 4 * 1024 * 1024;
        let tmp1 = tempfile::NamedTempFile::new().unwrap();
        let tmp2 = tempfile::NamedTempFile::new().unwrap();

        let data1 = vec![0x00u8; size];
        let mut data2 = vec![0x00u8; size];
        // Differ at position 0 — guaranteed to be in the first sample segment
        data2[0] = 0xFF;

        std::io::Write::write_all(&mut tmp1.as_file().try_clone().unwrap(), &data1).unwrap();
        std::io::Write::write_all(&mut tmp2.as_file().try_clone().unwrap(), &data2).unwrap();

        let hash1 = blake3_file(tmp1.path()).await.unwrap();
        let hash2 = blake3_file(tmp2.path()).await.unwrap();
        assert_ne!(
            hash1, hash2,
            "Files differing within sample range should have different hashes"
        );
    }

    #[tokio::test]
    async fn test_blake3_below_threshold_uses_full_hash() {
        // File below threshold should use full hash
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let data = vec![0xCDu8; (SAMPLED_HASH_THRESHOLD - 1) as usize];
        std::io::Write::write_all(&mut tmp.as_file().try_clone().unwrap(), &data).unwrap();

        let hash = blake3_file(tmp.path()).await.unwrap();
        let full_hash = blake3::hash(&data).to_hex().to_string();
        assert_eq!(hash, full_hash, "File below threshold should use full hash");
    }

    #[tokio::test]
    async fn test_blake3_at_threshold_uses_sampled_hash() {
        // File at exact threshold should use sampled hash (not full)
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let data = vec![0xCDu8; SAMPLED_HASH_THRESHOLD as usize];
        std::io::Write::write_all(&mut tmp.as_file().try_clone().unwrap(), &data).unwrap();

        let hash = blake3_file(tmp.path()).await.unwrap();
        let full_hash = blake3::hash(&data).to_hex().to_string();
        // At threshold, sampled hash kicks in — should differ from full hash
        assert_ne!(hash, full_hash, "File at threshold should use sampled hash");
    }

    #[tokio::test]
    async fn test_blake3_nonexistent_file() {
        let result = blake3_file(Path::new("/nonexistent/file.dat")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_blake3_empty_file() {
        // Empty file (0 bytes) — falls into full-hash branch
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let hash = blake3_file(tmp.path()).await.unwrap();
        let expected = blake3::hash(b"").to_hex().to_string();
        assert_eq!(
            hash, expected,
            "empty file should match Blake3 of empty input"
        );
    }

    #[tokio::test]
    async fn test_blake3_below_threshold_one_byte() {
        // SAMPLED_HASH_THRESHOLD - 1 boundary already covered;
        // here we verify a tiny single-byte file
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::io::Write::write_all(&mut tmp.as_file().try_clone().unwrap(), b"A").unwrap();
        let hash = blake3_file(tmp.path()).await.unwrap();
        let expected = blake3::hash(b"A").to_hex().to_string();
        assert_eq!(hash, expected);
    }

    #[tokio::test]
    async fn test_blake3_known_vector() {
        // Pin against a known Blake3 vector to catch any algorithm regression
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::io::Write::write_all(
            &mut tmp.as_file().try_clone().unwrap(),
            b"The quick brown fox jumps over the lazy dog",
        )
        .unwrap();
        let hash = blake3_file(tmp.path()).await.unwrap();
        // Blake3 of the well-known pangram
        assert_eq!(
            hash,
            "2f1514181aadccd913abd94cfa592701a5686ab23f8df1dff1b74710febc6d4a"
        );
    }

    #[tokio::test]
    async fn test_blake3_difference_in_last_segment() {
        // Two same-size files differing only at the END (in the last sample segment).
        // Verifies that sampling actually covers the tail, not just the head.
        let size = 8 * 1024 * 1024; // 8MB → sampled
        let tmp1 = tempfile::NamedTempFile::new().unwrap();
        let tmp2 = tempfile::NamedTempFile::new().unwrap();

        let mut data1 = vec![0xAAu8; size];
        let mut data2 = vec![0xAAu8; size];
        // Differ at last byte — must fall within the last sample segment
        data1[size - 1] = 0x00;
        data2[size - 1] = 0xFF;

        std::io::Write::write_all(&mut tmp1.as_file().try_clone().unwrap(), &data1).unwrap();
        std::io::Write::write_all(&mut tmp2.as_file().try_clone().unwrap(), &data2).unwrap();

        let hash1 = blake3_file(tmp1.path()).await.unwrap();
        let hash2 = blake3_file(tmp2.path()).await.unwrap();
        assert_ne!(
            hash1, hash2,
            "files differing at the tail must produce different sampled hashes"
        );
    }

    #[tokio::test]
    async fn test_blake3_directory_path_errors() {
        // Hashing a directory should error, not panic
        let tmp = tempfile::tempdir().unwrap();
        let result = blake3_file(tmp.path()).await;
        assert!(
            result.is_err(),
            "hashing a directory should produce an IO error"
        );
    }
}
