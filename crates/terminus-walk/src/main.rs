fn main() {
    let mut out = std::io::stdout().lock();
    if let Err(e) = terminus_walk::run_cli(std::env::args(), &mut out) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
