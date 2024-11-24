use std::net::{TcpListener, TcpStream};
use std::rc::Rc;
use std::{
    fmt::Display,
    io::{Read, Write},
    str::FromStr,
    thread, vec,
};

use anyhow::Result;
use flate2::write::DeflateEncoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use strum_macros::{Display, EnumString};

const CR_LF: &str = "\r\n";
const MAX_BUFFER_SIZE: usize = 1024;
const SERVER_ADDRESS: &str = "127.0.0.1:4221";

#[derive(EnumString, Display, Debug, Clone, Copy, PartialEq, Eq)]
enum StatusCode {
    #[strum(serialize = "OK")]
    Ok = 200,
    #[strum(serialize = "Created")]
    Created = 201,
    #[strum(serialize = "Bad Request")]
    BadRequest = 400,
    #[strum(serialize = "Not Found")]
    NotFound = 404,
    #[strum(serialize = "Internal Server Error")]
    InternalServerError = 500,
}

#[derive(EnumString, Display, Debug, Clone, Copy, PartialEq, Eq)]
enum SupprotedHeader {
    #[strum(serialize = "Content-Type")]
    ContentType,
    #[strum(serialize = "Content-Length")]
    ContentLength,
    #[strum(serialize = "Content-Encoding")]
    ContentEncoding,
    #[strum(serialize = "User-Agent")]
    UserAgent,
    #[strum(serialize = "Accept-Encoding")]
    AcceptEncoding,
}

#[derive(EnumString, Display, Debug, Clone, Copy, PartialEq, Eq)]
enum ContentType {
    #[strum(serialize = "text/plain")]
    TextPlain,
    #[strum(serialize = "application/json")]
    ApplicationJson,
    #[strum(serialize = "application/octet-stream")]
    ApplicationOctetStream,
    #[strum(serialize = "text/html")]
    TextHtml,
}

#[derive(EnumString, Display, Debug, Clone, Copy, PartialEq, Eq)]
enum SupportedEncoding {
    #[strum(serialize = "gzip")]
    Gzip,
    #[strum(serialize = "deflate")]
    Deflate,
}

impl SupportedEncoding {
    fn retrieve_supported_encodings(headers: &[Header]) -> Vec<SupportedEncoding> {
        if let Some(encoding_header) = headers
            .iter()
            .find(|h| h.key == SupprotedHeader::AcceptEncoding)
        {
            let encoding_values = encoding_header.value.trim();
            encoding_values
                .split(',')
                .filter_map(|value| SupportedEncoding::from_str(value.trim()).ok())
                .collect::<Vec<SupportedEncoding>>()
        } else {
            vec![]
        }
    }
}

#[derive(EnumString, Display, Debug, Clone, Copy, PartialEq, Eq)]
enum HttpMethod {
    #[strum(serialize = "GET")]
    Get,
    #[strum(serialize = "POST")]
    Post,
    #[strum(serialize = "PUT")]
    Put,
    #[strum(serialize = "DELETE")]
    Delete,
    #[strum(serialize = "PATCH")]
    Patch,
    #[strum(serialize = "HEAD")]
    Head,
    #[strum(serialize = "OPTIONS")]
    Options,
    #[strum(serialize = "CONNECT")]
    Connect,
    #[strum(serialize = "TRACE")]
    Trace,
}

#[derive(EnumString, Display, Debug, Clone, Copy, PartialEq, Eq)]
enum HttpVersion {
    #[strum(serialize = "HTTP/1.0")]
    Http1_0,
    #[strum(serialize = "HTTP/1.1")]
    Http1_1,
    #[strum(serialize = "HTTP/2.0")]
    Http2_0,
}

#[derive(Clone)]
struct Header {
    key: SupprotedHeader,
    value: Rc<String>,
}

impl Header {
    fn new(key: SupprotedHeader, value: Rc<String>) -> Header {
        Header { key, value }
    }
    fn parse_headers(headers: Vec<(&str, &str)>) -> Vec<Header> {
        headers
            .iter()
            .filter_map(|(key, value)| {
                SupprotedHeader::from_str(key).ok().map(|key| Header {
                    key,
                    value: Rc::new(value.to_string()),
                })
            })
            .collect::<Vec<Header>>()
    }
}
struct HttpRequest<'a> {
    method: HttpMethod,
    path: &'a str,
    _version: HttpVersion,
    headers: Vec<Header>,
    body: String,
}

impl<'a> HttpRequest<'a> {
    fn get_header_value(&self, key: SupprotedHeader) -> Option<&str> {
        self.headers
            .iter()
            .find(|header| header.key == key)
            .map(|header| header.value.as_str())
    }

    fn new(http_request: &'a str) -> Result<HttpRequest> {
        let (status_line, header_line, body_line) = extract_request_components(http_request);

        let status_parts: Vec<&str> = status_line.split_whitespace().collect();

        if status_parts.len() != 3 {
            return Err(anyhow::anyhow!("Invalid status line"));
        }

        let method = HttpMethod::from_str(status_parts[0])?;

        let path = status_parts[1];
        let version = HttpVersion::from_str(status_parts[2])?;

        let headers = Header::parse_headers(extract_request_headers(header_line));

        Ok(HttpRequest {
            method,
            path,
            _version: version,
            headers,
            body: body_line.to_string(),
        })
    }
}

struct HttpResponse {
    status_code: StatusCode,
    headers: Vec<Header>,
    body: String,
    encoding_type: Option<SupportedEncoding>,
}

impl HttpResponse {
    fn new(
        status_code: StatusCode,
        headers: Vec<Header>,
        body: String,
        encoding_type: Option<SupportedEncoding>,
    ) -> HttpResponse {
        HttpResponse {
            status_code,
            headers,
            body,
            encoding_type,
        }
    }
    fn compress_string(self, encoding_type: SupportedEncoding) -> Vec<u8> {
        match encoding_type {
            SupportedEncoding::Gzip => {
                let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
                encoder.write_all(self.body.as_bytes()).unwrap();
                encoder.finish().unwrap()
            }
            SupportedEncoding::Deflate => {
                let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
                encoder.write_all(self.body.as_bytes()).unwrap();
                encoder.finish().unwrap()
            }
        }
    }

    fn create_content_length_header<T: AsRef<[u8]>>(body: T) -> Header {
        Header::new(
            SupprotedHeader::ContentLength,
            Rc::new(body.as_ref().len().to_string()),
        )
    }
    fn body_to_bytes(self) -> Vec<u8> {
        let status_line = format!(
            "{} {} {}",
            HttpVersion::Http1_1,
            self.status_code as u16,
            StatusCode::to_string(&self.status_code)
        );

        let mut headers = self.headers.clone();

        if !self
            .headers
            .iter()
            .any(|header| header.key == SupprotedHeader::ContentType)
        {
            headers.push(Header::new(
                SupprotedHeader::ContentType,
                Rc::new(ContentType::TextPlain.to_string()),
            ));
        }

        // Gzipped data is compressed and may contain byte sequences that are not valid UTF-8 characters.
        // Therefore, interpreting it as a UTF-8 string with String::from_utf8(compressed_bytes).unwrap() will not work.
        let body = if let Some(encoding_type) = self.encoding_type {
            headers.push(Header::new(
                SupprotedHeader::ContentEncoding,
                Rc::new(SupportedEncoding::Gzip.to_string()),
            ));
            self.compress_string(encoding_type)
        } else {
            self.body.clone().into_bytes()
        };
        headers.push(HttpResponse::create_content_length_header(&body));

        let formatted_headers = headers
            .iter()
            .map(|header| format!("{}: {}", header.key, header.value))
            .collect::<Vec<String>>()
            .join(CR_LF)
            + CR_LF;

        let response = format!("{}{}{}{}", status_line, CR_LF, formatted_headers, CR_LF);
        let mut response_bytes = response.into_bytes();
        response_bytes.extend_from_slice(&body);
        response_bytes
    }
}

impl Display for HttpResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status_line = format!(
            "{} {} {}",
            HttpVersion::Http1_1,
            self.status_code as u16,
            StatusCode::to_string(&self.status_code)
        );

        let headers = self
            .headers
            .iter()
            .map(|header| format!("{}: {}", header.key, header.value))
            .collect::<Vec<String>>()
            .join(CR_LF);

        write!(f, "{}{}{}{}", status_line, CR_LF, headers, CR_LF)
    }
}

/*
* assume that the request is in the following format only:
- status line
- headers
- body
*/
fn extract_request_components(https_request: &str) -> (&str, &str, &str) {
    let (status_line, rest) = https_request.split_once(CR_LF).unwrap();
    let (header_line, body_line) = rest.rsplit_once(CR_LF).unwrap();

    (status_line, header_line, body_line)
}

fn extract_request_headers(header_line: &str) -> Vec<(&str, &str)> {
    let headers = header_line.split(CR_LF).filter(|header| !header.is_empty());

    headers
        .into_iter()
        .map(|header| {
            let (key, value) = header.split_once(":").unwrap();
            (key, value)
        })
        .collect()
}

fn handle_client(mut stream: TcpStream, directory: String) {
    let mut buffer = [0; MAX_BUFFER_SIZE];
    let bytes_read = stream.read(&mut buffer).unwrap();

    let http_request = String::from_utf8_lossy(&buffer[..bytes_read]);
    println!("Received request: {}", http_request);

    let request = match HttpRequest::new(&http_request) {
        Ok(request) => request,
        Err(e) => {
            let response = HttpResponse::new(StatusCode::BadRequest, vec![], e.to_string(), None);
            stream.write_all(&response.body_to_bytes()).unwrap();
            return;
        }
    };

    let mut response_headers = vec![];
    let content_encoding = SupportedEncoding::retrieve_supported_encodings(&request.headers)
        .into_iter()
        .find(|&encoding| encoding == SupportedEncoding::Gzip);

    let response = match (request.method, request.path) {
        (HttpMethod::Get, "/") => HttpResponse::new(
            StatusCode::Ok,
            response_headers,
            "Hello, World!".to_string(),
            content_encoding,
        ),
        (HttpMethod::Get, path) if path.starts_with("/echo/") => HttpResponse::new(
            StatusCode::Ok,
            response_headers,
            path[6..].to_string(),
            content_encoding,
        ),
        (HttpMethod::Get, "/user-agent") => {
            let user_agent = request
                .get_header_value(SupprotedHeader::UserAgent)
                .unwrap_or(&StatusCode::BadRequest.to_string())
                .trim()
                .to_string();
            let status = if user_agent == StatusCode::BadRequest.to_string() {
                StatusCode::BadRequest
            } else {
                StatusCode::Ok
            };
            HttpResponse::new(status, response_headers, user_agent, content_encoding)
        }
        (HttpMethod::Get, path) if path.starts_with("/files/") => {
            let file_path = format!("{}{}", directory, &path[7..]);
            match std::fs::read_to_string(&file_path) {
                Ok(content) => {
                    response_headers.push(Header::new(
                        SupprotedHeader::ContentType,
                        Rc::new(ContentType::ApplicationOctetStream.to_string()),
                    ));
                    HttpResponse::new(StatusCode::Ok, response_headers, content, content_encoding)
                }
                Err(_) => HttpResponse::new(
                    StatusCode::NotFound,
                    response_headers,
                    StatusCode::NotFound.to_string(),
                    content_encoding,
                ),
            }
        }
        (HttpMethod::Post, path) if path.starts_with("/files/") => {
            let file_path = format!("{}{}", directory, &path[7..]);
            let body = &request.body;
            match std::fs::write(&file_path, body) {
                Ok(_) => HttpResponse::new(
                    StatusCode::Created,
                    response_headers,
                    StatusCode::Created.to_string(),
                    content_encoding,
                ),
                Err(_) => HttpResponse::new(
                    StatusCode::InternalServerError,
                    response_headers,
                    StatusCode::InternalServerError.to_string(),
                    content_encoding,
                ),
            }
        }
        _ => HttpResponse::new(
            StatusCode::NotFound,
            response_headers,
            StatusCode::NotFound.to_string(),
            content_encoding,
        ),
    };

    println!("Response: {}", response);
    stream.write_all(&response.body_to_bytes()).unwrap();
    stream.flush().unwrap();
}

fn main() {
    let mut directory = String::from("/tmp/");
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 {
        directory = args[2].clone();
    }

    let listener = TcpListener::bind(SERVER_ADDRESS).unwrap();

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
