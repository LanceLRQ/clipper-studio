use std::path::{Path, PathBuf};

/// 当目标路径已存在时，在文件名后追加 " (N)" 形式的数字后缀直到唯一。
///
/// 例如 `foo.mp4` 已存在则返回 `foo (1).mp4`，再存在则 `foo (2).mp4`，依次递增。
/// 若目标路径不存在则原样返回。
pub fn dedup_output_path(target: &Path) -> PathBuf {
    if !target.exists() {
        return target.to_path_buf();
    }

    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let stem = target
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = target
        .extension()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    for n in 1..=9999 {
        let name = if ext.is_empty() {
            format!("{} ({})", stem, n)
        } else {
            format!("{} ({}).{}", stem, n, ext)
        };
        let candidate = parent.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    // 极端情况下仍冲突，回退到时间戳后缀
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let name = if ext.is_empty() {
        format!("{}_{}", stem, ts)
    } else {
        format!("{}_{}.{}", stem, ts, ext)
    };
    parent.join(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn returns_original_when_not_exists() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("foo.mp4");
        let got = dedup_output_path(&target);
        assert_eq!(got, target);
    }

    #[test]
    fn appends_numeric_suffix_on_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("foo.mp4");
        fs::write(&target, b"x").unwrap();

        let got = dedup_output_path(&target);
        assert_eq!(got.file_name().unwrap().to_string_lossy(), "foo (1).mp4");
    }

    #[test]
    fn increments_until_unique() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("foo.mp4"), b"x").unwrap();
        fs::write(dir.path().join("foo (1).mp4"), b"x").unwrap();
        fs::write(dir.path().join("foo (2).mp4"), b"x").unwrap();

        let got = dedup_output_path(&dir.path().join("foo.mp4"));
        assert_eq!(got.file_name().unwrap().to_string_lossy(), "foo (3).mp4");
    }

    #[test]
    fn handles_no_extension() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("README");
        fs::write(&target, b"x").unwrap();

        let got = dedup_output_path(&target);
        assert_eq!(got.file_name().unwrap().to_string_lossy(), "README (1)");
    }

    #[test]
    fn preserves_compound_extension_only_last() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("foo.bar.mp4");
        fs::write(&target, b"x").unwrap();

        let got = dedup_output_path(&target);
        // file_stem 取到最后一个点之前
        assert_eq!(
            got.file_name().unwrap().to_string_lossy(),
            "foo.bar (1).mp4"
        );
    }
}
