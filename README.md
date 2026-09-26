# libuvmanager

An experimental Rust adapter for managing a Python virtual environment with uv without launching the uv CLI executable. The uv source is pinned as a Git submodule at `vendor/uv` (release `0.12.19`). No files inside uv are modified.

```sh
git clone --recurse-submodules <this-repository>
cargo check --all-targets
cargo run --example basic
```

In a host project, depend on this checkout with `libuvmanager = { path = "/path/to/libuvmanager" }` and use a Tokio runtime. Keep the submodule initialized in the checkout used by Cargo.

The main API is `PythonEnvironment`: `create`, `install`, `uninstall`, and `sync_requirements` call uv inside the Rust process. `run_code` and `run_script` launch the resulting Python interpreter as a child process and capture its output. `invoke` accepts arbitrary uv arguments for commands without a convenience method.
The async management methods require a Tokio runtime; `python_command` returns a standard `std::process::Command` for applications that need custom arguments or streaming I/O.

```rust
use libuvmanager::PythonEnvironment;

# async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
let env = PythonEnvironment::new(".venv")?;
env.create(Some("3.12")).await?;
env.install(&["requests>=2,<3"]).await?;
let output = env.run_code("import requests; print(requests.__version__)")?;
assert!(output.status.success());
# Ok(())
# }
```

## Integration boundary

uv is organized as Rust crates, but its `uv::run` command dispatcher is marked as an internal API. uv explicitly documents that its `main` entry point assumes control of the process and may behave unexpectedly when called more than once. This adapter calls `uv::run` instead of `main`, serializes calls, and initializes uv's global state once. That is useful for prototyping but is **not** a stable upstream contract. Pin uv and compile-check every upgrade.

The dispatcher writes to the host process's stdout/stderr and reads process-wide configuration and environment. Some options, notably `--directory`, change the host process working directory. Do not use them through `invoke` in a multithreaded host. uv's initialization also installs process-wide logging, so hosts with their own tracing subscriber need integration testing. If the first dispatcher call returns an error, the adapter blocks later calls because uv may be partly initialized; restart the host process in that case.

Python still executes in a child process. uv manages Python installations, environments, and packages; it does not embed the CPython interpreter or provide a Python object ABI. An in-process scripting engine would need a separate CPython embedding layer such as PyO3, plus its own thread and GIL policy.

## Updating uv

Move the submodule to a chosen uv release, then run `cargo check --all-targets` and a smoke test that creates a virtual environment and installs a local wheel. uv's internal dispatcher and command types can change between releases, so expect to adapt this crate even when uv's CLI remains compatible.
