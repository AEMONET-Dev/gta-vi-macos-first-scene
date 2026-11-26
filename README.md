# GTA VI first-scene renderer · integration

This source branch holds the **integration** renderer from the Apple Silicon first-scene study, local revision `a6aeaa63d9b194d2861d940c92ac7ab24539f233`. Its upstream base is [VIctory-macOS at cc8e90e](https://github.com/pragyaangaur/VIctory-macOS/tree/cc8e90e7e32f803fcb1873b3b921709f08a51829). Original notices and the [GPL license](LICENSE) accompany the code.

[Study, methods and measurements](https://github.com/AEMONET-Dev/gta-vi-macos-first-scene/tree/main) · [Upstream build notes](docs/macos.md) · [Original upstream README](UPSTREAM-README.md)

For the native shader test package run `cargo test --release -p victory-shader --lib`. To build the diagnostic Windows renderer on the Mac, use the upstream cross-toolchain described in the build notes. The native daemon binary is provided by the `victory-metal` crate. Each participant supplies the runtime and their own game files locally.
