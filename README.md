# filink-rs

A Rust implementation of the FILINK file transfer protocol for serial communication.

## Overview

filink-rs is a modern, type-safe implementation of the FILINK protocol, designed for transferring files between computers over serial connections. This implementation enables file transfers between modern systems and vintage hardware.

The protocol uses a simple character-based handshake with 128-byte blocks and XOR checksums for data integrity.

## Features

- **Send files** over serial connections
- **Receive files** over serial connections
- **Multiple file transfers** in a single session

## Installation

### Prerequisites

- Rust 2024 edition or later

### Building from source

```bash
cargo build --release
```

The binary will be available at `target/release/filink`

## Usage

### Sending files

```bash
filink --port <serial-port> send <path/to/file>
```

### Receiving files

```bash
filink --port <serial-port> receive
```

### Common options

- `--port <PORT>`: Serial port to use (e.g., /dev/ttyUSB0 or COM1) **[required]**
- `--baud <BAUD>`: Baud rate (default: 9600)
- `--data-bits <BITS>`: Data bits - 5, 6, 7, or 8 (default: 8)
- `--parity <PARITY>`: Parity - none, odd, or even (default: none)
- `--stop-bits <BITS>`: Stop bits - 1 or 2 (default: 1)
- `--byte-delay <MS>`: Delay in milliseconds between each byte when sending data blocks (default: 0)
- `--debug`: Enable protocol trace output

### Examples

Send a file using 9600 baud:

```bash
filink --port /dev/ttyUSB0 --baud 9600 send document.txt
```

Send a file with 2ms delay per byte (useful for vintage hardware at higher baud rates):

```bash
filink --port /dev/ttyUSB0 --baud 9600 --byte-delay 2 send document.txt
```

Receive files to a specific directory:

```bash
filink --port /dev/ttyUSB0 receive --output-dir ~/received-files
```

Enable debug output to see protocol details:

```bash
filink --port /dev/ttyUSB0 --debug send document.txt
```

## Filename Handling

When sending files, modern long filenames are automatically converted to 8.3 format:

- Maximum 8 characters for name
- Maximum 3 characters for extension
- Converted to uppercase
- Space-padded
- Multi-extension files (e.g., `file.tar.gz`) use first extension (`FILE    TAR`)

Examples:

- `document.txt` → `DOCUMENTTXT`
- `verylongname.html` → `VERYLONGHTM` (truncated)
- `archive.tar.gz` → `ARCHIVE TAR`
- `readme` → `README` (no extension)

When receiving files, the 8.3 format filename transmitted by the sender is converted to lowercase:

- Spaces are removed
- Extension separator (`.`) is added between name and extension
- Result is a standard lowercase filename

Examples:

- `DOCUMENT.TXT` → `document.txt`
- `README` → `readme`
- `REPORT  DOC` → `report.doc`

## Compatibility

### Tested with

- **Epson QX-16** running CP/M with QXFILINK.COM

### Reference implementations

- `EPSLINK.ASM` - CP/M assembly implementation (1984)
- `filink.c` - Unix C implementation (1994)

## Development

### Running tests

```bash
cargo test
```

Run tests with output:

```bash
cargo test -- --nocapture
```

### Project structure

```
src/
├── main.rs      - CLI interface and main loop
├── protocol.rs  - Protocol constants
├── receiver.rs  - Receiver state machine
├── sender.rs    - Sender state machine
└── serial.rs    - Serial port abstraction and mocks
```

## License

GPL-2.0

Copyright (C) 2026 Brian Johnson

This program is free software; you can redistribute it and/or modify it under the terms of the GNU General Public License as published by the Free Software Foundation; either version 2 of the License, or (at your option) any later version.

## See Also

- [FILINK Protocol Specification](FILINK-PROTOCOL.md)

## Contributing

This is a personal project implementing a historical protocol. If you find bugs or have improvements, feel free to open an issue.

## Acknowledgments

Based on the original FILINK protocol designed for serial file transfers. Thanks to the creators of EPSLINK.ASM and filink.c for their reference implementations.
