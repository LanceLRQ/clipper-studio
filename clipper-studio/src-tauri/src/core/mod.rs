pub mod clipper;      // ④ FFmpeg 切片引擎
pub mod media_server; // 本地媒体文件服务器
pub mod queue;        // ⑥ 任务调度器

pub mod danmaku;      // ⑧ 弹幕系统
pub mod storage;      // ⑤ 资源管理
pub mod watcher;      // ⑤ 目录监控

pub mod subtitle;    // ⑦ 字幕 ASS 生成
pub mod segment;     // 音频自动分段
pub mod merger;      // 视频合并
