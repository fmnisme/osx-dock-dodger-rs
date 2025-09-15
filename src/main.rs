use std::path::{Path, PathBuf};
use serde::Deserialize;
use plist::Value;
use wry::http::Request;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::WindowBuilder;
use wry::{WebView, WebViewBuilder};

#[derive(Debug, Clone)]
struct ManagedApp {
    path: PathBuf,
}

#[derive(Debug)]
enum UserEvent {
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
    let window = WindowBuilder::new().with_title("Dock Dodger").build(&event_loop).unwrap();

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

    let webview = WebViewBuilder::new(&window)
        .with_html(html)
        .with_ipc_handler(move |req: Request<String>| {
            if let Ok(data) = serde_json::from_str::<IpcRequest>(req.body()) {
                if data.cmd == "restore" {
                    let _ = proxy.send_event(UserEvent::Restore(PathBuf::from(data.path)));
                }
            }
        })
        .build()
        .unwrap();

    let mut apps: Vec<ManagedApp> = Vec::new();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                *control_flow = ControlFlow::Exit
            }
            Event::WindowEvent { event: WindowEvent::DroppedFile(path), .. } => {
                if path.extension().map(|e| e == "app").unwrap_or(false) {
                    if hide_dock_icon(&path).is_ok() {
                        apps.push(ManagedApp { path: path.clone() });
                        let _ = webview.evaluate_script(&js_add_app(path.to_string_lossy().as_ref()));
                    }
                }
            }
            Event::UserEvent(UserEvent::Restore(path)) => {
                if restore_dock_icon(&path).is_ok() {
                    apps.retain(|a| a.path != path);
                    rebuild_list(&webview, &apps);
                }
            }
            _ => {}
        }
    });
}
