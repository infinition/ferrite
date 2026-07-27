//! Ferrite: inventaire et nettoyage des artefacts regenerables d'un workspace.
//!
//! Application de bureau: une fenetre native affiche l'interface via WebView2,
//! alimentee par un serveur HTTP local lie a la boucle d'evenements. Tout est
//! embarque dans un seul executable, il n'y a rien a installer.

// En release, pas de console derriere la fenetre. En debug, on la garde pour
// voir les traces.
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
            fatal("aucun port disponible pour l'interface locale");
            return;
        }
    };

    let port = server.server_addr().to_ip().map(|addr| addr.port()).unwrap_or(options.port);
    let url = format!("http://127.0.0.1:{port}");

    let state = Arc::new(server::AppState::new());
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            let state = state.clone();
            // Un thread par requete: le nettoyage peut durer plusieurs minutes
            // et ne doit pas figer le suivi d'avancement.
            std::thread::spawn(move || server::handle_request(state, request));
        }
    });

    if options.headless {
        println!();
        println!("  Ferrite {}", env!("CARGO_PKG_VERSION"));
        println!("  {} regles de detection", catalog::RULES.len());
        println!("  Interface: {url}");
        println!("  Ctrl+C pour arreter");
        println!();
        loop {
            std::thread::park();
        }
    }

    run_window(&url);
}

/// Ouvre le serveur local sur un port stable.
///
/// L'interface memorise ses preferences dans le stockage local du navigateur,
/// qui est indexe par origine: un port different a chaque lancement effacerait
/// la langue, le dernier workspace et les options. On garde donc le port par
/// defaut tant qu'il est libre, et on ne glisse vers un port voisin que si une
/// autre instance occupe deja la place.
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
            fatal(&format!("fenetre impossible a creer ({error})"));
            return;
        }
    };

    let webview = WebViewBuilder::new()
        .with_url(url)
        // Meme fond que la feuille de style: evite un flash blanc a l'ouverture.
        .with_background_color((23, 25, 29, 255))
        .build(&window);

    if let Err(error) = webview {
        fatal(&format!("WebView2 indisponible ({error})"));
        return;
    }

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        if let Event::WindowEvent { event: WindowEvent::CloseRequested, .. } = event {
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
        let mut options = Options { port: 0, headless: false };

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
                    println!("  --port <n>   port d'ecoute (defaut {DEFAULT_PORT})");
                    println!("  --headless   pas de fenetre, interface servie au navigateur");
                    std::process::exit(0);
                }
                _ => index += 1,
            }
        }
        options
    }
}

/// Signale une erreur bloquante. Sans console attachee, une boite de dialogue
/// est le seul canal visible pour l'utilisateur.
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
