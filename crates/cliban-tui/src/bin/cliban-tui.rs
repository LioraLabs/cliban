fn main() -> Result<(), Box<dyn std::error::Error>> {
    match std::env::args().nth(1) {
        Some(p) => cliban_tui::run(p),
        None => cliban_tui::run(cliban_core::paths::db_path()),
    }
}
