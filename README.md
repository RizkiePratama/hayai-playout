# Hayai Playout

Hayai Playout is a standalone livestreaming tool for Windows and Linux designed for seamless, continuous broadcasting. It allows users to create and dynamically manage playlists from local videos and HLS URLs, and stream the output to an RTMP destination without interruption.

This project is in the early stages of development.

## License

This project is licensed under the **GNU General Public License v3.0**. Please see the `LICENSE` file for the full text.

---

## Development Environment Setup

These instructions are tailored for developers on a **Debian-based Linux system (such as Debian, Ubuntu, or WSL2)**.

The project relies on Rust, GStreamer, and GTK4. A helper script is provided to install all necessary dependencies and configure the environment for both native (Linux) and cross-platform (Windows) compilation.

### Automated Setup

1.  **Make the script executable:**
    ```bash
    chmod +x ./script/setup-dev-env.sh
    ```

2.  **Run the script:**
    ```bash
    ./script/setup-dev-env.sh
    ```
    The script will use `sudo` to install system packages, so it may ask for your password.

3.  **Restart your shell:**
    After the script finishes, it is crucial to close and reopen your terminal session to ensure all environment changes are loaded correctly.

---

## Building the Project

Once the environment is set up, you can build the application using standard Cargo commands from the root of the project workspace.

### Building for Linux

*   **To build a debug version:**
    ```bash
    cargo build --workspace
    ```
    The binary will be located at: `./target/debug/hayai-playout-gtk`

*   **To build an optimized release version:**
    ```bash
    cargo build --workspace --release
    ```
    The binary will be located at: `./target/release/hayai-playout-gtk`

### Cross-Compiling for Windows

*   **To build a debug version for Windows:**
    ```bash
    cargo build --workspace --target x86_64-pc-windows-gnu
    ```
    The binary will be located at: `./target/x86_64-pc-windows-gnu/debug/hayai-playout-gtk.exe`

*   **To build an optimized release version for Windows:**
    ```bash
    cargo build --workspace --target x86_64-pc-windows-gnu --release
    ```
    The binary will be located at: `./target/x86_64-pc-windows-gnu/release/hayai-playout-gtk.exe`

---

## Running the Application

To run the application, execute the `hayai-playout-gtk` binary that you have built.

For example, to run a debug build on Linux:

```bash
./target/debug/hayai-playout-gtk
```
