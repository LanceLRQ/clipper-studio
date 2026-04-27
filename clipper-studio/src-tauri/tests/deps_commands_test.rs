//! commands/deps.rs 集成测试
//!
//! 由于 set_dep_custom_path / set_deps_proxy 紧耦合 AppState（含 DependencyManager），
//! 本测试聚焦其底层"配置文件持久化"语义：
//! - AppConfig save → load roundtrip
//! - "derive ffprobe from same dir" 文件系统逻辑（与 set_dep_custom_path 一致）
//! - 拒绝未知 dep_id
//! - 代理 URL 的空串/非空串语义

use clipper_studio_lib::config::AppConfig;
use std::fs;
use std::path::Path;

/// 写入临时 config_dir，加载回来，比较关键字段。
fn save_and_reload(config: &AppConfig, dir: &Path) -> AppConfig {
    config.save(dir).expect("save");
    AppConfig::load(dir)
}

#[test]
fn test_app_config_default_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let original = AppConfig::default();
    let loaded = save_and_reload(&original, dir.path());

    assert_eq!(loaded.ffmpeg.ffmpeg_path, "");
    assert_eq!(loaded.ffmpeg.ffprobe_path, "");
    assert_eq!(loaded.tools.danmaku_factory_path, "");
    assert_eq!(loaded.network.proxy_url, "");
    assert_eq!(loaded.general.log_level, "info");
}

#[test]
fn test_set_ffmpeg_path_persists() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = AppConfig::default();
    config.ffmpeg.ffmpeg_path = "/usr/local/bin/ffmpeg".to_string();
    config.ffmpeg.ffprobe_path = "/usr/local/bin/ffprobe".to_string();

    let loaded = save_and_reload(&config, dir.path());
    assert_eq!(loaded.ffmpeg.ffmpeg_path, "/usr/local/bin/ffmpeg");
    assert_eq!(loaded.ffmpeg.ffprobe_path, "/usr/local/bin/ffprobe");
}

#[test]
fn test_set_danmaku_factory_path_persists() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = AppConfig::default();
    config.tools.danmaku_factory_path = "/opt/DanmakuFactory".to_string();

    let loaded = save_and_reload(&config, dir.path());
    assert_eq!(loaded.tools.danmaku_factory_path, "/opt/DanmakuFactory");
}

#[test]
fn test_set_proxy_url_persists() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = AppConfig::default();
    config.network.proxy_url = "http://127.0.0.1:7890".to_string();

    let loaded = save_and_reload(&config, dir.path());
    assert_eq!(loaded.network.proxy_url, "http://127.0.0.1:7890");
}

#[test]
fn test_clear_proxy_url_persists() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = AppConfig::default();
    config.network.proxy_url = "http://proxy:8080".to_string();
    save_and_reload(&config, dir.path());

    config.network.proxy_url = String::new();
    let loaded = save_and_reload(&config, dir.path());
    assert!(
        loaded.network.proxy_url.is_empty(),
        "清空 proxy 应被持久化为空串"
    );
}

#[test]
fn test_independent_field_updates_dont_clobber() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = AppConfig::default();
    config.ffmpeg.ffmpeg_path = "/path/to/ffmpeg".to_string();
    config.network.proxy_url = "http://p:1".to_string();
    save_and_reload(&config, dir.path());

    // 第二次仅修改 tools 字段，验证其它字段保留
    let mut config2 = AppConfig::load(dir.path());
    config2.tools.danmaku_factory_path = "/d/f".to_string();
    let loaded = save_and_reload(&config2, dir.path());

    assert_eq!(loaded.ffmpeg.ffmpeg_path, "/path/to/ffmpeg");
    assert_eq!(loaded.network.proxy_url, "http://p:1");
    assert_eq!(loaded.tools.danmaku_factory_path, "/d/f");
}

#[test]
fn test_load_creates_default_when_file_missing() {
    let dir = tempfile::tempdir().unwrap();
    // dir 是空目录 → load 应返回默认值并创建文件
    let loaded = AppConfig::load(dir.path());
    assert_eq!(loaded.ffmpeg.ffmpeg_path, "");

    let config_path = dir.path().join("config.toml");
    assert!(config_path.exists(), "load 应创建默认 config.toml");
}

#[test]
fn test_load_falls_back_to_default_on_parse_error() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    fs::write(&config_path, "this is not valid TOML !!! [unclosed").unwrap();

    // 解析失败应返回默认值（不 panic）
    let loaded = AppConfig::load(dir.path());
    assert_eq!(loaded.ffmpeg.ffmpeg_path, "");
    assert_eq!(loaded.general.log_level, "info");
}

#[test]
fn test_partial_toml_uses_defaults_for_missing_sections() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    // 仅写 [network] 段，其它 section 应使用 default
    fs::write(&config_path, "[network]\nproxy_url = \"http://only:1\"\n").unwrap();

    let loaded = AppConfig::load(dir.path());
    assert_eq!(loaded.network.proxy_url, "http://only:1");
    assert_eq!(loaded.ffmpeg.ffmpeg_path, "");
    assert_eq!(loaded.general.log_level, "info");
    assert!(loaded.workspaces.recent.is_empty());
}

// ============== set_dep_custom_path 的「同目录推导 ffprobe」逻辑 ==============

/// 复刻 set_dep_custom_path 中的推导逻辑：
/// 给定 ffmpeg 二进制路径，若同目录存在 ffprobe(.exe)，返回该路径
fn derive_ffprobe_from_ffmpeg_dir(ffmpeg_path: &str) -> Option<String> {
    let p = Path::new(ffmpeg_path);
    let dir = p.parent()?;
    #[cfg(target_os = "windows")]
    let ffprobe = dir.join("ffprobe.exe");
    #[cfg(not(target_os = "windows"))]
    let ffprobe = dir.join("ffprobe");

    if ffprobe.exists() {
        Some(ffprobe.to_string_lossy().to_string())
    } else {
        None
    }
}

#[test]
fn test_derive_ffprobe_when_present() {
    let dir = tempfile::tempdir().unwrap();
    let ffmpeg_name = if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    let ffprobe_name = if cfg!(target_os = "windows") {
        "ffprobe.exe"
    } else {
        "ffprobe"
    };

    let ffmpeg = dir.path().join(ffmpeg_name);
    let ffprobe = dir.path().join(ffprobe_name);
    fs::write(&ffmpeg, b"x").unwrap();
    fs::write(&ffprobe, b"y").unwrap();

    let derived = derive_ffprobe_from_ffmpeg_dir(&ffmpeg.to_string_lossy());
    assert!(derived.is_some(), "同目录有 ffprobe 应被发现");
    assert!(derived.unwrap().ends_with(ffprobe_name));
}

#[test]
fn test_derive_ffprobe_returns_none_when_absent() {
    let dir = tempfile::tempdir().unwrap();
    let ffmpeg_name = if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    let ffmpeg = dir.path().join(ffmpeg_name);
    fs::write(&ffmpeg, b"x").unwrap();

    let derived = derive_ffprobe_from_ffmpeg_dir(&ffmpeg.to_string_lossy());
    assert!(derived.is_none(), "同目录无 ffprobe 应返回 None");
}

// ============== set_dep_custom_path 的 dep_id 白名单语义 ==============

/// 复刻 set_dep_custom_path 中的 dep_id 派发逻辑（仅判定是否支持，不副作用）
fn is_supported_custom_path_dep(dep_id: &str) -> bool {
    matches!(dep_id, "ffmpeg" | "danmaku-factory")
}

#[test]
fn test_unknown_dep_id_rejected() {
    assert!(!is_supported_custom_path_dep("python"));
    assert!(!is_supported_custom_path_dep("unknown"));
    assert!(!is_supported_custom_path_dep(""));
}

#[test]
fn test_known_dep_ids_accepted() {
    assert!(is_supported_custom_path_dep("ffmpeg"));
    assert!(is_supported_custom_path_dep("danmaku-factory"));
}
