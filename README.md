# HTTP Server

This is a simple HTTP server written in Rust. It supports basic HTTP methods such as GET and POST, and can handle gzip compression for responses.

## Endpoints

### 1. `GET /`
- **Description**: Returns a simple "Hello, World!" message.
- **Response**: 
  - Status: `200 OK`
  - Body: `"Hello, World!"`

### 2. `GET /echo/{text}`
- **Description**: Echoes back the `{text}` provided in the URL.
- **Response**: 
  - Status: `200 OK`
  - Body: `{text}`

### 3. `GET /user-agent`
- **Description**: Returns the `User-Agent` header value from the request.
- **Response**: 
  - Status: `200 OK` if `User-Agent` is present, otherwise `400 Bad Request`
  - Body: `User-Agent` value or `400 Bad Request`

### 4. `GET /files/{filename}`
- **Description**: Serves the file specified by `{filename}` from the specified directory.
- **Response**: 
  - Status: `200 OK` if file is found, otherwise `404 Not Found`
  - Body: File content or `404 Not Found`

### 5. `POST /files/{filename}`
- **Description**: Saves the request body as a file specified by `{filename}` in the specified directory.
- **Response**: 
  - Status: `201 Created` if file is successfully created, otherwise `500 Internal Server Error`
  - Body: `201 Created` or `500 Internal Server Error`

## Usage

### Running the Server

To run the server, use the following command:

```sh
cargo run -- [directory]
```

- [directory] is the directory from which files will be served. If not specified, it defaults to /tmp/.

## Dependencies
This project uses the following dependencies:

- `anyhow`: For error handling.
- `flate2`: For gzip compression.
- `strum_macros`: For deriving EnumString and Display for enums.
