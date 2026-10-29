#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    match std::env::args().nth(1).as_deref() {
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
