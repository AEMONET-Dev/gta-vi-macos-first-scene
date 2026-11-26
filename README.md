# GTA VI first-scene renderer · graphics-halves

This source branch holds the **graphics-halves** renderer from the Apple Silicon first-scene study, local revision `d253f5428377a9ef47790216d0b4b77527a9607e`. Its upstream base is [VIctory-macOS at cc8e90e](https://github.com/pragyaangaur/VIctory-macOS/tree/cc8e90e7e32f803fcb1873b3b921709f08a51829). Original notices and the [GPL license](LICENSE) accompany the code.

[Study, methods and measurements](https://github.com/AEMONET-Dev/gta-vi-macos-first-scene/tree/main) · [Upstream build notes](docs/macos.md) · [Original upstream README](UPSTREAM-README.md)

For the native shader test package run `cargo test --release -p victory-shader --lib`. To build the diagnostic Windows renderer on the Mac, use the upstream cross-toolchain described in the build notes. The native daemon binary is provided by the `victory-metal` crate. Each participant supplies the runtime and their own game files locally.
