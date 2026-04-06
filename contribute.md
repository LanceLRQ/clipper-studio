# 如何为 ClipperStudio 开发插件

> 最后更新：2026-04-06

---

## 插件类型

| 类型 | Rust 代码 | Tauri API | 前端 UI | 分发方式 |
|------|----------|-----------|---------|---------|
| 🌱 纯 UI | ❌ | ❌ | ✅ React 组件 | 打包 JS 文件 |
| 📦 HTTP/Stdio 服务 | ❌ | ❌ | ✅ React 组件 | 独立进程 |
| 🔧 Builtin Rust | ✅ | ✅ | ✅ React 组件 | 编译进主程序 |

---

## 🌱 方式 1：纯 UI 插件（推荐入门）

适合：会 React 的开发者。只需贡献前端界面，无需了解 Rust 或 Tauri。

### 1. 创建插件目录

```
clipper-studio/plugins/
└── my-ui-plugin/
    ├── plugin.json
    ├── src/
    │   └── settings.tsx
    └── dist/
        └── settings.js    # Vite 打包产物
```

### 2. 编写 plugin.json

```json
{
  "id": "ui.my-plugin",
  "name": "My Plugin",
  "type": "recorder",
  "version": "1.0.0",
  "frontend": {
    "entry": "dist/settings.js",
    "target": "settings"
  },
  "config_schema": {
    "api_url": {
      "type": "string",
      "default": "http://127.0.0.1:8080",
      "description": "服务地址"
    }
  }
}
```

### 3. 开发 React 组件

```tsx
// src/settings.tsx
import React from 'react';

export function registerSettings(
  container: HTMLElement,
  ctx: { pluginId: string; pluginDir: string }
) {
  // ctx 包含插件信息，可以在组件中使用
  const root = ReactDOM.createRoot(container);
  root.render(<MyPluginUI pluginId={ctx.pluginId} />);
}
```

### 4. Vite 打包配置

使用 `@clipper-studio/plugin-vite-config`（待发布）或手动配置：

```typescript
// vite.config.ts
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  build: {
    lib: {
      entry: 'src/settings.tsx',
      formats: ['iife'],
      name: 'ClipperStudioPlugin',
      fileName: () => 'settings.js',
    },
  },
});
```

### 5. 分发

将整个插件目录分发给用户，用户将其放入插件目录即可。

---

## 📦 方式 2：HTTP/Stdio 服务插件

适合：有后端开发经验的团队。插件是一个独立运行的服务，通过 HTTP 或 stdio 与主程序通信。

### 1. 开发独立 HTTP 服务

实现以下端点（以 HTTP 为例）：

```
POST /<action>   # 处理主程序的请求
GET  /health     # 健康检查
```

请求格式：
```json
{
  "action": "status",
  "payload": {}
}
```

响应格式：
```json
{
  "result": { "connected": true }
}
# 或
{
  "error": "something went wrong"
}
```

### 2. 编写 plugin.json

```json
{
  "id": "service.my-service",
  "name": "My Service",
  "version": "1.0.0",
  "transport": "http",
  "port": 8080,
  "health_endpoint": "/health",
  "config_schema": {
    "api_url": {
      "type": "string",
      "default": "http://127.0.0.1:8080"
    }
  }
}
```

### 3. 配置网络地址和认证

在插件配置的 `config_schema` 中定义 `api_url`、`api_key` 等字段，用户在设置页填写。

---

## 🔧 方式 3：Builtin Rust 插件

适合：ClipperStudio 核心 contributor。需要 Fork 主仓库，在 `crates/` 下创建插件 crate。

### 1. Fork 主仓库

```bash
git clone https://github.com/LanceLRQ/clipper-studio.git
cd clipper-studio/clipper-studio
```

### 2. 创建插件 crate

```
crates/
└── clipper-studio-plugin-myplugin/
    ├── Cargo.toml
    └── src/
        ├── lib.rs
        └── my_plugin.rs
```

### 3. 实现 PluginInstance trait

```rust
use clipper_studio_plugin_core::*;

pub struct MyPlugin {
    manifest: PluginManifest,
}

#[async_trait::async_trait]
impl PluginInstance for MyPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn initialize(&self) -> Result<(), PluginError> {
        tracing::info!("Initializing my plugin");
        Ok(())
    }

    async fn shutdown(&self) -> Result<(), PluginError> {
        Ok(())
    }

    async fn handle_request(&self, action: &str, payload: Value) -> Result<Value, PluginError> {
        match action {
            "do_something" => { /* 处理请求 */ Ok(serde_json::json!({ "ok": true })) }
            _ => Err(PluginError::UnsupportedAction(action.to_string())),
        }
    }
}

pub struct MyPluginBuilder;

impl PluginBuilder for MyPluginBuilder {
    fn id(&self) -> &'static str {
        "builtin.my-plugin"
    }

    fn build(&self) -> Result<Box<dyn PluginInstance>, PluginError> {
        Ok(Box::new(MyPlugin {
            manifest: PluginManifest {
                id: "builtin.my-plugin".to_string(),
                name: "My Plugin".to_string(),
                plugin_type: PluginType::Recorder,
                version: "1.0.0".to_string(),
                api_version: 1,
                transport: Transport::Builtin,
                managed: false,
                // ... 其他字段
                config_schema: Default::default(),
                description: Some("My builtin plugin".to_string()),
                frontend: None,
            },
        }))
    }
}
```

### 4. 注册到 workspace

**Cargo.toml（项目根目录）** 添加新成员：

```toml
[workspace]
members = [
    "src-tauri",
    "crates/clipper-studio-plugin-core",
    "crates/clipper-studio-plugin-myplugin",  # 新增
]
```

**src-tauri/Cargo.toml** 添加依赖：

```toml
[dependencies]
clipper-studio-plugin-myplugin = { path = "../crates/clipper-studio-plugin-myplugin", optional = true }

[features]
default = []
builtin-plugins = ["dep:clipper-studio-plugin-myplugin"]
```

**lib.rs** 注册插件：

```rust
let mut registry = PluginRegistry::new();
registry.register(clipper_studio_plugin_myplugin::MyPluginBuilder::new());
```

### 5. 提交 PR

测试通过后提交 Pull Request，合并后插件将成为主程序的一部分。

---

## 插件通用配置系统

所有插件都可以通过 `config_schema` 定义配置项，主程序会根据 schema 自动渲染表单。

### 支持的字段类型

| type | 渲染控件 |
|------|---------|
| `string` | 文本输入框 |
| `boolean` | 是/否 选择 |

### 配置存储

插件配置存在 `settings_kv` 表，key 格式为 `plugin:{plugin_id}:{config_key}`。

---

## 验证插件

1. 将插件目录放入 `~/Library/Application Support/com.clipper-studio.clipper-studio/plugins/`（macOS）
2. 打开应用 → 设置页 → 插件配置区域
3. 填写配置 → 保存
4. 在插件管理页加载插件
