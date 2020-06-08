use actix_web::{App, HttpServer};
use clap::Arg;
use simplelog::{LevelFilter, SimpleLogger, TermLogger};

mod search;

struct AppState {
    cs: graphannis::CorpusStorage,
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    let matches = clap::App::new("graphANNIS web service")
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about("Web service line interface to graphANNIS.")
        .arg(
            Arg::with_name("debug")
                .short("d")
                .long("debug")
                .help("Enables debug output")
                .takes_value(false),
        )
        .get_matches();

    let log_filter = if matches.is_present("debug") {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    };

    let log_config = simplelog::ConfigBuilder::new()
        .add_filter_ignore_str("rustyline:")
        .build();

    if let Err(e) = TermLogger::init(
        log_filter,
        log_config.clone(),
        simplelog::TerminalMode::Mixed,
    ) {
        println!("Error, can't initialize the terminal log output: {}.\nWill degrade to a more simple logger", e);
        if let Err(e_simple) = SimpleLogger::init(log_filter, log_config) {
            println!("Simple logging failed too: {}", e_simple);
        }
    }

    // Create a graphANNIS corpus storage as shared state
    let data_dir = std::path::PathBuf::from("data/");
    let cs = graphannis::CorpusStorage::with_auto_cache_size(&data_dir, true).unwrap();
    let app_state = actix_web::web::Data::new(AppState { cs });

    // Run server
    HttpServer::new(move || App::new().app_data(app_state.clone()).service(search::count))
        .bind("127.0.0.1:5711")?
        .run()
        .await
}
