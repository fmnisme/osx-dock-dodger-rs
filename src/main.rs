use std::path::{Path, PathBuf};

use plist::Value;
use serde::Deserialize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::WindowBuilder;
use wry::{DragDropEvent, WebView, WebViewBuilder, http::Request};

#[derive(Debug, Clone)]
struct ManagedApp {
    path: PathBuf,
}

#[derive(Debug)]
enum UserEvent {
    Add(PathBuf),
    Restore(PathBuf),
}

#[derive(Deserialize)]
struct IpcRequest {
    cmd: String,
    path: String,
}

fn hide_dock_icon(app: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let plist_path = app.join("Contents/Info.plist");
    let mut plist = Value::from_file(&plist_path)?;
    if let Value::Dictionary(ref mut dict) = plist {
        dict.insert("LSUIElement".into(), Value::String("1".into()));
        plist::to_file_xml(plist_path, &plist)?;
    }
    Ok(())
}

fn restore_dock_icon(app: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let plist_path = app.join("Contents/Info.plist");
    let mut plist = Value::from_file(&plist_path)?;
    if let Value::Dictionary(ref mut dict) = plist {
        dict.insert("LSUIElement".into(), Value::String("0".into()));
        plist::to_file_xml(plist_path, &plist)?;
    }
    Ok(())
}

fn is_app_bundle(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("app"))
        .unwrap_or(false)
}

fn js_add_app(path: &str) -> String {
    format!("addApp({});", serde_json::to_string(path).unwrap())
}

fn rebuild_list(webview: &WebView, apps: &[ManagedApp]) {
    let mut script = String::from("document.getElementById('list').innerHTML='';");
    for app in apps {
        script.push_str(&js_add_app(app.path.to_string_lossy().as_ref()));
    }
    script.push_str("toggleEmptyState();");
    let _ = webview.evaluate_script(&script);
}

fn main() {
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let window = WindowBuilder::new()
        .with_title("Dock Dodger")
        .build(&event_loop)
        .unwrap();

    let html = r#"
    <!DOCTYPE html>
    <html lang="zh-CN">
      <head>
        <meta charset="utf-8">
        <meta name="viewport" content="width=device-width, initial-scale=1">
        <title>Dock Dodger</title>
        <style>
          :root {
            color-scheme: light dark;
            font-family: -apple-system, BlinkMacSystemFont, "SF Pro Display", "SF Pro Text", "Helvetica Neue", Helvetica, Arial, sans-serif;
          }

          * {
            box-sizing: border-box;
          }

          body {
            margin: 0;
            min-height: 100vh;
            display: flex;
            align-items: center;
            justify-content: center;
            padding: 32px;
            background: linear-gradient(135deg, #dbeafe 0%, #ede9fe 45%, #e0f2fe 100%);
            color: #0f172a;
          }

          .wrapper {
            width: min(560px, 100%);
            background: rgba(255, 255, 255, 0.85);
            border-radius: 24px;
            box-shadow: 0 20px 45px rgba(15, 23, 42, 0.15);
            padding: 32px 36px;
            backdrop-filter: blur(18px);
          }

          .hero {
            display: flex;
            align-items: center;
            gap: 18px;
            margin-bottom: 24px;
          }

          .icon-circle {
            width: 64px;
            height: 64px;
            border-radius: 50%;
            display: flex;
            align-items: center;
            justify-content: center;
            font-size: 32px;
            color: #ffffff;
            background: linear-gradient(135deg, #60a5fa, #6366f1);
            box-shadow: 0 15px 35px rgba(99, 102, 241, 0.35);
            flex-shrink: 0;
          }

          h1 {
            margin: 0;
            font-size: 28px;
            font-weight: 700;
            letter-spacing: 0.3px;
          }

          .subtitle {
            margin: 10px 0 0;
            color: #475569;
            line-height: 1.6;
          }

          .empty-state {
            display: flex;
            flex-direction: column;
            align-items: center;
            justify-content: center;
            gap: 12px;
            text-align: center;
            padding: 40px 24px;
            border-radius: 20px;
            border: 2px dashed rgba(59, 130, 246, 0.35);
            background: rgba(59, 130, 246, 0.08);
            color: #475569;
            margin-bottom: 28px;
            transition: border-color 0.25s ease, transform 0.25s ease, background 0.25s ease;
          }

          .empty-state.hidden {
            display: none;
          }

          .empty-icon {
            font-size: 36px;
          }

          .empty-state h2 {
            margin: 0;
            font-size: 20px;
            font-weight: 600;
            color: #1d4ed8;
          }

          .empty-state p {
            margin: 0;
            line-height: 1.6;
            max-width: 360px;
          }

          .app-list {
            list-style: none;
            margin: 0;
            padding: 0;
            display: flex;
            flex-direction: column;
            gap: 16px;
          }

          .app-item {
            display: flex;
            align-items: flex-start;
            justify-content: space-between;
            gap: 16px;
            background: rgba(248, 250, 252, 0.95);
            border: 1px solid rgba(148, 163, 184, 0.25);
            border-radius: 18px;
            padding: 16px 20px;
            box-shadow: 0 10px 25px rgba(15, 23, 42, 0.12);
            transition: transform 0.18s ease, box-shadow 0.18s ease;
          }

          .app-item:hover {
            transform: translateY(-2px);
            box-shadow: 0 18px 32px rgba(59, 130, 246, 0.18);
          }

          .app-info {
            display: flex;
            flex-direction: column;
            gap: 6px;
            min-width: 0;
          }

          .app-name {
            font-weight: 600;
            font-size: 17px;
            color: #1d4ed8;
            letter-spacing: 0.2px;
          }

          .app-path {
            font-size: 13px;
            color: #64748b;
            word-break: break-all;
          }

          .restore-btn {
            border: none;
            padding: 10px 18px;
            border-radius: 999px;
            font-weight: 600;
            font-size: 14px;
            background: linear-gradient(135deg, #6366f1, #3b82f6);
            color: #ffffff;
            cursor: pointer;
            box-shadow: 0 12px 24px rgba(59, 130, 246, 0.28);
            transition: transform 0.18s ease, box-shadow 0.18s ease, filter 0.18s ease;
            flex-shrink: 0;
          }

          .restore-btn:hover {
            transform: translateY(-1px);
            box-shadow: 0 16px 32px rgba(37, 99, 235, 0.35);
            filter: brightness(1.03);
          }

          .restore-btn:active {
            transform: translateY(0);
            box-shadow: 0 8px 18px rgba(37, 99, 235, 0.35);
          }

          .hint {
            margin-top: 30px;
            font-size: 12px;
            text-align: center;
            color: #64748b;
            line-height: 1.6;
          }

          body.dragging .empty-state {
            border-color: rgba(37, 99, 235, 0.75);
            background: rgba(59, 130, 246, 0.15);
            transform: scale(1.01);
          }

          @media (prefers-color-scheme: dark) {
            body {
              background: radial-gradient(circle at top, #0f172a, #020617 65%);
              color: #e2e8f0;
            }

            .wrapper {
              background: rgba(15, 23, 42, 0.78);
              box-shadow: 0 22px 50px rgba(2, 6, 23, 0.65);
            }

            .subtitle {
              color: #cbd5f5;
            }

            .empty-state {
              border-color: rgba(96, 165, 250, 0.55);
              background: rgba(59, 130, 246, 0.16);
              color: #cbd5f5;
            }

            .empty-state h2 {
              color: #93c5fd;
            }

            .app-item {
              background: rgba(15, 23, 42, 0.9);
              border-color: rgba(148, 163, 184, 0.2);
              box-shadow: 0 16px 28px rgba(2, 6, 23, 0.6);
            }

            .app-path {
              color: #94a3b8;
            }

            .hint {
              color: #94a3b8;
            }
          }
        </style>
      </head>
      <body>
        <main class="wrapper">
          <header class="hero">
            <div class="icon-circle">🛶</div>
            <div>
              <h1>Dock Dodger</h1>
              <p class="subtitle">将 .app 包拖放到下方区域即可隐藏 Dock 图标，恢复后会立刻重新显示。</p>
            </div>
          </header>
          <section id="empty-state" class="empty-state">
            <div class="empty-icon">📦</div>
            <h2>把应用拖到这里</h2>
            <p>支持 macOS 的 .app 包。放下后会自动修改 Info.plist 中的 LSUIElement 字段。</p>
          </section>
          <ul id="list" class="app-list"></ul>
          <footer class="hint">
            <p>提示：恢复按钮会撤销隐藏效果，并刷新列表。若操作失败，请查看终端日志。</p>
          </footer>
        </main>
        <script>
          function extractAppName(path) {
            if (!path) return "";
            const segments = path.split("/").filter(Boolean);
            if (segments.length === 0) {
              return path;
            }
            const last = segments[segments.length - 1];
            return last.replace(/\.app$/i, "");
          }

          function toggleEmptyState() {
            const list = document.getElementById("list");
            const emptyState = document.getElementById("empty-state");
            if (!list || !emptyState) {
              return;
            }
            if (list.children.length === 0) {
              emptyState.classList.remove("hidden");
            } else {
              emptyState.classList.add("hidden");
            }
          }

          function createRestoreButton(path) {
            const button = document.createElement("button");
            button.className = "restore-btn";
            button.type = "button";
            button.textContent = "恢复显示";
            button.addEventListener("click", function () {
              window.ipc.postMessage(JSON.stringify({ cmd: "restore", path }));
            });
            return button;
          }

          function addApp(path) {
            const list = document.getElementById("list");
            if (!list) {
              return;
            }

            const item = document.createElement("li");
            item.className = "app-item";

            const info = document.createElement("div");
            info.className = "app-info";

            const name = document.createElement("div");
            name.className = "app-name";
            name.textContent = extractAppName(path);

            const fullPath = document.createElement("div");
            fullPath.className = "app-path";
            fullPath.textContent = path;

            info.appendChild(name);
            info.appendChild(fullPath);

            item.appendChild(info);
            item.appendChild(createRestoreButton(path));
            list.appendChild(item);

            toggleEmptyState();
          }

          document.addEventListener("DOMContentLoaded", function () {
            toggleEmptyState();
          });

          document.addEventListener("dragover", function (event) {
            event.preventDefault();
            document.body.classList.add("dragging");
          });

          document.addEventListener("dragenter", function () {
            document.body.classList.add("dragging");
          });

          document.addEventListener("dragleave", function (event) {
            if (event.target === document.body || event.clientX <= 0 || event.clientY <= 0 || event.clientX >= window.innerWidth || event.clientY >= window.innerHeight) {
              document.body.classList.remove("dragging");
            }
          });

          document.addEventListener("drop", function (event) {
            event.preventDefault();
            document.body.classList.remove("dragging");
          });

          document.addEventListener("dragend", function () {
            document.body.classList.remove("dragging");
          });
        </script>
      </body>
    </html>
    "#;

    let drag_proxy = proxy.clone();
    let ipc_proxy = proxy.clone();

    let webview = WebViewBuilder::new(&window)
        .with_html(html)
        .with_drag_drop_handler(move |event| {
            if let DragDropEvent::Drop { paths, .. } = event {
                for path in paths {
                    let display = path.display().to_string();
                    if is_app_bundle(&path) {
                        println!("[DragDrop] 收到来自 Finder 的 .app：{}", display);
                        let _ = drag_proxy.send_event(UserEvent::Add(path));
                    } else {
                        println!("[DragDrop] 忽略非 .app 文件：{}", display);
                    }
                }
                true
            } else {
                false
            }
        })
        .with_ipc_handler(move |req: Request<String>| {
            if let Ok(data) = serde_json::from_str::<IpcRequest>(req.body()) {
                if data.cmd == "restore" {
                    println!("[IPC] 收到恢复请求：{}", data.path);
                    let _ = ipc_proxy.send_event(UserEvent::Restore(PathBuf::from(data.path)));
                }
            }
        })
        .build()
        .unwrap();

    let mut apps: Vec<ManagedApp> = Vec::new();

    fn handle_app_drop(path: PathBuf, apps: &mut Vec<ManagedApp>, webview: &WebView) {
        let path_display = path.display().to_string();
        println!("[Add] 处理拖入的路径：{}", path_display);

        if !is_app_bundle(&path) {
            println!("[Add] 路径不是 .app 包，忽略：{}", path_display);
            return;
        }

        if apps.iter().any(|app| app.path == path) {
            println!("[Add] 已存在记录，忽略重复：{}", path_display);
            return;
        }

        match hide_dock_icon(&path) {
            Ok(_) => {
                println!("[Add] 成功隐藏 Dock 图标：{}", path_display);
                apps.push(ManagedApp { path });
                let _ = webview.evaluate_script(&js_add_app(&path_display));
            }
            Err(err) => {
                println!("[Add] 隐藏 Dock 图标失败：{}，错误：{}", path_display, err);
            }
        }
    }

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                println!("[Window] 接收到关闭请求，准备退出。");
                *control_flow = ControlFlow::Exit;
            }
            Event::WindowEvent {
                event: WindowEvent::DroppedFile(path),
                ..
            } => {
                println!("[Window] 收到窗口层面的拖入文件：{}", path.display());
                handle_app_drop(path, &mut apps, &webview);
            }
            Event::UserEvent(UserEvent::Add(path)) => {
                println!("[Event] 处理 Add 事件：{}", path.display());
                handle_app_drop(path, &mut apps, &webview);
            }
            Event::UserEvent(UserEvent::Restore(path)) => {
                let display = path.display().to_string();
                println!("[Event] 收到 Restore 事件：{}", display);
                match restore_dock_icon(&path) {
                    Ok(_) => {
                        println!("[Restore] 已恢复 Dock 图标：{}", display);
                        apps.retain(|a| a.path != path);
                        rebuild_list(&webview, &apps);
                    }
                    Err(err) => {
                        println!("[Restore] 恢复 Dock 图标失败：{}，错误：{}", display, err);
                    }
                }
            }
            _ => {}
        }
    });
}
