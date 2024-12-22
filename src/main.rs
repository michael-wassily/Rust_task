use embedded_recruitment_task::server::Server;
use log::info;
use std::io;

fn main()->io::Result<()>{
    //initialize logger
    env_logger::Builder::new()
        .parse_filters("info")
        .init();

    //create server
    let server=Server::new("localhost:8080")?;
    info!("server starting on localhost:8080");

    server.run()
}