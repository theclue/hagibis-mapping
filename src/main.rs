fn main() {
    if let Err(e) = hagibis_hub_mapper::run() {
        eprintln!("Fatal error: {}", e);
        std::process::exit(1);
    }
}
