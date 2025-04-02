
# ui-events

[![Build Status](https://img.shields.io/github/actions/workflow/status/your-username/ui-events/rust.yml?branch=main)](https://github.com/your-username/ui-events/actions) [![Crates.io](https://img.shields.io/crates/v/ui-events.svg)](https://crates.io/crates/ui-events) [![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A cross-platform Rust library designed to capture specific UI interaction events using native operating system accessibility APIs (initially focusing on macOS `AXObserver` capabilities, with planned support for Windows UI Automation and Linux AT-SPI) and stream them over a websocket connection.

### Motivation

Understanding user interface interactions like focus shifts, window changes, and value modifications is essential for building context-aware applications, automation tools, and analytics platforms such as [[screenpipe]]. Accessing these events consistently across platforms often involves complex, platform-specific code.

`ui-events` aims to provide a performant Rust core that leverages native accessibility APIs, exposing a stream of key UI events via a simple websocket interface accessible from any language (JavaScript, Python, Go, etc.).

### Features (Based on typical `AXObserver` capabilities)

*   **macOS Focus:** Captures events like application activation/deactivation, window creation/destruction/movement/resizing, focused UI element changes, and potentially value changes or selection changes within elements.
*   **Cross-Platform Goal:** Targets macOS (initially), Windows (via UI Automation), and Linux (via AT-SPI).
*   **Real-time Websocket Stream:** Provides a low-latency stream of UI events over a local websocket server.
*   **Language Agnostic:** Consumable from any language via standard websockets.
*   **Performant Rust Core:** Built with Rust for efficiency and reliability.

*Note: Direct capture of low-level mouse clicks or raw keyboard presses might require different OS mechanisms; `ui-events` focuses on events reported through the accessibility layer.*

### Architecture

```
+---------------------------------+      +---------------------+      +-------------------+      +---------------------+
| Native Accessibility Events     | ---> | ui-events           | ---> | Websocket Server  | ---> | Client Application  |
| (AXObserver, UIA, AT-SPI)       |      | (Rust Core Listener)|      | (ws://localhost:xxxx) |      | (JS, Python, etc.)  |
+---------------------------------+      +---------------------+      +-------------------+      +---------------------+
```

### Quick Start

1.  **Install/Run the Core Service:**
    ```bash
    # Build from source (ensure Rust toolchain is installed)
    cargo build --release
    # Run the binary (requires accessibility permissions)
    ./target/release/ui-events
    # Or: Instructions for pre-built binaries if available
    ```
    Grant accessibility permissions when prompted by the OS. The service hosts the websocket server (default: `ws://localhost:9001` - confirm/specify port).

2.  **Connect from Your Client:**

    *JavaScript Example:*
    ```javascript
    const ws = new WebSocket('ws://localhost:9001'); // Use the configured port

    ws.onmessage = (event) => {
      const event_data = JSON.parse(event.data);
      console.log('Received UI Event:', event_data);
      // Process event data (e.g., focus change, window created)
    };

    ws.onerror = (error) => {
      console.error('WebSocket Error:', error);
    };

    ws.onopen = () => {
      console.log('Connected to ui-events');
    };
    ```

    *Python Example:*
    ```python
    import websocket
    import json
    import threading
    # (Ensure 'websocket-client' library is installed: pip install websocket-client)

    def on_message(ws, message):
        event_data = json.loads(message)
        print(f"Received UI Event: {event_data}")
        # Process event data

    def on_error(ws, error):
        print(f"Error: {error}")

    def on_close(ws, close_status_code, close_msg):
        print("### Connection Closed ###")

    def on_open(ws):
        print("WebSocket Connection Opened")

    def run_ws():
        ws = websocket.WebSocketApp("ws://localhost:9001/", # Use configured port
                                  on_open=on_open,
                                  on_message=on_message,
                                  on_error=on_error,
                                  on_close=on_close)
        ws.run_forever()

    if __name__ == "__main__":
        ws_thread = threading.Thread(target=run_ws)
        ws_thread.start()
        # Keep main thread alive or join ws_thread
    ```

### Event Schema (Example Structure)

Events streamed over the websocket follow a consistent JSON structure. The exact `event_type` and `details` will depend on the specific accessibility event captured.

```json
{
  "event_type": "string", // e.g., "focus_changed", "window_created", "value_changed", "application_activated"
  "timestamp": "iso8601_string", // UTC timestamp
  "application_name": "string | null", // Name of the relevant application
  "window_title": "string | null", // Title of the relevant window
  "element_details": { // Information about the UI element involved, if applicable
    "role": "string | null", // e.g., "AXTextField", "AXButton", "AXWindow"
    "identifier": "string | null", // Accessibility label or identifier
    "value": "string | number | boolean | null", // Current value, if relevant and available
    "position": { "x": number, "y": number } | null,
    "size": { "width": number, "height": number } | null
  },
  "event_specific_data": {
    // Optional details specific to the event_type
    // e.g., for "window_moved": { "new_position": { "x": ..., "y": ... } }
  }
}
```
*(A detailed `SCHEMA.md` should document the specific events captured on each platform)*

### Supported Platforms & APIs

*   **macOS:** [Version Range, e.g., 11.0+] (via `AXObserver` / `AppKit` Accessibility) - Status: [Developing/Experimental]
*   **Windows:** [Version Range] (via `UI Automation`) - Status: [Planned]
*   **Linux:** (via `AT-SPI`) - Status: [Planned]

*Note: Capturing accessibility events requires user-granted permissions on all platforms.*

### TODOS

- [ ] Add Windows support
- [ ] Add Linux support
- [ ] Add documentation
- [ ] Add tests
- [ ] Provide serverless alternative

### Contributing

Contributions are welcome! Please see `CONTRIBUTING.md`.

### License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
