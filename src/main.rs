use http_server::start_server;

fn main() {
    let directory = std::env::args().nth(2);
    start_server(directory);
}
