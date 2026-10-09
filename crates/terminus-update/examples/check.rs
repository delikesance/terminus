//! `cargo run -p terminus-update --example check -- <running-version>`
//!
//! Asks the official endpoint (or `TERMINUS_UPDATE_URL`) whether a newer
//! release exists, and whether this build could install it.

fn main() {
    let current = std::env::args()
        .nth(1)
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
    let client = match terminus_update::Client::official() {
        Ok(client) => client,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    };
    match client.check(&current) {
        Ok(Some(release)) => {
            println!(
                "update available: {} ({})",
                release.version, release.page_url
            );
            for asset in &release.assets {
                println!("  {} ({} bytes)", asset.name, asset.size);
            }
        }
        Ok(None) => println!("{current} is up to date"),
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    }
}
