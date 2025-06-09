# soop3

A secure, high-performance HTTP file server written in Rust. Features file serving, uploads, authentication, and directory listings with comprehensive security protections.

## Usage

```bash
soop3                           # serve current directory on localhost:8000
soop3 --enable-upload           # allow file uploads
soop3 --host 0.0.0.0 --port 80  # listen on all interfaces
soop3 --config server.toml      # use config file
soop3 /path/to/files            # serve specific directory
```

## Configuration

Create `server.toml`:

```toml
[server]
host = "0.0.0.0"
port = 8000
enable_upload = true
public_dir = "./files"

[security]
username = "admin"
password = "secure_password" 
policy = "authenticate_upload"

[upload]
prepend_timestamp = true
prevent_overwrite = true
max_request_size = 1073741824
create_directories = false

[listing]
ignore_file = ".gitignore"
```

**Security policies:**
- `authenticate_none` - No authentication required
- `authenticate_upload` - Authentication required for uploads only  
- `authenticate_download` - Authentication required for downloads only
- `authenticate_all` - Authentication required for all requests

## Features

- **File Serving**: Static file serving with proper MIME types
- **File Uploads**: Secure multipart file uploads with validation  
- **Authentication**: HTTP Basic Auth with configurable policies
- **Security**: Path traversal protection, security headers, input validation
- **Directory Listings**: Beautiful HTML directory listings with sorting
- **Ignore Files**: Support for `.gitignore`-style file filtering
- **Configuration**: Flexible TOML configuration with CLI overrides

## Build & Development

```bash
cargo build --release                    # optimized production binary
cargo test --features test-helpers       # run test suite
cargo clippy                            # run linter
cargo fmt                               # format code
```

## Testing

```bash
# run all tests
cargo test --features test-helpers

# run unit tests only 
cargo test --lib --features test-helpers

# run specific test
cargo test --features test-helpers test_file_serving
```

**Note**: The `test-helpers` feature is required for integration tests. You may see warnings about unused test helper functions during test compilation - this is expected and doesn't affect production builds.

**68 tests** covering:
- File serving and static assets
- Upload functionality and validation  
- Authentication and authorization
- Security protections and edge cases
- Configuration parsing and validation

## License

MIT