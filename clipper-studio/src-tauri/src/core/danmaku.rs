use std::path::Path;

use serde::{Deserialize, Serialize};

/// Danmaku display mode
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DanmakuMode {
    /// Right-to-left scrolling (mode 1, 2, 3)
    Scroll,
    /// Fixed at bottom (mode 4)
    Bottom,
    /// Fixed at top (mode 5)
    Top,
}

/// A single danmaku item parsed from Bilibili XML
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DanmakuItem {
    /// Appearance time in milliseconds (relative to video start)
    pub time_ms: i64,
    /// Danmaku text content
    pub text: String,
    /// Display mode
    pub mode: DanmakuMode,
    /// RGB color (decimal)
    pub color: u32,
    /// Font size
    pub font_size: u16,
}

/// Parse a Bilibili-format XML danmaku file.
///
/// XML format: `<d p="time,mode,fontSize,color,timestamp,pool,uid,dbid">text</d>`
/// Only processes regular danmaku (modes 1-5), ignores special danmaku (mode 7+).
pub fn parse_bilibili_xml(path: &Path) -> Result<Vec<DanmakuItem>, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read danmaku file: {}", e))?;

    parse_bilibili_xml_str(&content)
}

/// Parse XML content string into danmaku items
pub fn parse_bilibili_xml_str(xml: &str) -> Result<Vec<DanmakuItem>, String> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(xml);
    let mut items = Vec::new();
    let mut in_d_element = false;
    let mut current_params = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) if e.name().as_ref() == b"d" => {
                // Extract the "p" attribute
                for attr in e.attributes().flatten() {
                    if attr.key.as_ref() == b"p" {
                        current_params =
                            String::from_utf8_lossy(&attr.value).to_string();
                        in_d_element = true;
                    }
                }
            }
            Ok(Event::Text(ref e)) if in_d_element => {
                let text = e.unescape().unwrap_or_default().trim().to_string();
                if !text.is_empty() {
                    if let Some(item) = parse_d_element(&current_params, &text) {
                        items.push(item);
                    }
                }
                in_d_element = false;
            }
            Ok(Event::End(ref e)) if e.name().as_ref() == b"d" => {
                in_d_element = false;
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                tracing::warn!("XML parse error at position {}: {}", reader.error_position(), e);
                break;
            }
            _ => {}
        }
    }

    // Sort by time
    items.sort_by_key(|d| d.time_ms);

    tracing::info!("Parsed {} danmaku items from XML", items.len());
    Ok(items)
}

/// Parse a single `<d p="...">text</d>` element
fn parse_d_element(params: &str, text: &str) -> Option<DanmakuItem> {
    let parts: Vec<&str> = params.split(',').collect();
    if parts.len() < 4 {
        return None;
    }

    // Field 1: appearance time (seconds, float)
    let time_secs: f64 = parts[0].parse().ok()?;
    let time_ms = (time_secs * 1000.0) as i64;

    // Field 2: mode
    let mode_int: u16 = parts[1].parse().ok()?;
    let mode = match mode_int {
        1 | 2 | 3 => DanmakuMode::Scroll, // 1=R2L, 2=L2R (rare), 3=top(rare variant)
        4 => DanmakuMode::Bottom,
        5 => DanmakuMode::Top,
        _ => return None, // Skip special danmaku (mode 7, 8, 9, etc.)
    };

    // Field 3: font size
    let font_size: u16 = parts[2].parse().unwrap_or(25);

    // Field 4: color (decimal RGB)
    let color: u32 = parts[3].parse().unwrap_or(0xFFFFFF);

    Some(DanmakuItem {
        time_ms,
        text: text.to_string(),
        mode,
        color,
        font_size,
    })
}

/// Compute danmaku density per time window (for heatmap overlay).
///
/// Returns a vector of counts, one per window.
/// `window_ms`: window size in milliseconds (e.g. 5000 for 5 seconds)
pub fn compute_density(items: &[DanmakuItem], duration_ms: i64, window_ms: i64) -> Vec<u32> {
    if duration_ms <= 0 || window_ms <= 0 {
        return Vec::new();
    }

    let num_windows = ((duration_ms + window_ms - 1) / window_ms) as usize;
    let mut density = vec![0u32; num_windows];

    for item in items {
        let idx = (item.time_ms / window_ms) as usize;
        if idx < density.len() {
            density[idx] += 1;
        }
    }

    density
}

/// Normalize density values to 0.0~1.0 range
pub fn normalize_density(density: &[u32]) -> Vec<f32> {
    let max = density.iter().cloned().max().unwrap_or(0);
    if max == 0 {
        return vec![0.0; density.len()];
    }
    density.iter().map(|&v| v as f32 / max as f32).collect()
}

/// Options for DanmakuFactory ASS conversion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DanmakuAssOptions {
    /// Video resolution width
    #[serde(default = "default_width")]
    pub width: u32,
    /// Video resolution height
    #[serde(default = "default_height")]
    pub height: u32,
    /// Scrolling danmaku speed (seconds to cross screen)
    #[serde(default = "default_scroll_time")]
    pub scroll_time: f32,
    /// Font size
    #[serde(default = "default_font_size")]
    pub font_size: u32,
    /// Opacity (1-255, 255=opaque)
    #[serde(default = "default_opacity")]
    pub opacity: u32,
    /// Density: -1=non-overlap, 0=unlimited
    #[serde(default)]
    pub density: i32,
}

fn default_width() -> u32 { 1920 }
fn default_height() -> u32 { 1080 }
fn default_scroll_time() -> f32 { 12.0 }
fn default_font_size() -> u32 { 38 }
fn default_opacity() -> u32 { 180 }

impl Default for DanmakuAssOptions {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            scroll_time: 12.0,
            font_size: 38,
            opacity: 180,
            density: -1,
        }
    }
}

/// Convert danmaku XML to ASS using DanmakuFactory CLI
pub fn convert_to_ass(
    danmaku_factory_path: &str,
    input_xml: &Path,
    output_ass: &Path,
    options: &DanmakuAssOptions,
) -> Result<(), String> {
    if danmaku_factory_path.is_empty() {
        return Err("DanmakuFactory not available".to_string());
    }

    let status = std::process::Command::new(danmaku_factory_path)
        .args([
            "-o", "ass",
            &output_ass.to_string_lossy(),
            "-i", "xml",
            &input_xml.to_string_lossy(),
            "-r", &format!("{}x{}", options.width, options.height),
            "-s", &options.scroll_time.to_string(),
            "-S", &options.font_size.to_string(),
            "-O", &options.opacity.to_string(),
            "-d", &options.density.to_string(),
            "--force",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .status()
        .map_err(|e| format!("Failed to run DanmakuFactory: {}", e))?;

    if !status.success() {
        return Err(format!("DanmakuFactory exited with code {:?}", status.code()));
    }

    Ok(())
}

/// Filter danmaku items to a specific time range and shift timestamps.
///
/// - Only keeps items where `start_ms <= time_ms < end_ms`
/// - Shifts all timestamps by `-start_ms` so the output starts at 0
pub fn filter_danmaku_by_range(
    items: &[DanmakuItem],
    start_ms: i64,
    end_ms: i64,
) -> Vec<DanmakuItem> {
    items
        .iter()
        .filter(|d| d.time_ms >= start_ms && d.time_ms < end_ms)
        .map(|d| DanmakuItem {
            time_ms: d.time_ms - start_ms,
            text: d.text.clone(),
            mode: d.mode,
            color: d.color,
            font_size: d.font_size,
        })
        .collect()
}

/// Write danmaku items as a minimal Bilibili XML file.
///
/// Produces XML that DanmakuFactory can read:
/// ```xml
/// <?xml version="1.0" encoding="utf-8"?>
/// <i>
///   <d p="time,mode,fontSize,color,0,0,0,0">text</d>
///   ...
/// </i>
/// ```
pub fn write_bilibili_xml(items: &[DanmakuItem], output: &Path) -> Result<(), String> {
    use std::io::Write;

    let mut file = std::fs::File::create(output)
        .map_err(|e| format!("Failed to create danmaku XML: {}", e))?;

    writeln!(file, "<?xml version=\"1.0\" encoding=\"utf-8\"?>")
        .map_err(|e| e.to_string())?;
    writeln!(file, "<i>").map_err(|e| e.to_string())?;

    for item in items {
        let mode_int: u16 = match item.mode {
            DanmakuMode::Scroll => 1,
            DanmakuMode::Bottom => 4,
            DanmakuMode::Top => 5,
        };
        let time_secs = item.time_ms as f64 / 1000.0;
        // Escape XML special characters in text
        let escaped_text = item
            .text
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;");
        writeln!(
            file,
            "<d p=\"{:.3},{},{},{},0,0,0,0\">{}</d>",
            time_secs, mode_int, item.font_size, item.color, escaped_text,
        )
        .map_err(|e| e.to_string())?;
    }

    writeln!(file, "</i>").map_err(|e| e.to_string())?;

    tracing::info!(
        "Wrote {} danmaku items to {}",
        items.len(),
        output.display(),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_d_element() {
        let item = parse_d_element("1.5,1,25,16777215,1234567890,0,12345,0", "测试弹幕").unwrap();
        assert_eq!(item.time_ms, 1500);
        assert_eq!(item.text, "测试弹幕");
        assert_eq!(item.mode, DanmakuMode::Scroll);
        assert_eq!(item.color, 16777215);
        assert_eq!(item.font_size, 25);
    }

    #[test]
    fn test_parse_bottom_danmaku() {
        let item = parse_d_element("10.0,4,25,255,0,0,0,0", "底部弹幕").unwrap();
        assert_eq!(item.mode, DanmakuMode::Bottom);
    }

    #[test]
    fn test_parse_top_danmaku() {
        let item = parse_d_element("10.0,5,25,255,0,0,0,0", "顶部弹幕").unwrap();
        assert_eq!(item.mode, DanmakuMode::Top);
    }

    #[test]
    fn test_skip_special_danmaku() {
        assert!(parse_d_element("10.0,7,25,255,0,0,0,0", "特殊弹幕").is_none());
        assert!(parse_d_element("10.0,9,25,255,0,0,0,0", "高级弹幕").is_none());
    }

    #[test]
    fn test_parse_xml_str() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<i>
<d p="0.5,1,25,16777215,1234567890,0,12345,0">第一条</d>
<d p="1.0,4,25,255,1234567890,0,12345,0">底部弹幕</d>
<d p="2.0,5,25,16711680,1234567890,0,12345,0">顶部弹幕</d>
</i>"#;
        let items = parse_bilibili_xml_str(xml).unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].text, "第一条");
        assert_eq!(items[1].mode, DanmakuMode::Bottom);
        assert_eq!(items[2].mode, DanmakuMode::Top);
    }

    #[test]
    fn test_filter_danmaku_by_range() {
        let items = vec![
            DanmakuItem { time_ms: 500, text: "a".into(), mode: DanmakuMode::Scroll, color: 0, font_size: 25 },
            DanmakuItem { time_ms: 1500, text: "b".into(), mode: DanmakuMode::Scroll, color: 0, font_size: 25 },
            DanmakuItem { time_ms: 2500, text: "c".into(), mode: DanmakuMode::Scroll, color: 0, font_size: 25 },
            DanmakuItem { time_ms: 3500, text: "d".into(), mode: DanmakuMode::Scroll, color: 0, font_size: 25 },
        ];
        let filtered = filter_danmaku_by_range(&items, 1000, 3000);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].text, "b");
        assert_eq!(filtered[0].time_ms, 500); // 1500 - 1000
        assert_eq!(filtered[1].text, "c");
        assert_eq!(filtered[1].time_ms, 1500); // 2500 - 1000
    }

    #[test]
    fn test_write_bilibili_xml() {
        let items = vec![
            DanmakuItem { time_ms: 1000, text: "hello".into(), mode: DanmakuMode::Scroll, color: 16777215, font_size: 25 },
            DanmakuItem { time_ms: 2000, text: "<test>&".into(), mode: DanmakuMode::Top, color: 255, font_size: 30 },
        ];
        let dir = std::env::temp_dir();
        let path = dir.join("test_danmaku_write.xml");
        write_bilibili_xml(&items, &path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("<d p=\"1.000,1,25,16777215,0,0,0,0\">hello</d>"));
        assert!(content.contains("<d p=\"2.000,5,30,255,0,0,0,0\">&lt;test&gt;&amp;</d>"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_compute_density() {
        let items = vec![
            DanmakuItem { time_ms: 100, text: "a".into(), mode: DanmakuMode::Scroll, color: 0, font_size: 25 },
            DanmakuItem { time_ms: 200, text: "b".into(), mode: DanmakuMode::Scroll, color: 0, font_size: 25 },
            DanmakuItem { time_ms: 1500, text: "c".into(), mode: DanmakuMode::Scroll, color: 0, font_size: 25 },
        ];
        let density = compute_density(&items, 3000, 1000);
        assert_eq!(density, vec![2, 1, 0]);
    }

    // ==================== normalize_density ====================

    #[test]
    fn test_normalize_all_zeros() {
        assert_eq!(normalize_density(&[0, 0, 0]), vec![0.0, 0.0, 0.0]);
    }

    #[test]
    fn test_normalize_basic() {
        let result = normalize_density(&[2, 10, 5]);
        assert!((result[0] - 0.2).abs() < 0.001);
        assert!((result[1] - 1.0).abs() < 0.001);
        assert!((result[2] - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_normalize_single() {
        assert_eq!(normalize_density(&[42]), vec![1.0]);
    }

    #[test]
    fn test_normalize_empty() {
        assert!(normalize_density(&[]).is_empty());
    }

    // ==================== write + read roundtrip ====================

    #[test]
    fn test_write_read_roundtrip() {
        let items = vec![
            DanmakuItem { time_ms: 1000, text: "你好".into(), mode: DanmakuMode::Scroll, color: 16777215, font_size: 25 },
            DanmakuItem { time_ms: 2000, text: "世界".into(), mode: DanmakuMode::Top, color: 255, font_size: 30 },
            DanmakuItem { time_ms: 3000, text: "底部".into(), mode: DanmakuMode::Bottom, color: 16711680, font_size: 25 },
        ];
        let dir = std::env::temp_dir();
        let path = dir.join("test_roundtrip.xml");
        write_bilibili_xml(&items, &path).unwrap();

        let parsed = parse_bilibili_xml(&path).unwrap();
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].text, "你好");
        assert_eq!(parsed[0].mode, DanmakuMode::Scroll);
        assert_eq!(parsed[1].mode, DanmakuMode::Top);
        assert_eq!(parsed[2].mode, DanmakuMode::Bottom);
        assert_eq!(parsed[2].color, 16711680);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_xml_escape_roundtrip() {
        let items = vec![
            DanmakuItem { time_ms: 500, text: "<script>&alert(\"xss\")</script>".into(), mode: DanmakuMode::Scroll, color: 0, font_size: 25 },
        ];
        let dir = std::env::temp_dir();
        let path = dir.join("test_escape_roundtrip.xml");
        write_bilibili_xml(&items, &path).unwrap();

        let parsed = parse_bilibili_xml(&path).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].text, "<script>&alert(\"xss\")</script>");

        let _ = std::fs::remove_file(&path);
    }
}
