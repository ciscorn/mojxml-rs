# mojxml-rs

An experimental MOJXML-to-FlatGeobuf converter written in Rust.

```
cargo run --package mojxml-cli --release -- 15222-1107-2023.zip output.fgb
```

## Known Issues

- Zip extraction is the main bottleneck, which is slower than XML parsing.
