use clap::Parser;

use tracing::info;
use ui_events::run;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// WebSocket server port
    #[clap(short, long, value_parser, default_value_t = 9001)]
    port: u16,
}

fn main() {
    tracing_subscriber::fmt::init();
    info!("starting ui-events...");

    let port = Args::parse().port;

    run(port);
}
