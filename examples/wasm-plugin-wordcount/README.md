# Example WASM plugin: word count

A Axiom plugin with one tool, `count_words`, written in Rust. It implements
`axiom:plugin/tools` from [`crates/engines/wit/plugin.wit`](../../crates/engines/wit/plugin.wit)
and imports nothing, so it runs sandboxed: no files, network or environment.

## Build

```sh
rustup target add wasm32-unknown-unknown
cargo build --release --target wasm32-unknown-unknown
wasm-tools component new target/wasm32-unknown-unknown/release/wasm_plugin_wordcount.wasm -o wordcount.wasm
```

[`wasm-tools`](https://github.com/bytecodealliance/wasm-tools) turns the core module into a component
(`cargo install wasm-tools`, or a release download). `wasm-tools component wit wordcount.wasm` shows
what it exports.

## Use

Axiom built with `--features wasm-plugins`, and in `config.toml`:

```toml
[[plugins.wasm]]
id = "wordcount"
path = "/full/path/to/wordcount.wasm"
```

Its tool is then `wordcount_count_words`, everywhere: the app, the CLI
(`axiom-cli wordcount-count-words --text "one two"`), MCP and the agent.

## Check it

```sh
AXIOM_TEST_WASM_PLUGIN=/full/path/to/wordcount.wasm \
  cargo test -p axiom-engines --features wasm-plugins the_example_plugin
```
