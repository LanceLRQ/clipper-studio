//! "防君子" 级别的敏感配置值编码。
//!
//! ClipperStudio 是本地优先的桌面应用，完整加密会强制用户输入主密码，
//! 反复输入影响体验。这里采用 base64 编码作为最低限度的"不可见"处理：
//! 阻止用户或脚本通过 `SELECT * FROM settings_kv` 直接看到明文 API Key，
//! 但**不构成实际加密**，任何攻击者仍可解码。
//!
//! 与真正的加密方案相比，这样做的权衡：
//! - ✅ 零 UI 成本（无须主密码）
//! - ✅ 兼容旧明文数据（`deobfuscate` 对无前缀值返回原值）
//! - ❌ 对本机有读取权限的进程无防护能力（与威胁模型一致）

use base64::{engine::general_purpose::STANDARD, Engine as _};

/// 编码后的值前缀，用于区分"已编码"与"旧明文"
const OBFUSCATE_PREFIX: &str = "b64:";

/// 判断给定的 settings/plugin key 名是否应被视作敏感字段。
///
/// 判断采用关键字启发式：key 中含有 `api_key`、`token`、`password`、`secret` 等关键词即视为敏感。
pub fn is_secret_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    const KEYWORDS: &[&str] = &[
        "api_key",
        "apikey",
        "api-key",
        "token",
        "password",
        "passwd",
        "secret",
        "basic_pass",
    ];
    KEYWORDS.iter().any(|kw| lower.contains(kw))
}

/// 将明文值编码为"防君子"的 base64 形式（带 `b64:` 前缀）。
/// 空值直接返回空串，避免 DB 中出现无意义的 `b64:` 占位符。
pub fn obfuscate(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    format!("{}{}", OBFUSCATE_PREFIX, STANDARD.encode(value.as_bytes()))
}

/// 解码存储中的值。
/// - 带 `b64:` 前缀：尝试 base64 解码，失败时退回空串（视为数据损坏）
/// - 无前缀：原样返回（兼容旧明文）
pub fn deobfuscate(value: &str) -> String {
    match value.strip_prefix(OBFUSCATE_PREFIX) {
        Some(encoded) => STANDARD
            .decode(encoded)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .unwrap_or_default(),
        None => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_secret_key() {
        assert!(is_secret_key("asr_api_key"));
        assert!(is_secret_key("plugin:foo:token"));
        assert!(is_secret_key("basic_pass"));
        assert!(is_secret_key("user_password"));
        assert!(!is_secret_key("asr_url"));
        assert!(!is_secret_key("proxy_url"));
    }

    #[test]
    fn test_roundtrip() {
        let original = "sk-abcdef123456";
        let encoded = obfuscate(original);
        assert!(encoded.starts_with(OBFUSCATE_PREFIX));
        assert_ne!(encoded, original);
        assert_eq!(deobfuscate(&encoded), original);
    }

    #[test]
    fn test_empty() {
        assert_eq!(obfuscate(""), "");
        assert_eq!(deobfuscate(""), "");
    }

    #[test]
    fn test_plaintext_fallback() {
        // 旧明文应原样返回，保证兼容
        assert_eq!(deobfuscate("old-plaintext-key"), "old-plaintext-key");
    }
}
