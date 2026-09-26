# cloop (archived)

> [!IMPORTANT]
> **cloop has been merged into [loopgen-rs](https://github.com/adventurewave-labs/loopgen-rs) (v0.3.0) and this repository is archived.**

loopgen now carries cloop's two useful ideas:

- **Named loops**: `loopgen --save-as NAME`, `--run NAME`, `--list`, `--show`, `--remove`
- **Command-check stop mode**: `loopgen --until "cargo test"` ends the loop when the command exits 0

It also brings a parsed `LOOP_STATUS` contract, a max-iteration cap, an optional verify gate, tests and CI. See the [loopgen README](https://github.com/adventurewave-labs/loopgen-rs#readme).

The original cloop source stays in this repository's git history.
