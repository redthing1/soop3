# soop3

the based http fileserver

## usage

```bash
soop3                           # serve current directory
soop3 --enable-upload           # allow uploads
soop3 --host 0.0.0.0 --port 80  # listen on all interfaces
soop3 --config server.toml      # use config file
soop3 /path/to/files            # serve directory
```

## config

```toml
[server]
host = "0.0.0.0"
port = 8000
enable_upload = true
public_dir = "./files"

[security]
username = "admin"
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
cargo build --release                # production binary
cargo test --features test-helpers   # run tests
```

## license

mit