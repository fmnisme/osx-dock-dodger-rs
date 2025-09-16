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
    <html>
      <head><meta charset='utf-8'><title>Dock Dodger</title></head>
      <body>
        <h1>Dock Dodger</h1>
        <p>将 .app 拖入窗口以隐藏 Dock 图标。</p>
        <ul id='list'></ul>
        <script>
          function addApp(path){
            const li=document.createElement('li');
            li.textContent=path;
            const btn=document.createElement('button');
            btn.textContent='恢复';
            btn.onclick=()=>window.ipc.postMessage(JSON.stringify({cmd:'restore', path}));
            li.appendChild(btn);
            document.getElementById('list').appendChild(li);
          }
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
