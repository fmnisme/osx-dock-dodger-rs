# osx-dock-dodger-rs
使用rust实现的Dock Dodger

## 功能
- 拖入 `.app` 文件到窗口后自动修改 Info.plist 中的 `LSUIElement` 字段，使其不再显示 Dock 图标
- 在界面中展示已处理的应用列表，并可点击“恢复”按钮恢复 Dock 图标

## 构建
```bash
cargo build
```

> 该程序依赖 macOS 环境，Linux 下无法正常运行。
