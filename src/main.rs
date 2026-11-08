#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("--host-key") => {
            if let Err(error) = tabius_desktop_lib::host_key_helper() {
                eprintln!("Host-key verification failed: {error}");
                std::process::exit(1);
            }
            return;
        }
        Some("--version" | "-V") => {
            println!("tabius-desktop {}", env!("CARGO_PKG_VERSION"));
            return;
        }
        Some("--help" | "-h") => {
            println!(
                "Tabius — a focused SSH workspace\n\nUsage: tabius-desktop [--version | --help]"
            );
            return;
        }
        _ => {}
    }
    tabius_desktop_lib::run();
}
