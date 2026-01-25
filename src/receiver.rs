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

use std::marker::PhantomData;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;
use crate::serial::SerialPort;
use crate::protocol::*;

// ============================================================================
// Error Types
// ============================================================================

#[derive(Debug)]
pub enum ReceiverError {
    Io(std::io::Error),
    TransferComplete,
}

impl std::fmt::Display for ReceiverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReceiverError::Io(e) => write!(f, "I/O error: {}", e),
            ReceiverError::TransferComplete => write!(f, "Transfer complete"),
        }
    }
}

impl std::error::Error for ReceiverError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ReceiverError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ReceiverError {
    fn from(err: std::io::Error) -> Self {
        ReceiverError::Io(err)
    }
}

// ============================================================================
// States
// ============================================================================

pub struct InitialHandshake;
pub struct WaitGood;
pub struct WaitFileOrEnd;
pub struct ReceiveFilename;
pub struct EndFilename;
pub struct WaitBlockOrEOF;
pub struct ReceiveBlock;
pub struct VerifyChecksum;

// ============================================================================
// FSM Structure
// ============================================================================

pub struct ReceiverFsm<State> {
    state: PhantomData<State>,
    serial: Box<dyn SerialPort>,
    output_dir: PathBuf,
    current_file: Option<File>,
    filename_buffer: [u8; 11],
    filename_idx: usize,
    block_buffer: [u8; 128],
    bytes_received: usize,
    checksum: u8,
    debug: bool,
}

// ============================================================================
// Trait
// ============================================================================

pub trait ReceiverState: Send {
    fn step(self: Box<Self>) -> Result<Box<dyn ReceiverState>, ReceiverError>;
}

// ============================================================================
// Helper to transition states
// ============================================================================

impl<S> ReceiverFsm<S> {
    fn transition<T>(self) -> Box<ReceiverFsm<T>> {
        Box::new(ReceiverFsm {
            state: PhantomData,
            serial: self.serial,
            output_dir: self.output_dir,
            current_file: self.current_file,
            filename_buffer: self.filename_buffer,
            filename_idx: self.filename_idx,
            block_buffer: self.block_buffer,
            bytes_received: self.bytes_received,
            checksum: self.checksum,
            debug: self.debug,
        })
    }

    fn io_error(&self, e: std::io::Error) -> ReceiverError {
        let type_name = std::any::type_name::<S>();
        let state_name = type_name.split("::").last().unwrap_or(type_name);
        ReceiverError::Io(std::io::Error::new(
            e.kind(),
            format!("{} (in state: {})", e, state_name)
        ))
    }
}

// ============================================================================
// State Implementations
// ============================================================================

impl ReceiverState for ReceiverFsm<InitialHandshake> {
    fn step(self: Box<Self>) -> Result<Box<dyn ReceiverState>, ReceiverError> {
        let mut fsm = *self;

        let mut buf = [0u8; 1];
        match fsm.serial.read_timeout(&mut buf, Duration::from_secs(5)) {
            Ok(_) if buf[0] == SENDER_READY => {
                if fsm.debug { println!("Received: 'R'"); }

                fsm.serial.write_all(&[RECEIVER_READY])?;
                if fsm.debug { println!("Sent: 'S'"); }

                let next = fsm.transition::<WaitGood>();
                Ok(next as Box<dyn ReceiverState>)
            }
            Err(e) if e.kind() != std::io::ErrorKind::TimedOut => Err(fsm.io_error(e)),
            _ => {
                println!("Sender not ready");
                Ok(Box::new(fsm) as Box<dyn ReceiverState>)
            }
        }
    }
}

impl ReceiverState for ReceiverFsm<WaitGood> {
    fn step(self: Box<Self>) -> Result<Box<dyn ReceiverState>, ReceiverError> {
        let mut fsm = *self;

        let mut buf = [0u8; 1];
        match fsm.serial.read_timeout(&mut buf, Duration::from_secs(2)) {
            Ok(_) if buf[0] == GOOD => {
                if fsm.debug { println!("Received: 'G'"); }
                let next = fsm.transition::<WaitFileOrEnd>();
                Ok(next as Box<dyn ReceiverState>)
            }
            Err(e) => Err(fsm.io_error(e)),
            Ok(_) => {
                if fsm.debug { println!("Wrong character, waiting for 'G'..."); }
                Ok(Box::new(fsm) as Box<dyn ReceiverState>)
            }
        }
    }
}

impl ReceiverState for ReceiverFsm<WaitFileOrEnd> {
    fn step(self: Box<Self>) -> Result<Box<dyn ReceiverState>, ReceiverError> {
        let mut fsm = *self;

        let mut buf = [0u8; 1];
        match fsm.serial.read_timeout(&mut buf, Duration::from_secs(2)) {
            Ok(_) if buf[0] == EOT => {
                if fsm.debug { println!("Received: EOT"); }

                fsm.serial.write_all(&[BS])?;
                if fsm.debug { println!("Sent: BS"); }

                fsm.filename_idx = 0;
                let next = fsm.transition::<ReceiveFilename>();
                Ok(next as Box<dyn ReceiverState>)
            }
            Ok(_) if buf[0] == XOFF => {
                if fsm.debug { println!("Received: XOFF (All transfers complete)"); }
                Err(ReceiverError::TransferComplete)
            }
            Ok(_) => {
                if fsm.debug { println!("Received invalid char, sending 'X'"); }
                fsm.serial.write_all(&[ERROR])?;
                Ok(Box::new(fsm) as Box<dyn ReceiverState>)
            }
            Err(e) => Err(fsm.io_error(e))
        }
    }
}

impl ReceiverState for ReceiverFsm<ReceiveFilename> {
    fn step(self: Box<Self>) -> Result<Box<dyn ReceiverState>, ReceiverError> {
        let mut fsm = *self;

        let mut buf = [0u8; 1];
        match fsm.serial.read_timeout(&mut buf, Duration::from_secs(2)) {
            Ok(_) => {
                let ch = buf[0];

                if ch < 0x20 {
                    fsm.serial.write_all(&[ERROR])?;
                    if fsm.debug { println!("Invalid filename character (< 0x20), sending 'X'"); }
                    fsm.filename_idx = 0;
                    let next = fsm.transition::<WaitFileOrEnd>();
                    return Ok(next as Box<dyn ReceiverState>);
                }

                fsm.filename_buffer[fsm.filename_idx] = ch;

                fsm.serial.write_all(&[ch])?;
                if fsm.debug {
                    println!("Received filename char[{}]: '{}' - Echoed", fsm.filename_idx, ch as char);
                }

                fsm.filename_idx += 1;

                if fsm.filename_idx >= 11 {
                    let next = fsm.transition::<EndFilename>();
                    Ok(next as Box<dyn ReceiverState>)
                } else {
                    Ok(Box::new(fsm) as Box<dyn ReceiverState>)
                }
            }
            Err(e) => Err(fsm.io_error(e))
        }
    }
}

impl ReceiverState for ReceiverFsm<EndFilename> {
    fn step(self: Box<Self>) -> Result<Box<dyn ReceiverState>, ReceiverError> {
        let mut fsm = *self;

        let mut buf = [0u8; 1];
        match fsm.serial.read_timeout(&mut buf, Duration::from_secs(2)) {
            Ok(_) if buf[0] == ENQ => {
                if fsm.debug { println!("Received: ENQ"); }

                let filename = parse_filename(&fsm.filename_buffer);
                let filepath = fsm.output_dir.join(&filename);

                match File::create(&filepath) {
                    Ok(file) => {
                        if fsm.debug { println!("Created file: {:?}", filepath); }
                        fsm.current_file = Some(file);

                        fsm.serial.write_all(&[TAB])?;
                        if fsm.debug { println!("Sent: TAB"); }

                        let next = fsm.transition::<WaitBlockOrEOF>();
                        Ok(next as Box<dyn ReceiverState>)
                    }
                    Err(e) => {
                        if fsm.debug { println!("Failed to create file: {}", e); }
                        fsm.serial.write_all(&[ERROR])?;
                        fsm.filename_idx = 0;
                        let next = fsm.transition::<WaitFileOrEnd>();
                        Ok(next as Box<dyn ReceiverState>)
                    }
                }
            }
            Ok(_) => {
                if fsm.debug { println!("Expected ENQ, sending 'X'"); }
                fsm.serial.write_all(&[ERROR])?;
                fsm.filename_idx = 0;
                let next = fsm.transition::<WaitFileOrEnd>();
                Ok(next as Box<dyn ReceiverState>)
            }
            Err(e) => Err(fsm.io_error(e))
        }
    }
}

impl ReceiverState for ReceiverFsm<WaitBlockOrEOF> {
    fn step(self: Box<Self>) -> Result<Box<dyn ReceiverState>, ReceiverError> {
        let mut fsm = *self;

        let mut buf = [0u8; 1];
        match fsm.serial.read_timeout(&mut buf, Duration::from_secs(2)) {
            Ok(_) if buf[0] == STX => {
                if fsm.debug { println!("Received: STX"); }

                fsm.serial.write_all(&[PROCEED])?;
                if fsm.debug { println!("Sent: 'P'"); }

                fsm.bytes_received = 0;
                fsm.checksum = 0;
                let next = fsm.transition::<ReceiveBlock>();
                Ok(next as Box<dyn ReceiverState>)
            }
            Ok(_) if buf[0] == ETX => {
                if fsm.debug { println!("Received: ETX (End of file)"); }

                fsm.current_file = None;

                let next = fsm.transition::<WaitFileOrEnd>();
                Ok(next as Box<dyn ReceiverState>)
            }
            Ok(_) => {
                if fsm.debug { println!("Expected STX or ETX, sending 'N'"); }
                fsm.serial.write_all(&[NAK])?;
                Ok(Box::new(fsm) as Box<dyn ReceiverState>)
            }
            Err(e) => Err(fsm.io_error(e))
        }
    }
}

impl ReceiverState for ReceiverFsm<ReceiveBlock> {
    fn step(self: Box<Self>) -> Result<Box<dyn ReceiverState>, ReceiverError> {
        let mut fsm = *self;

        while fsm.bytes_received < 128 {
            let mut buf = [0u8; 1];
            match fsm.serial.read_timeout(&mut buf, Duration::from_secs(2)) {
                Ok(_) => {
                    let byte = buf[0];
                    fsm.block_buffer[fsm.bytes_received] = byte;
                    fsm.checksum ^= byte;
                    fsm.bytes_received += 1;
                }
                Err(e) => return Err(fsm.io_error(e))
            }
        }

        if fsm.debug { println!("Received: 128 byte block"); }

        let next = fsm.transition::<VerifyChecksum>();
        Ok(next as Box<dyn ReceiverState>)
    }
}

impl ReceiverState for ReceiverFsm<VerifyChecksum> {
    fn step(self: Box<Self>) -> Result<Box<dyn ReceiverState>, ReceiverError> {
        let mut fsm = *self;

        let mut buf = [0u8; 1];
        match fsm.serial.read_timeout(&mut buf, Duration::from_secs(2)) {
            Ok(_) => {
                let received_checksum = buf[0];
                if fsm.debug {
                    println!("Received: Checksum 0x{:02X}, Expected: 0x{:02X}",
                             received_checksum, fsm.checksum);
                }

                if received_checksum == fsm.checksum {
                    if fsm.debug { println!("Checksum OK"); }

                    if let Some(ref mut file) = fsm.current_file {
                        file.write_all(&fsm.block_buffer)?;
                    }

                    fsm.serial.write_all(&[GOOD])?;
                    if fsm.debug { println!("Sent: 'G'"); }

                    let next = fsm.transition::<WaitBlockOrEOF>();
                    Ok(next as Box<dyn ReceiverState>)
                } else {
                    if fsm.debug { println!("Checksum mismatch!"); }

                    fsm.serial.write_all(&[BAD])?;
                    if fsm.debug { println!("Sent: 'B'"); }

                    let next = fsm.transition::<WaitBlockOrEOF>();
                    Ok(next as Box<dyn ReceiverState>)
                }
            }
            Err(e) => Err(fsm.io_error(e))
        }
    }
}

// ============================================================================
// Constructor & Runner
// ============================================================================

impl ReceiverFsm<InitialHandshake> {
    pub fn new(serial: Box<dyn SerialPort>, output_dir: PathBuf, debug: bool) -> Box<dyn ReceiverState> {
        Box::new(ReceiverFsm {
            state: PhantomData::<InitialHandshake>,
            serial,
            output_dir,
            current_file: None,
            filename_buffer: [b' '; 11],
            filename_idx: 0,
            block_buffer: [0; 128],
            bytes_received: 0,
            checksum: 0,
            debug,
        })
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

fn parse_filename(buffer: &[u8; 11]) -> String {
    let mut result = String::new();

    let name: String = buffer[0..8]
        .iter()
        .map(|&b| (b as char).to_lowercase().to_string())
        .collect::<String>()
        .trim_end()
        .to_string();

    let ext: String = buffer[8..11]
        .iter()
        .map(|&b| (b as char).to_lowercase().to_string())
        .collect::<String>()
        .trim_end()
        .to_string();

    result.push_str(&name);
    if !ext.is_empty() {
        result.push('.');
        result.push_str(&ext);
    }

    result
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serial::MockSerialPort;

    fn run_receiver(mut fsm: Box<dyn ReceiverState>) -> Result<(), ReceiverError> {
        loop {
            match fsm.step() {
                Ok(next) => fsm = next,
                Err(ReceiverError::TransferComplete) => return Ok(()),
                Err(e) => return Err(e),
            }
        }
    }

    #[test]
    fn test_parse_filename() {
        let buffer = *b"TEST    TXT";
        let result = parse_filename(&buffer);
        assert_eq!(result, "test.txt");

        let buffer = *b"EXAMPLE C  ";
        let result = parse_filename(&buffer);
        assert_eq!(result, "example.c");

        let buffer = *b"README     ";
        let result = parse_filename(&buffer);
        assert_eq!(result, "readme");
    }

    #[test]
    fn test_receiver_full_transfer() {
        let temp_dir = std::env::temp_dir();

        let mut responses = vec![
            Some(SENDER_READY),
            Some(GOOD),
            Some(EOT),
        ];

        for ch in b"SMALL   TXT" {
            responses.push(Some(*ch));
        }

        responses.push(Some(ENQ));

        responses.push(Some(STX));

        let mut block = b"Test data".to_vec();
        while block.len() < 128 {
            block.push(0x1A);
        }

        let checksum: u8 = block.iter().fold(0u8, |acc, &b| acc ^ b);

        for &byte in &block {
            responses.push(Some(byte));
        }
        responses.push(Some(checksum));

        responses.push(Some(ETX));

        responses.push(Some(XOFF));

        let mut expected_writes = vec![
            RECEIVER_READY,
            BS,
        ];

        expected_writes.extend_from_slice(b"SMALL   TXT");
        expected_writes.push(TAB);
        expected_writes.push(PROCEED);
        expected_writes.push(GOOD);

        let mock_serial = Box::new(MockSerialPort::new(responses, expected_writes));
        let fsm = ReceiverFsm::new(mock_serial, temp_dir.clone(), true);

        match run_receiver(fsm) {
            Ok(()) => {},
            Err(ReceiverError::TransferComplete) => {},
            Err(e) => panic!("Transfer failed: {:?}", e),
        }

        let filepath = temp_dir.join("small.txt");
        assert!(filepath.exists(), "File should be created");

        let content = std::fs::read(&filepath).expect("Should read file");
        assert_eq!(&content[0..9], b"Test data", "File content should match");

        std::fs::remove_file(&filepath).ok();
    }

    #[test]
    fn test_receiver_bad_checksum_retry() {
        let temp_dir = std::env::temp_dir();

        let mut responses = vec![
            Some(SENDER_READY), 
            Some(GOOD),
            Some(EOT),
        ];

        for ch in b"BADCS   TXT" {
            responses.push(Some(*ch));
        }

        responses.push(Some(ENQ));

        responses.push(Some(STX));

        let mut block = b"Bad cs".to_vec();
        while block.len() < 128 {
            block.push(0x1A);
        }

        let correct_checksum: u8 = block.iter().fold(0u8, |acc, &b| acc ^ b);
        let bad_checksum = correct_checksum ^ 0xFF;

        for &byte in &block {
            responses.push(Some(byte));
        }
        responses.push(Some(bad_checksum));

        responses.push(Some(STX));
        for &byte in &block {
            responses.push(Some(byte));
        }
        responses.push(Some(correct_checksum));

        responses.push(Some(ETX));

        responses.push(Some(XOFF));

        let mut expected_writes = vec![
            RECEIVER_READY,
            BS,
        ];

        expected_writes.extend_from_slice(b"BADCS   TXT");
        expected_writes.push(TAB);
        expected_writes.push(PROCEED);
        expected_writes.push(BAD);
        expected_writes.push(PROCEED);
        expected_writes.push(GOOD);

        let mock_serial = Box::new(MockSerialPort::new(responses, expected_writes));
        let fsm = ReceiverFsm::new(mock_serial, temp_dir.clone(), true);

        match run_receiver(fsm) {
            Ok(()) => {},
            Err(ReceiverError::TransferComplete) => {},
            Err(e) => panic!("Transfer failed: {:?}", e),
        }

        let filepath = temp_dir.join("badcs.txt");
        assert!(filepath.exists(), "File should be created");

        let content = std::fs::read(&filepath).expect("Should read file");
        assert_eq!(&content[0..6], b"Bad cs", "File content should match");

        std::fs::remove_file(&filepath).ok();
    }

    #[test]
    fn test_receiver_multiple_blocks() {
        let temp_dir = std::env::temp_dir();

        let mut responses = vec![
            Some(SENDER_READY),
            Some(GOOD),
            Some(EOT),
        ];

        for ch in b"MULTI   TXT" {
            responses.push(Some(*ch));
        }

        responses.push(Some(ENQ));

        for block_num in 0..3 {
            responses.push(Some(STX));

            let mut block = vec![0u8; 128];
            for i in 0..128 {
                block[i] = ((block_num * 128 + i) % 256) as u8;
            }

            let checksum: u8 = block.iter().fold(0u8, |acc, &b| acc ^ b);

            for &byte in &block {
                responses.push(Some(byte));
            }
            responses.push(Some(checksum));
        }

        responses.push(Some(ETX));

        responses.push(Some(XOFF));

        let mut expected_writes = vec![
            RECEIVER_READY,
            BS,
        ];

        expected_writes.extend_from_slice(b"MULTI   TXT");
        expected_writes.push(TAB);

        for _ in 0..3 {
            expected_writes.push(PROCEED);
            expected_writes.push(GOOD);
        }

        let mock_serial = Box::new(MockSerialPort::new(responses, expected_writes));
        let fsm = ReceiverFsm::new(mock_serial, temp_dir.clone(), true);

        match run_receiver(fsm) {
            Ok(()) => {},
            Err(ReceiverError::TransferComplete) => {},
            Err(e) => panic!("Transfer failed: {:?}", e),
        }

        let filepath = temp_dir.join("multi.txt");
        assert!(filepath.exists(), "File should be created");

        let content = std::fs::read(&filepath).expect("Should read file");
        assert_eq!(content.len(), 384, "File should be 3 blocks (384 bytes)");

        for (i, &byte) in content.iter().enumerate() {
            assert_eq!(byte, (i % 256) as u8, "Byte at position {} should match", i);
        }

        std::fs::remove_file(&filepath).ok();
    }

    #[test]
    fn test_receiver_handshake_retry() {
        let temp_dir = std::env::temp_dir();

        let responses = vec![None, Some(SENDER_READY), Some(GOOD)];

        let expected_writes = vec![
            RECEIVER_READY,
        ];

        let mock_serial = Box::new(MockSerialPort::new(responses, expected_writes));
        let mut fsm = ReceiverFsm::new(mock_serial, temp_dir, true);

        for _ in 0..3 {
            fsm = fsm.step().expect("Should succeed");
        }
    }

    #[test]
    fn test_receiver_multiple_files() {
        let temp_dir = std::env::temp_dir();

        let mut responses = vec![
            Some(SENDER_READY),
            Some(GOOD),
        ];

        responses.push(Some(EOT));
        for ch in b"FILE1   TXT" {
            responses.push(Some(*ch));
        }
        responses.push(Some(ENQ));
        responses.push(Some(STX));

        let mut block1 = b"First file data".to_vec();
        while block1.len() < 128 {
            block1.push(0x1A);
        }
        let checksum1: u8 = block1.iter().fold(0u8, |acc, &b| acc ^ b);
        for &byte in &block1 {
            responses.push(Some(byte));
        }
        responses.push(Some(checksum1));
        responses.push(Some(ETX));

        responses.push(Some(EOT));
        for ch in b"FILE2   TXT" {
            responses.push(Some(*ch));
        }
        responses.push(Some(ENQ));
        responses.push(Some(STX));

        let mut block2 = b"Second file data".to_vec();
        while block2.len() < 128 {
            block2.push(0x1A);
        }
        let checksum2: u8 = block2.iter().fold(0u8, |acc, &b| acc ^ b);
        for &byte in &block2 {
            responses.push(Some(byte));
        }
        responses.push(Some(checksum2));
        responses.push(Some(ETX));

        responses.push(Some(XOFF));

        let mut expected_writes = vec![
            RECEIVER_READY,
        ];

        expected_writes.push(BS);
        expected_writes.extend_from_slice(b"FILE1   TXT");
        expected_writes.push(TAB);
        expected_writes.push(PROCEED);
        expected_writes.push(GOOD);

        expected_writes.push(BS);
        expected_writes.extend_from_slice(b"FILE2   TXT");
        expected_writes.push(TAB);
        expected_writes.push(PROCEED);
        expected_writes.push(GOOD);

        let mock_serial = Box::new(MockSerialPort::new(responses, expected_writes));
        let fsm = ReceiverFsm::new(mock_serial, temp_dir.clone(), true);

        match run_receiver(fsm) {
            Ok(()) => {},
            Err(ReceiverError::TransferComplete) => {},
            Err(e) => panic!("Transfer failed: {:?}", e),
        }

        let filepath1 = temp_dir.join("file1.txt");
        let filepath2 = temp_dir.join("file2.txt");

        assert!(filepath1.exists(), "First file should be created");
        assert!(filepath2.exists(), "Second file should be created");

        let content1 = std::fs::read(&filepath1).expect("Should read first file");
        assert_eq!(&content1[0..15], b"First file data", "First file content should match");

        let content2 = std::fs::read(&filepath2).expect("Should read second file");
        assert_eq!(&content2[0..16], b"Second file data", "Second file content should match");

        std::fs::remove_file(&filepath1).ok();
        std::fs::remove_file(&filepath2).ok();
    }
}
