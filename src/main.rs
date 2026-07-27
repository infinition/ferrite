//! Ferrite: inventories and cleans the regenerable artifacts of a workspace.
//!
//! Desktop application: a native window renders the interface through WebView2,
//! backed by a local HTTP server. Everything ships inside a single executable,
//! there is nothing to install.

// No console behind the window in release. Kept in debug to read traces.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod catalog;
mod i18n;
mod report;
mod scanner;
mod server;

use std::sync::Arc;

use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::{Icon, WindowBuilder};
use wry::WebViewBuilder;

const WINDOW_ICON: &[u8] = include_bytes!("../assets/icon-64.rgba");
const ICON_EDGE: u32 = 64;
const DEFAULT_PORT: u16 = 7420;

fn main() {
    let options = Options::from_args();

    let server = match bind(options.port) {
        Some(server) => server,
        None => {
            fatal("no port available for the local interface");
            return;
        }
    };

    let port = server
        .server_addr()
        .to_ip()
        .map(|addr| addr.port())
        .unwrap_or(options.port);
    let url = format!("http://127.0.0.1:{port}");

    let state = Arc::new(server::AppState::new());
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            let state = state.clone();
            // One thread per request: cleaning can run for minutes and must
            // not freeze progress reporting.
            std::thread::spawn(move || server::handle_request(state, request));
        }
    });

    if options.headless {
        println!();
        println!("  Ferrite {}", env!("CARGO_PKG_VERSION"));
        println!("  {} detection rules loaded", catalog::RULES.len());
        println!("  Interface: {url}");
        println!("  Ctrl+C to stop");
        println!();
        loop {
            std::thread::park();
        }
    }

    run_window(&url);
}

/// Opens the local server on a stable port.
///
/// The interface stores its preferences in browser local storage, which is
/// keyed by origin: a different port on every launch would wipe the language,
/// the last workspace and the options. So the default port is kept while it is
/// free, and a neighbouring port is only used when another instance already
/// holds it.
fn bind(requested: u16) -> Option<tiny_http::Server> {
    if requested != 0 {
        return tiny_http::Server::http(("127.0.0.1", requested)).ok();
    }

    for port in DEFAULT_PORT..DEFAULT_PORT + 10 {
        if let Ok(server) = tiny_http::Server::http(("127.0.0.1", port)) {
            return Some(server);
        }
    }
    tiny_http::Server::http(("127.0.0.1", 0u16)).ok()
}

fn run_window(url: &str) {
    let event_loop = EventLoop::new();

    let window = WindowBuilder::new()
        .with_title("Ferrite")
        .with_inner_size(LogicalSize::new(1280.0, 860.0))
        .with_min_inner_size(LogicalSize::new(560.0, 480.0))
        .with_window_icon(window_icon())
        .build(&event_loop);

    let window = match window {
        Ok(window) => window,
        Err(error) => {
            fatal(&format!("could not create the window ({error})"));
            return;
        }
    };

    let webview = WebViewBuilder::new()
        .with_url(url)
        // Same background as the stylesheet, so there is no white flash while
        // the page loads.
        .with_background_color((23, 25, 29, 255))
        .build(&window);

    if let Err(error) = webview {
        fatal(&format!("WebView2 is unavailable ({error})"));
        return;
    }

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            *control_flow = ControlFlow::Exit;
        }
    });
}

fn window_icon() -> Option<Icon> {
    Icon::from_rgba(WINDOW_ICON.to_vec(), ICON_EDGE, ICON_EDGE).ok()
}

struct Options {
    port: u16,
    headless: bool,
}

impl Options {
    fn from_args() -> Self {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let mut options = Options {
            port: 0,
            headless: false,
        };

        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--port" | "-p" => {
                    if let Some(value) = args.get(index + 1).and_then(|v| v.parse().ok()) {
                        options.port = value;
                    }
                    index += 2;
                }
                "--headless" => {
                    options.headless = true;
                    index += 1;
                }
                "--help" | "-h" => {
                    println!("Ferrite {}", env!("CARGO_PKG_VERSION"));
                    println!("  --port <n>   listening port (default {DEFAULT_PORT})");
                    println!("  --headless   no window, interface served to the browser");
                    std::process::exit(0);
                }
                _ => index += 1,
            }
        }
        options
    }
}

/// Reports a fatal error. With no console attached, a dialog box is the only
/// channel the user can actually see.
fn fatal(message: &str) {
    eprintln!("  Ferrite: {message}");

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let script = format!(
            "Add-Type -AssemblyName PresentationFramework; \
             [System.Windows.MessageBox]::Show('{}', 'Ferrite')",
            message.replace('\'', "''")
        );
        let _ = std::process::Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
            .creation_flags(0x0800_0000)
            .status();
    }
}
