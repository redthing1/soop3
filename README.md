# soop3

http file server

## usage

```bash
soop3                           # serve current directory
soop3 --enable-upload           # allow file uploads
soop3 --host 0.0.0.0 --port 80  # listen on all interfaces
soop3 --config server.toml      # use config file
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
policy = "authenticate_uploads"

[upload]
prepend_timestamp = true
prevent_overwrite = true
```

policies: `authenticate_none`, `authenticate_uploads`, `authenticate_all`

## build

```bash
cargo build --release  # ./target/release/soop3
cargo test             # run tests
```

## license

mit