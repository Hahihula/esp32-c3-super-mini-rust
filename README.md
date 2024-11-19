# esp32-c3-super-mini board

inspired by this great repository -> https://github.com/sidharthmohannair/Tutorial-ESP32-C3-Super-Mini

You can find most of the information about the board there. Here I just want to stash some basic RUST examples compatible with esp_hal v0.23.1

You can also copy the .cargo/config.toml and build.rs files from this repository if you just want to build some rust code for the board.

## getting it up and running

`cargo build --release`

`cargo espflash flash --release`

`cargo espflash monitor`

## Examples

- [blink](examples/blink.rs)
  `cargo espflash flash --release --example blink`

- [snow](examples/sk6812_rgbw_embassy.rs)
  `cargo espflash flash --release --example sk6812_rgbw_embassy`
