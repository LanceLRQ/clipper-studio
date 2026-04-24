//! 公共输入验证辅助函数，供 Tauri IPC 命令使用。
//!
//! 设计目标：
//! - 集中 ID / 字符串 / URL 的边界与长度校验逻辑，避免分散重复
//! - 提供清晰的错误消息便于前端展示
//! - 校验失败返回 `Result::Err(String)`，与现有 command 签名一致

/// 字符串参数最大长度（通用名称、标题等）
pub const MAX_NAME_LEN: usize = 255;

/// 长文本字段最大长度（描述、URL 等）
pub const MAX_URL_LEN: usize = 2048;

/// 验证数据库 ID 合法：必须为正整数。
///
/// 用于所有接收 `video_id` / `clip_id` / `task_id` 等参数的 IPC command。
pub fn validate_id(id: i64, label: &str) -> Result<(), String> {
    if id <= 0 {
        return Err(format!("无效的 {}: 必须为正整数", label));
    }
    Ok(())
}

/// 验证名称类字符串：非空且不超过 [`MAX_NAME_LEN`]。
///
/// 会修剪前后空白后再判断非空，但返回给数据库的值由调用者自行决定是否 trim。
pub fn validate_name(s: &str, label: &str) -> Result<(), String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Err(format!("{} 不能为空", label));
    }
    if s.chars().count() > MAX_NAME_LEN {
        return Err(format!("{} 长度不能超过 {} 个字符", label, MAX_NAME_LEN));
    }
    Ok(())
}

/// 验证可选名称字段（None 或 Some）。仅当为 Some 且非空时校验长度。
pub fn validate_optional_name(s: Option<&str>, label: &str) -> Result<(), String> {
    match s {
        Some(v) if !v.trim().is_empty() => {
            if v.chars().count() > MAX_NAME_LEN {
                return Err(format!("{} 长度不能超过 {} 个字符", label, MAX_NAME_LEN));
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// 验证 URL 字符串长度上限。
pub fn validate_url_length(s: &str, label: &str) -> Result<(), String> {
    if s.chars().count() > MAX_URL_LEN {
        return Err(format!("{} 长度不能超过 {} 个字符", label, MAX_URL_LEN));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_id_rejects_zero_and_negative() {
        assert!(validate_id(0, "video_id").is_err());
        assert!(validate_id(-1, "video_id").is_err());
        assert!(validate_id(1, "video_id").is_ok());
    }

    #[test]
    fn validate_name_rejects_empty_and_too_long() {
        assert!(validate_name("", "name").is_err());
        assert!(validate_name("   ", "name").is_err());
        assert!(validate_name("ok", "name").is_ok());
        let long = "a".repeat(MAX_NAME_LEN + 1);
        assert!(validate_name(&long, "name").is_err());
    }

    #[test]
    fn validate_optional_name_allows_none() {
        assert!(validate_optional_name(None, "title").is_ok());
        assert!(validate_optional_name(Some(""), "title").is_ok());
        assert!(validate_optional_name(Some("t"), "title").is_ok());
        let long = "a".repeat(MAX_NAME_LEN + 1);
        assert!(validate_optional_name(Some(&long), "title").is_err());
    }
}
