# mojxml-rs

An experimental MOJXML parser and converter written in Rust.

License: MIT

## Convert to FlatGeobuf

```
cargo run --package mojxml-cli --release -- 15222-1107-2023.zip output.fgb
```

## Acknowledgements

- For multi-threaded Zip file extraction, we use `cloneable_seekable_reader.rs` from [google/ripunzip](https://github.com/google/ripunzip).
