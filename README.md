# soop3

the based http file server

## usage

```bash
soop3                           # serve current directory
soop3 --enable-upload           # allow file uploads
soop3 --host 0.0.0.0 --port 80  # listen on all interfaces
soop3 --config server.toml      # use config file
soop3 /path/to/files            # serve specific directory
```

## config

create `server.toml`:

```toml
[server]
host = "0.0.0.0"
port = 8000
enable_upload = true
public_dir = "./files"

[security]
username = "user"
password = "pass" 
policy = "authenticate_upload"

[upload]
prepend_timestamp = true
prevent_overwrite = true
max_request_size = 1073741824

[listing]
ignore_file = ".gitignore"
```

policies: `authenticate_none`, `authenticate_upload`, `authenticate_download`, `authenticate_all`

## build

```bash
cargo build --release  # optimized binary
cargo test             # run test suite
```

## license

mit