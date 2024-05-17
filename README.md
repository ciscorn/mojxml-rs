# mojxml-rs

An experimental MOJXML-to-FlatGeobuf converter written in Rust.

```
cargo run --package mojxml-cli --release -- 15222-1107-2023.zip output.fgb
```

License: MIT

## Acknowledgements

- For multi-threaded Zip file extraction, we use `cloneable_seekable_reader.rs` from [google/ripunzip](https://github.com/google/ripunzip).
