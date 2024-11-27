use ::http_server::start_server;
use ::http_server::SERVER_ADDRESS;
use reqwest::blocking::Client;
use std::thread;

static SERVER_STARTED: OnceCell<()> = OnceCell::new();

use once_cell::sync::OnceCell;
fn start_test_server() {
    let _ = SERVER_STARTED.get_or_init(|| {
        thread::spawn(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                start_server(None);
            });
        });
    });
}

#[test]
fn test_root() {
    start_test_server();
    let response = reqwest::blocking::get(&format!("http://{}/", SERVER_ADDRESS))
        .expect("Failed to send request");
    assert!(response.status().is_success());
    assert_eq!(response.text().unwrap(), "Hello, World!");
}

#[test]
fn test_echo() {
    start_test_server();
    let client = Client::new();
    let response = client
        .get(&format!("http://{}/echo/someText", SERVER_ADDRESS))
        .send()
        .expect("Failed to send request");

    assert!(response.status().is_success());
    assert_eq!(response.text().unwrap(), "someText");
}

#[test]
fn test_user_agent() {
    start_test_server();
    let client = Client::new();
    let response = client
        .get(&format!("http://{}/user-agent", SERVER_ADDRESS))
        .header("User-Agent", "test-agent")
        .send()
        .expect("Failed to send request");

    assert!(response.status().is_success());
    assert_eq!(response.text().unwrap(), "test-agent");
}

#[test]
fn test_no_user_agent() {
    start_test_server();
    let client = Client::new();
    let response = client
        .get(&format!("http://{}/user-agent", SERVER_ADDRESS))
        .send()
        .expect("Failed to send request");

    assert!(response.status().is_client_error());
    assert_eq!(response.text().unwrap(), "Bad Request");
}

#[test]
fn test_files() {
    start_test_server();
    let client = Client::new();
    let response = client
        .get(&format!("http://{}/files/test.txt", SERVER_ADDRESS))
        .send()
        .expect("Failed to send request");

    assert!(response.status().is_success());
    assert_eq!(response.text().unwrap(), "test file");
}

#[test]
fn test_post_files() {
    start_test_server();
    let client = Client::new();
    let response = client
        .post(&format!("http://{}/files/test.txt", SERVER_ADDRESS))
        .body("test file")
        .send()
        .expect("Failed to send request");

    assert!(response.status().is_success());
    assert_eq!(response.text().unwrap(), "Created");
}
