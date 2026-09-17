[![Dual License](https://img.shields.io/badge/license-MIT-blue)](./LICENSE)
[![Stars](https://img.shields.io/github/stars/joaquinbejar/Option-Chain-OrderBook-Backend.svg)](https://github.com/joaquinbejar/Option-Chain-OrderBook-Backend/stargazers)
[![Issues](https://img.shields.io/github/issues/joaquinbejar/Option-Chain-OrderBook-Backend.svg)](https://github.com/joaquinbejar/Option-Chain-OrderBook-Backend/issues)
[![PRs](https://img.shields.io/github/issues-pr/joaquinbejar/Option-Chain-OrderBook-Backend.svg)](https://github.com/joaquinbejar/Option-Chain-OrderBook-Backend/pulls)

[![Build Status](https://img.shields.io/github/workflow/status/joaquinbejar/Option-Chain-OrderBook-Backend/CI)](https://github.com/joaquinbejar/Option-Chain-OrderBook-Backend/actions)
[![Coverage](https://img.shields.io/codecov/c/github/joaquinbejar/Option-Chain-OrderBook-Backend)](https://codecov.io/gh/joaquinbejar/Option-Chain-OrderBook-Backend)
[![Dependencies](https://img.shields.io/librariesio/github/joaquinbejar/Option-Chain-OrderBook-Backend)](https://libraries.io/github/joaquinbejar/Option-Chain-OrderBook-Backend)



{{readme}}


## 🛠 Makefile Commands

This project includes a `Makefile` with common tasks to simplify development. Here's a list of useful commands:

### 🔧 Build & Run

```sh
make build         # Compile the project
make release       # Build in release mode
make run           # Run the main binary
```

### 🧪 Test & Quality

```sh
make test          # Run all tests
make fmt           # Format code
make fmt-check     # Check formatting without applying
make lint          # Run clippy with warnings as errors
make lint-fix      # Auto-fix lint issues
make fix           # Auto-fix Rust compiler suggestions
make check         # Run fmt-check + lint + test
```

### 📦 Packaging & Docs

```sh
make doc           # Check for missing docs via clippy
make doc-open      # Build and open Rust documentation
make create-doc    # Generate internal docs
make readme        # Regenerate README using cargo-readme
make publish       # Prepare and publish crate to crates.io
```

### 📈 Coverage & Benchmarks

```sh
make coverage            # Generate code coverage report (XML)
make coverage-html       # Generate HTML coverage report
make open-coverage       # Open HTML report
make bench               # Run benchmarks using Criterion
make bench-show          # Open benchmark report
make bench-save          # Save benchmark history snapshot
make bench-compare       # Compare benchmark runs
make bench-json          # Output benchmarks in JSON
make bench-clean         # Remove benchmark data
```

### 🧪 Git & Workflow Helpers

```sh
make git-log             # Show commits on current branch vs main
make check-spanish       # Check for Spanish words in code
make zip                 # Create zip without target/ and temp files
make tree                # Visualize project tree (excludes common clutter)
```

### 🤖 GitHub Actions (via act)

```sh
make workflow-build      # Simulate build workflow
make workflow-lint       # Simulate lint workflow
make workflow-test       # Simulate test workflow
make workflow-coverage   # Simulate coverage workflow
make workflow            # Run all workflows
```

ℹ️ Requires act for local workflow simulation and cargo-tarpaulin for coverage.

## Contribution and Contact

We welcome contributions to this project! If you would like to contribute, please follow these steps:

1. Fork the repository.
2. Create a new branch for your feature or bug fix.
3. Make your changes and ensure that the project still builds and all tests pass.
4. Commit your changes and push your branch to your forked repository.
5. Submit a pull request to the main repository.

If you have any questions, issues, or would like to provide feedback, please feel free to contact the project
maintainer:

### **Contact Information**
- **Author**: Joaquín Béjar García
- **Email**: jb@taunais.com
- **Telegram**: [@joaquin_bejar](https://t.me/joaquin_bejar)
- **Repository**: <https://github.com/joaquinbejar/Option-Chain-OrderBook-Backend>


We appreciate your interest and look forward to your contributions!

**License**: MIT

<!-- related-projects:start -->
## Related projects

Repositories by the same author that this project depends on, and repositories that depend on it.

### Depends on

| Repository | Description |
|------------|-------------|
| [Option-Chain-OrderBook](https://github.com/joaquinbejar/Option-Chain-OrderBook) · [crates.io](https://crates.io/crates/option-chain-orderbook) | Option chain order book system (underlying, expiration, strike) built on OrderBook-rs, PriceLevel and OptionStratLib. |
| [OptionStratLib](https://github.com/joaquinbejar/OptionStratLib) · [crates.io](https://crates.io/crates/optionstratlib) | Options pricing, Greeks, strategies and simulation library. |
| [OrderBook-rs](https://github.com/joaquinbejar/OrderBook-rs) · [crates.io](https://crates.io/crates/orderbook-rs) | High-performance, lock-free limit order book and matching engine. |

<!-- related-projects:end -->
