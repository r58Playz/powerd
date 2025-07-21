# powerd
Intel laptop cooling/CPU tweaking tool that supports power-profiles-daemon APIs.

> [!WARNING]
> This tool may not work on your machine due to hardcoded paths/missing sysfs features. I tried to make it as flexible as possible, though.

## Usage
1. Clone the repo
2. `cargo install --path .`
3. Create a folder somewhere for powerd configuration
4. Use `powerd root dump` to create a config based off the current state and edit it
5. Write a `powerd.json` based off `DaemonConfig` in `src/daemon.rs`
6. Install and edit `powerd.service` to point to your powerd binary installation and configuration file location
7. Enable/start `powerd.service`
