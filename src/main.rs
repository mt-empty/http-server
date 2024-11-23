use std::net::{TcpListener, TcpStream};
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
struct Header<'a> {
    key: SupprotedHeader,
    value: &'a str,
}

impl<'a> Header<'a> {
    fn new(key: SupprotedHeader, value: &'a str) -> Header<'a> {
        Header { key, value }
    }
    fn parse_headers(headers: Vec<(&'a str, &'a str)>) -> Vec<Header<'a>> {
        headers
            .iter()
            .filter_map(|(key, value)| {
                SupprotedHeader::from_str(key)
                    .ok()
                    .map(|key| Header { key, value })
            })
            .collect::<Vec<Header<'a>>>()
    }
}
struct HttpRequest<'a> {
    method: HttpMethod,
    path: &'a str,
    _version: HttpVersion,
    headers: Vec<Header<'a>>,
    body: String,
}

impl<'a> HttpRequest<'a> {
    fn get_header_value(&self, key: SupprotedHeader) -> Option<&str> {
        self.headers
            .iter()
            .find(|header| header.key == key)
            .map(|header| header.value)
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

struct HttpResponse<'a> {
    status_code: StatusCode,
    headers: Vec<Header<'a>>,
    body: String,
    encoding_type: Option<SupportedEncoding>,
}

impl<'a> HttpResponse<'a> {
    fn new(
        status_code: StatusCode,
        headers: Vec<Header<'a>>,
        body: String,
        encoding_type: Option<SupportedEncoding>,
    ) -> HttpResponse<'a> {
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

    fn create_content_length_header<T: AsRef<[u8]>>(body: T) -> Header<'a> {
        let content_length = body.as_ref().len().to_string();
        Header::new(
            SupprotedHeader::ContentLength,
            Box::leak(content_length.into_boxed_str()),
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
            let content_type_string = ContentType::TextPlain.to_string();
            headers.push(Header::new(
                SupprotedHeader::ContentType,
                Box::leak(content_type_string.into_boxed_str()),
            ));
        }

        // Gzipped data is compressed and may contain byte sequences that are not valid UTF-8 characters.
        // Therefore, interpreting it as a UTF-8 string with String::from_utf8(compressed_bytes).unwrap() will not work.
        let body = if let Some(encoding_type) = self.encoding_type {
            let encoding_str = SupportedEncoding::Gzip.to_string();
            headers.push(Header::new(
                SupprotedHeader::ContentEncoding,
                Box::leak(encoding_str.into_boxed_str()),
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

impl Display for HttpResponse<'_> {
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

    let HttpRequest {
        method,
        path,
        headers: request_headers,
        body: request_body,
        ..
    } = match HttpRequest::new(&http_request) {
        Ok(request) => request,
        Err(e) => {
            let http_response =
                HttpResponse::new(StatusCode::BadRequest, vec![], e.to_string(), None);
            stream.write_all(&http_response.body_to_bytes()).unwrap();
            return;
        }
    };

    let mut response_headers = vec![];

    let mut content_encoding: Option<SupportedEncoding> = None;

    // if headers container Accept-Encoding
    let supported_content_encoding =
        SupportedEncoding::retrieve_supported_encodings(&request_headers);

    if supported_content_encoding.contains(&SupportedEncoding::Gzip) {
        content_encoding = Some(SupportedEncoding::Gzip);
    }

    match (method, path) {
        (HttpMethod::Get, "/") => {
            let http_response = HttpResponse::new(
                StatusCode::Ok,
                response_headers,
                "Hello, World!".to_string(),
                content_encoding,
            );
            println!("Response: {}", http_response);
            stream.write_all(&http_response.body_to_bytes()).unwrap();
        }
        (HttpMethod::Get, path) if path.starts_with("/echo/") => {
            let body = &path[6..];

            let http_response = HttpResponse::new(
                StatusCode::Ok,
                response_headers,
                body.to_string(),
                content_encoding,
            );

            println!("Response: {}", http_response);
            stream.write_all(&http_response.body_to_bytes()).unwrap();
        }
        (HttpMethod::Get, "/user-agent") => {
            let user_agent_value = request_headers
                .iter()
                .find(|header| header.key == SupprotedHeader::UserAgent)
                .map(|header| header.value.trim());

            let http_response = match user_agent_value {
                Some(user_agent) => HttpResponse::new(
                    StatusCode::Ok,
                    response_headers,
                    user_agent.to_string(),
                    content_encoding,
                ),
                None => HttpResponse::new(
                    StatusCode::BadRequest,
                    response_headers,
                    "Bad Request".to_string(),
                    content_encoding,
                ),
            };

            println!("Response: {}", http_response);
            stream.write_all(&http_response.body_to_bytes()).unwrap();
        }
        (HttpMethod::Get, path) if path.starts_with("/files/") => {
            let file_path = format!("{}{}", directory, &path[7..]);

            let file_content = match std::fs::read_to_string(file_path) {
                Ok(content) => content,
                Err(_) => {
                    let http_response = HttpResponse::new(
                        StatusCode::NotFound,
                        response_headers,
                        StatusCode::NotFound.to_string(),
                        content_encoding,
                    );
                    stream.write_all(&http_response.body_to_bytes()).unwrap();
                    return;
                }
            };
            let content_type_string = ContentType::ApplicationOctetStream.to_string();
            let content_type = content_type_string.as_str();
            response_headers.push(Header::new(SupprotedHeader::ContentType, content_type));

            let http_response = HttpResponse::new(
                StatusCode::Ok,
                response_headers,
                file_content,
                content_encoding,
            );

            println!("Response: {}", http_response);
            stream.write_all(&http_response.body_to_bytes()).unwrap();
        }
        (HttpMethod::Post, path) if path.starts_with("/files/") => {
            let file_path = format!("{}{}", directory, &path[7..]);

            let content_length = request_headers
                .iter()
                .find(|header| header.key == SupprotedHeader::ContentLength)
                .map(|header| header.value.trim())
                .unwrap();

            let content_length: usize = content_length.parse().unwrap();
            let body = &request_body[..content_length];

            let http_response: HttpResponse = match std::fs::write(file_path, body) {
                Ok(_) => HttpResponse::new(
                    StatusCode::Created,
                    response_headers,
                    StatusCode::Created.to_string(),
                    content_encoding,
                ),
                Err(_) => HttpResponse::new(
                    StatusCode::InternalServerError,
                    response_headers,
                    "Internal Server Error".to_string(),
                    content_encoding,
                ),
            };
            println!("Response: {}", http_response);
            stream.write_all(&http_response.body_to_bytes()).unwrap();
        }
        _ => {
            let http_response = HttpResponse::new(
                StatusCode::NotFound,
                request_headers,
                StatusCode::NotFound.to_string(),
                content_encoding,
            );
            println!("Response: {}", http_response);
            stream.write_all(&http_response.body_to_bytes()).unwrap();
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
