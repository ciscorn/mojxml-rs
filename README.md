# mojxml-rs

An experimental MOJXML（MOJ 地図XML）parser and converter written in Rust.

License: MIT

## Convert to FlatGeobuf

```
cargo run --package mojxml-cli --release -- 15222-1107-2023.zip output.fgb
```

## Benchmark

Input: [15222-1107-2023.zip](https://www.geospatial.jp/ckan/dataset/houmusyouchizu-2024-1-824), excluding no-crs data

- mojxml-rs: **8.21s**
- [mojxml-py](https://github.com/MIERUNE/mojxml-py): 56.4s

## Acknowledgements

- For multi-threaded Zip file extraction, we use `cloneable_seekable_reader.rs` from [google/ripunzip](https://github.com/google/ripunzip).
