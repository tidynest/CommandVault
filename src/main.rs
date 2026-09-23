mod app;
mod config;
mod export;
mod history;
mod model;
mod storage;
mod view;

use iced::Size;

use app::{App, MIN_SIZE};

/// Three lines for --help and for an unknown argument.
fn usage() -> String {
    let dir = storage::vault_path()
        .and_then(|p| p.parent().map(|d| d.display().to_string()))
        .unwrap_or_else(|| "no config directory found".into());
    format!(
        "CommandVault {}\nUsage: command_vault [--version | --help]\nData: {dir}",
        env!("CARGO_PKG_VERSION")
    )
}

fn main() -> iced::Result {
    env_logger::init();
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("--version" | "-V") => {
            println!("CommandVault {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("--help" | "-h") => {
            println!("{}", usage());
            return Ok(());
        }
        Some(other) => {
            eprintln!("Unknown argument: {other}\n{}", usage());
            std::process::exit(2);
        }
        None => {}
    }
    // Settings are loaded here rather than in App::new because the window size
    // has to be known before the window opens.
    let settings = storage::vault_path()
        .map(|p| config::load(&config::config_path(&p)))
        .unwrap_or_default();
    let defaults = iced::window::Settings::default();
    // Both sizes are in the compositor's logical pixels, so the zoom scales the
    // minimum: iced divides by the zoom before laying out.
    let min_size = MIN_SIZE * settings.zoom;
    let size = settings.window.map_or(defaults.size, |w| {
        Size::new(w.width.max(min_size.width), w.height.max(min_size.height))
    });
    iced::application(move || App::new(settings.clone()), App::update, App::view)
        .title("CommandVault")
        .subscription(App::subscription)
        .theme(App::theme)
        .scale_factor(App::scale)
        .window(iced::window::Settings {
            size,
            min_size: Some(min_size),
            // The close request reaches update, which writes the size and then closes.
            exit_on_close_request: false,
            ..defaults
        })
        .run()
}
