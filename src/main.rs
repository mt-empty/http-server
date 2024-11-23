#[allow(unused_imports)]
use std::net::{TcpListener, TcpStream};
use std::{
    io::{Read, Write},
    thread, vec,
};

use flate2::write::GzEncoder;
use flate2::Compression;

const CR_LF: &str = "\r\n";
const MAX_BUFFER_SIZE: usize = 1024;

// generate enums for the status codes
#[derive(Debug)]
enum StatusCode {
    Ok = 200,
    BadRequest = 400,
}

enum SupportedEncoding {
    Gzip,
}

/*
* assume that the request is in the following format only:
- status line
- headers
- body
*/
fn seperate_request_parts(https_request: &str) -> (&str, &str, &str) {
    let (status_line, rest) = https_request.split_once(CR_LF).unwrap();

    let (header_line, body_line) = rest.rsplit_once(CR_LF).unwrap();

    (status_line, header_line, body_line)
}

fn separate_request_headers(header_line: &str) -> Vec<(&str, &str)> {
    let headers = header_line.split(CR_LF).filter(|header| !header.is_empty());

    headers
        .into_iter()
        .map(|header| {
            let (key, value) = header.split_once(":").unwrap();
            (key, value)
        })
        .collect()
}

fn create_header_for_user_agent<T: AsRef<[u8]>>(
    body: T,
    content_type: &str,
    extra_headers: Vec<(&str, &str)>,
) -> String {
    let content_length = format!("Content-Length: {}", body.as_ref().len());
    let mut headers = vec![content_type.to_string(), content_length];

    for (key, value) in extra_headers {
        headers.push(format!("{}: {}", key, value));
    }

    headers.join(CR_LF) + CR_LF
}

fn compress_string(body: &str) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(body.as_bytes()).unwrap();
    encoder.finish().unwrap()

    // As gzipped data is compressed, trying to interpret it using text encoding can result in errors. This is because the compressed data may include byte sequences that do not correspond to valid characters in any encoding system. Keep this in mind when working with Strings.

    // String::from_utf8(compressed_bytes).unwrap() // will not work
}

fn handle_client(mut stream: TcpStream, directory: String) {
    let mut buffer = [0; MAX_BUFFER_SIZE];
    let bytes_read = stream.read(&mut buffer).unwrap();

    let https_request = String::from_utf8_lossy(&buffer[..bytes_read]);
    println!("Received request: {}", https_request);

    let (status_request_line, header_request_line, body_request_line) =
        seperate_request_parts(&https_request);

    let status_parts: Vec<&str> = status_request_line.split_whitespace().collect();

    if status_parts.len() != 3 {
        let response = format!("HTTP/1.1 400 Bad Request{}{}", CR_LF, CR_LF);
        stream.write_all(response.as_bytes()).unwrap();
        return;
    }

    let method = status_parts[0];
    let path = status_parts[1];
    let _version = status_parts[2];
    let status_response_line = "HTTP/1.1 200 OK";
    let mut extra_headers = vec![];
    let headers = separate_request_headers(header_request_line);

    let mut content_encoding: Option<SupportedEncoding> = None;

    // if headers container Accept-Encoding
    if let Some(encoding) = headers.iter().find(|(key, _)| key == &"Accept-Encoding") {
        let encoding_values = encoding
            .1
            .trim()
            .split(",")
            .map(|value| value.trim())
            .collect::<Vec<&str>>();
        println!("Encoding values: {:?}", encoding_values);
        if encoding_values.contains(&"gzip") {
            content_encoding = Some(SupportedEncoding::Gzip);
            extra_headers.push(("Content-Encoding", "gzip"));
        }
    }

    match (method, path) {
        ("GET", "/") => {
            stream
                .write_all(format!("{}{}{}", status_response_line, CR_LF, CR_LF).as_bytes())
                .unwrap();
        }
        ("GET", path) if path.starts_with("/echo/") => {
            let body_response_line = &path[6..];
            let content_type = "Content-Type: text/plain";
            let response_body;
            let header_response_line;

            if content_encoding.is_some() {
                let body = compress_string(body_response_line);
                header_response_line =
                    create_header_for_user_agent(&body, content_type, extra_headers);
                response_body = body;
            } else {
                header_response_line =
                    create_header_for_user_agent(body_response_line, content_type, extra_headers);
                response_body = body_response_line.as_bytes().to_vec();
            }

            let response = format!(
                "{}{}{}{}",
                status_response_line, CR_LF, header_response_line, CR_LF
            );
            println!("Response: {}", response);
            stream.write_all(response.as_bytes()).unwrap();
            stream.write_all(&response_body).unwrap();
        }
        ("GET", "/user-agent") => {
            let user_agent_value = headers
                .iter()
                .find(|(key, _)| key == &"User-Agent")
                .map(|(_, value)| value.trim());
            let content_type = "Content-Type: text/plain";

            let response = if let Some(user_agent) = user_agent_value {
                format!(
                    "{}{}{}{}{}",
                    status_response_line,
                    CR_LF,
                    create_header_for_user_agent(user_agent, content_type, extra_headers),
                    CR_LF,
                    user_agent
                )
            } else {
                format!("HTTP/1.1 400 Bad Request{}{}", CR_LF, CR_LF)
            };

            println!("Response: {}", response);
            stream.write_all(response.as_bytes()).unwrap();
        }
        ("GET", path) if path.starts_with("/files/") => {
            let file_path = format!("{}{}", directory, &path[7..]);
            println!("File path: {}", file_path);
            let file_content = match std::fs::read_to_string(file_path) {
                Ok(content) => content,
                Err(_) => {
                    let response: String = format!("HTTP/1.1 404 Not Found{}{}", CR_LF, CR_LF);
                    stream.write_all(response.as_bytes()).unwrap();
                    return;
                }
            };

            let content_type = "Content-Type: application/octet-stream";
            let header_response_line =
                create_header_for_user_agent(&file_content, content_type, extra_headers);

            let response = format!(
                "{}{}{}{}{}",
                status_response_line, CR_LF, header_response_line, CR_LF, file_content
            );
            println!("Response: {}", response);
            stream.write_all(response.as_bytes()).unwrap();
        }
        ("POST", path) if path.starts_with("/files/") => {
            let file_path = format!("{}{}", directory, &path[7..]);
            let headers = separate_request_headers(header_request_line);

            let content_length = headers
                .iter()
                .find(|(key, _)| key == &"Content-Length")
                .map(|(_, value)| value.trim())
                .unwrap();

            let content_length: usize = content_length.parse().unwrap();
            let body = &body_request_line[..content_length];

            println!("File content: {}", body);

            match std::fs::write(file_path, body) {
                Ok(_) => {
                    let response = format!("HTTP/1.1 201 Created{}{}", CR_LF, CR_LF);
                    stream.write_all(response.as_bytes()).unwrap();
                }
                Err(_) => {
                    let response = format!("HTTP/1.1 500 Internal Server Error{}{}", CR_LF, CR_LF);
                    stream.write_all(response.as_bytes()).unwrap();
                }
            }
        }
        _ => {
            let response = format!("HTTP/1.1 404 Not Found{}{}", CR_LF, CR_LF);
            stream.write_all(response.as_bytes()).unwrap();
        }
    }
    stream.flush().unwrap()
}

fn main() {
    let mut directory = String::from("/tmp/");
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 {
        directory = args[2].clone();
    }

    let listener = TcpListener::bind("127.0.0.1:4221").unwrap();

    for stream in listener.incoming() {
        match stream {
            Ok(mut _stream) => {
                println!("accepted new connection");
                let directory = directory.clone();
                thread::spawn(move || {
                    handle_client(_stream, directory);
                });
            }
            Err(e) => {
                println!("error: {}", e);
            }
        }
    }
}
