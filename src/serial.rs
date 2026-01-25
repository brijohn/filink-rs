// Copyright (C) 2026 Brian Johnson
//
// This program is free software; you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation; either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License along
// with this program; if not, write to the Free Software Foundation, Inc.,
// 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA.

use std::time::Duration;
use serialport::{SerialPort as SerialPortTrait, DataBits, Parity, StopBits};

// ============================================================================
// SerialPort Trait
// ============================================================================

/// Trait for serial port operations needed by the filink protocol
pub trait SerialPort: Send {
    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()>;

    fn read_timeout(&mut self, buf: &mut [u8], timeout: Duration) -> std::io::Result<usize>;
}

// ============================================================================
// Real Serial Port Implementation
// ============================================================================

/// Real serial port implementation that wraps the serialport crate
pub struct RealSerialPort {
    port: Box<dyn SerialPortTrait>,
}

impl RealSerialPort {
    pub fn open(
        port_name: &str,
        baud_rate: u32,
        data_bits: DataBits,
        parity: Parity,
        stop_bits: StopBits,
    ) -> Result<Self, serialport::Error> {
        let port = serialport::new(port_name, baud_rate)
            .data_bits(data_bits)
            .parity(parity)
            .stop_bits(stop_bits)
            .timeout(Duration::from_millis(100))
            .open()?;

        Ok(RealSerialPort { port })
    }
}

impl SerialPort for RealSerialPort {
    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        self.port.write_all(buf)?;
        self.port.flush()?;
        Ok(())
    }

    fn read_timeout(&mut self, buf: &mut [u8], timeout: Duration) -> std::io::Result<usize> {
        self.port.set_timeout(timeout)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        self.port.read(buf)
    }
}

// ============================================================================
// Mock Serial Port for Testing
// ============================================================================

#[cfg(test)]
pub struct MockSerialPort {
    // Data to return on reads (None = timeout)
    read_buffer: Vec<Option<u8>>,
    read_pos: usize,
    // Track what was written
    write_log: Vec<u8>,
    // Expected writes for verification
    expected_writes: Vec<u8>,
}

#[cfg(test)]
impl MockSerialPort {
    pub fn new(responses: Vec<Option<u8>>, expected_writes: Vec<u8>) -> Self {
        MockSerialPort {
            read_buffer: responses,
            read_pos: 0,
            write_log: Vec::new(),
            expected_writes,
        }
    }
}

#[cfg(test)]
impl SerialPort for MockSerialPort {
    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        self.write_log.extend_from_slice(buf);
        Ok(())
    }

    fn read_timeout(&mut self, buf: &mut [u8], _timeout: Duration) -> std::io::Result<usize> {
        // Out of responses = timeout
        if self.read_pos >= self.read_buffer.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "Mock timeout"
            ));
        }

        // If current response is None = timeout
        if self.read_buffer[self.read_pos].is_none() {
            self.read_pos += 1;
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "Mock timeout"
            ));
        }

        let mut bytes_read = 0;
        while bytes_read < buf.len() && self.read_pos < self.read_buffer.len() {
            match self.read_buffer[self.read_pos] {
                Some(byte) => {
                    buf[bytes_read] = byte;
                    bytes_read += 1;
                    self.read_pos += 1;
                }
                None => break,  // Stop at timeout marker
            }
        }

        Ok(bytes_read)
    }
}

#[cfg(test)]
impl Drop for MockSerialPort {
    fn drop(&mut self) {
        assert_eq!(
            self.read_pos,
            self.read_buffer.len(),
            "MockSerialPort dropped with {} unconsumed responses (read {} of {} bytes)",
            self.read_buffer.len() - self.read_pos,
            self.read_pos,
            self.read_buffer.len()
        );

        assert_eq!(
            &self.write_log,
            &self.expected_writes,
            "MockSerialPort write log mismatch!\nExpected {} bytes:\n{:02X?}\nGot {} bytes:\n{:02X?}",
            self.expected_writes.len(),
            self.expected_writes,
            self.write_log.len(),
            self.write_log
        );
    }
}

