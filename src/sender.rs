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
use std::path::PathBuf;
use std::io::Read;
use std::time::Duration;
use crate::serial::SerialPort;
use crate::protocol::*;

// ============================================================================
// Error Types
// ============================================================================

#[derive(Debug)]
pub enum SenderError {
    Io(std::io::Error),
    TransferComplete,
}

impl std::fmt::Display for SenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SenderError::Io(e) => write!(f, "I/O error: {}", e),
            SenderError::TransferComplete => write!(f, "Transfer complete"),
        }
    }
}

impl std::error::Error for SenderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SenderError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for SenderError {
    fn from(err: std::io::Error) -> Self {
        SenderError::Io(err)
    }
}

// ============================================================================
// States
// ============================================================================

pub struct InitialHandshake;
pub struct SendGood;
pub struct RequestFilename;
pub struct TransmitFilename;
pub struct EndFilename;
pub struct CheckMoreData;
pub struct TransmitBlock;
pub struct SendChecksum;
pub struct EndFile;

// ============================================================================
// FSM Structure
// ============================================================================

pub struct SenderFsm<State> {
    state: PhantomData<State>,
    serial: Box<dyn SerialPort>,
    files: Vec<PathBuf>,
    current_file: Option<File>,
    filename: [u8; 11],
    filename_idx: usize,
    buffer: [u8; 128],
    checksum: u8,
    retransmit: bool,
    byte_delay: u8,
    debug: bool,
}

// ============================================================================
// Trait
// ============================================================================

pub trait SenderState: Send {
    fn step(self: Box<Self>) -> Result<Box<dyn SenderState>, SenderError>;
}

// ============================================================================
// Helper to transition states
// ============================================================================

impl<S> SenderFsm<S> {
    fn transition<T>(self) -> Box<SenderFsm<T>> {
        Box::new(SenderFsm {
            state: PhantomData,
            serial: self.serial,
            files: self.files,
            current_file: self.current_file,
            filename: self.filename,
            filename_idx: self.filename_idx,
            buffer: self.buffer,
            checksum: self.checksum,
            retransmit: self.retransmit,
            byte_delay: self.byte_delay,
            debug: self.debug,
        })
    }

    fn io_error(&self, e: std::io::Error) -> SenderError {
        let type_name = std::any::type_name::<S>();
        let state_name = type_name.split("::").last().unwrap_or(type_name);
        SenderError::Io(std::io::Error::new(
            e.kind(),
            format!("{} (in state: {})", e, state_name)
        ))
    }
}

// ============================================================================
// State Implementations
// ============================================================================

impl SenderState for SenderFsm<InitialHandshake> {
    fn step(self: Box<Self>) -> Result<Box<dyn SenderState>, SenderError> {
        let mut fsm = *self;
        fsm.serial.write_all(&[SENDER_READY])?;
        if fsm.debug { println!("Sent: 'R'"); }

        let mut buf = [0u8; 1];
        match fsm.serial.read_timeout(&mut buf, Duration::from_secs(5)) {
            Ok(_) if buf[0] == RECEIVER_READY => {
                if fsm.debug { println!("Received: 'S'"); }
                let next = fsm.transition::<SendGood>();
                Ok(next as Box<dyn SenderState>)
            }
            Err(e) if e.kind() != std::io::ErrorKind::TimedOut => Err(fsm.io_error(e)),
            _ => {
                println!("Receiver not ready");
                Ok(Box::new(fsm) as Box<dyn SenderState>)
            }
        }
    }
}

impl SenderState for SenderFsm<SendGood> {
    fn step(self: Box<Self>) -> Result<Box<dyn SenderState>, SenderError> {
        let mut fsm = *self;
        fsm.serial.write_all(&[GOOD])?;
        if fsm.debug { println!("Sent: 'G'"); }
        let next = fsm.transition::<RequestFilename>();
        Ok(next as Box<dyn SenderState>)
    }
}

impl SenderState for SenderFsm<RequestFilename> {
    fn step(self: Box<Self>) -> Result<Box<dyn SenderState>, SenderError> {
        let mut fsm = *self;
        if fsm.files.is_empty() {
            return Err(SenderError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No files to send",
            )));
        }

        fsm.serial.write_all(&[EOT])?;
        if fsm.debug { println!("Sent: EOT"); }

        let mut buf = [0u8; 1];
        match fsm.serial.read_timeout(&mut buf, Duration::from_secs(2)) {
            Ok(_) if buf[0] == BS => {
                if fsm.debug { println!("Received: BS"); }
                fsm.filename = prepare_filename(&fsm.files[0]);
                fsm.filename_idx = 0;
                let next = fsm.transition::<TransmitFilename>();
                Ok(next as Box<dyn SenderState>)
            }
            Err(e) => Err(fsm.io_error(e)),
            Ok(_) => {
                if fsm.debug { println!("Wrong character, waiting for BS..."); }
                Ok(Box::new(fsm) as Box<dyn SenderState>)
            }
        }
    }
}

impl SenderState for SenderFsm<TransmitFilename> {
    fn step(self: Box<Self>) -> Result<Box<dyn SenderState>, SenderError> {
        let mut fsm = *self;
        let ch = fsm.filename[fsm.filename_idx];
        fsm.serial.write_all(&[ch])?;
        if fsm.debug { print!("Sent: '{}'", ch as char); }

        let mut buf = [0u8; 1];
        match fsm.serial.read_timeout(&mut buf, Duration::from_secs(2)) {
            Ok(_) if buf[0] == ch => {
                if fsm.debug { println!(" - OK"); }
                fsm.filename_idx += 1;

                if fsm.filename_idx >= 11 {
                    let next = fsm.transition::<EndFilename>();
                    Ok(next as Box<dyn SenderState>)
                } else {
                    Ok(Box::new(fsm) as Box<dyn SenderState>)
                }
            }
            Ok(_) => {
                if fsm.debug { println!(" - Mismatch"); }
                fsm.filename_idx = 0;
                let next = fsm.transition::<RequestFilename>();
                Ok(next as Box<dyn SenderState>)
            }
            Err(e) => Err(fsm.io_error(e))
        }
    }
}

impl SenderState for SenderFsm<EndFilename> {
    fn step(self: Box<Self>) -> Result<Box<dyn SenderState>, SenderError> {
        let mut fsm = *self;
        fsm.serial.write_all(&[ENQ])?;
        if fsm.debug { println!("Sent: ENQ"); }

        let mut buf = [0u8; 1];
        match fsm.serial.read_timeout(&mut buf, Duration::from_secs(2)) {
            Ok(_) if buf[0] == TAB => {
                if fsm.debug { println!("Received: TAB"); }
                let path = fsm.files[0].clone();
                fsm.current_file = Some(File::open(&path)?);
                if fsm.debug { println!("Opened: {:?}", path); }
                let next = fsm.transition::<CheckMoreData>();
                Ok(next as Box<dyn SenderState>)
            }
            Err(e) => Err(fsm.io_error(e)),
            Ok(_) => {
                if fsm.debug { println!("Wrong character, restarting filename exchange..."); }
                fsm.filename_idx = 0;
                let next = fsm.transition::<RequestFilename>();
                Ok(next as Box<dyn SenderState>)
            }
        }
    }
}

impl SenderState for SenderFsm<CheckMoreData> {
    fn step(self: Box<Self>) -> Result<Box<dyn SenderState>, SenderError> {
        let mut fsm = *self;

        let is_eof = if fsm.retransmit {
            fsm.retransmit = false;
            if fsm.debug { println!("Retransmitting block"); }
            false
        } else {
            let bytes_read = if let Some(ref mut file) = fsm.current_file {
                file.read(&mut fsm.buffer)?
            } else {
                0
            };

            if bytes_read == 0 {
                true
            } else {
                for i in bytes_read..128 {
                    fsm.buffer[i] = 0x1A;
                }

                fsm.checksum = 0;
                for i in 0..128 {
                    fsm.checksum ^= fsm.buffer[i];
                }

                if fsm.debug { println!("Prepared block ({} bytes)", bytes_read); }
                false
            }
        };

        if is_eof {
            fsm.serial.write_all(&[ETX])?;
            if fsm.debug { println!("Sent: ETX"); }
            let next = fsm.transition::<EndFile>();
            Ok(next as Box<dyn SenderState>)
        } else {
            fsm.serial.write_all(&[STX])?;
            if fsm.debug { println!("Sent: STX"); }

            let mut buf = [0u8; 1];
            match fsm.serial.read_timeout(&mut buf, Duration::from_secs(2)) {
                Ok(_) if buf[0] == PROCEED => {
                    if fsm.debug { println!("Received: 'P'"); }
                    let next = fsm.transition::<TransmitBlock>();
                    Ok(next as Box<dyn SenderState>)
                }
                Err(e) => Err(fsm.io_error(e)),
                Ok(_) => {
                    if fsm.debug { println!("Wrong character, waiting for 'P'..."); }
                    Ok(Box::new(fsm) as Box<dyn SenderState>)
                }
            }
        }
    }
}

impl SenderState for SenderFsm<TransmitBlock> {
    fn step(self: Box<Self>) -> Result<Box<dyn SenderState>, SenderError> {
        let mut fsm = *self;

        // Send block byte-by-byte with optional delay to prevent receiver buffer overflow
        for i in 0..128 {
            fsm.serial.write_all(&[fsm.buffer[i]])?;
            if fsm.byte_delay > 0 {
                std::thread::sleep(Duration::from_millis(fsm.byte_delay as u64));
            }
        }

        if fsm.debug { println!("Sent: 128 byte block"); }

        let next = fsm.transition::<SendChecksum>();
        Ok(next as Box<dyn SenderState>)
    }
}

impl SenderState for SenderFsm<SendChecksum> {
    fn step(self: Box<Self>) -> Result<Box<dyn SenderState>, SenderError> {
        let mut fsm = *self;
        fsm.serial.write_all(&[fsm.checksum])?;
        if fsm.debug { println!("Sent: Checksum 0x{:02X}", fsm.checksum); }

        let mut buf = [0u8; 1];
        match fsm.serial.read_timeout(&mut buf, Duration::from_secs(2)) {
            Ok(_) if buf[0] == GOOD => {
                if fsm.debug { println!("Received: 'G'"); }
                fsm.retransmit = false;
                let next = fsm.transition::<CheckMoreData>();
                Ok(next as Box<dyn SenderState>)
            }
            Ok(_) if buf[0] == BAD => {
                if fsm.debug { println!("Received: 'B' (bad checksum)"); }
                fsm.retransmit = true;
                let next = fsm.transition::<CheckMoreData>();
                Ok(next as Box<dyn SenderState>)
            }
            Err(e) => Err(fsm.io_error(e)),
            Ok(_) => {
                if fsm.debug { println!("Wrong character, waiting for 'G' or 'B'..."); }
                Ok(Box::new(fsm) as Box<dyn SenderState>)
            }
        }
    }
}

impl SenderState for SenderFsm<EndFile> {
    fn step(self: Box<Self>) -> Result<Box<dyn SenderState>, SenderError> {
        let mut fsm = *self;
        fsm.current_file = None;
        fsm.files.remove(0);

        if fsm.files.is_empty() {
            fsm.serial.write_all(&[XOFF])?;
            if fsm.debug { println!("Sent: XOFF"); }
            Err(SenderError::TransferComplete)
        } else {
            if fsm.debug { println!("{} files remaining", fsm.files.len()); }
            let next = fsm.transition::<RequestFilename>();
            Ok(next as Box<dyn SenderState>)
        }
    }
}

// ============================================================================
// Constructor & Runner
// ============================================================================

impl SenderFsm<InitialHandshake> {
    pub fn new(serial: Box<dyn SerialPort>, files: Vec<PathBuf>, byte_delay: u8, debug: bool) -> Box<dyn SenderState> {
        Box::new(SenderFsm {
            state: PhantomData::<InitialHandshake>,
            serial,
            files,
            current_file: None,
            filename: [b' '; 11],
            filename_idx: 0,
            buffer: [0; 128],
            checksum: 0,
            retransmit: false,
            byte_delay,
            debug,
        })
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

fn prepare_filename(path: &PathBuf) -> [u8; 11] {
    let mut result = [b' '; 11];

    if let Some(filename) = path.file_name() {
        if let Some(s) = filename.to_str() {
            let upper = s.to_uppercase();
            let parts: Vec<&str> = upper.splitn(2, '.').collect();

            for (i, ch) in parts.get(0).unwrap_or(&"").chars().take(8).enumerate() {
                result[i] = ch as u8;
            }

            if let Some(ext) = parts.get(1) {
                let ext_first = ext.split('.').next().unwrap_or("");
                for (i, ch) in ext_first.chars().take(3).enumerate() {
                    result[8 + i] = ch as u8;
                }
            }
        }
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

    fn run_sender(mut fsm: Box<dyn SenderState>) -> Result<(), SenderError> {
        loop {
            match fsm.step() {
                Ok(next) => fsm = next,
                Err(SenderError::TransferComplete) => return Ok(()),
                Err(e) => return Err(e),
            }
        }
    }

    #[test]
    fn test_prepare_filename() {
        let path = PathBuf::from("test.txt");
        let result = prepare_filename(&path);
        assert_eq!(&result, b"TEST    TXT");

        let path = PathBuf::from("verylongname.txt");
        let result = prepare_filename(&path);
        assert_eq!(&result, b"VERYLONGTXT");

        let path = PathBuf::from("readme");
        let result = prepare_filename(&path);
        assert_eq!(&result, b"README     ");

        let path = PathBuf::from("file.html");
        let result = prepare_filename(&path);
        assert_eq!(&result, b"FILE    HTM");

        let path = PathBuf::from("/path/to/file.txt");
        let result = prepare_filename(&path);
        assert_eq!(&result, b"FILE    TXT");

        let path = PathBuf::from("filename.ext");
        let result = prepare_filename(&path);
        assert_eq!(&result, b"FILENAMEEXT");

        let path = PathBuf::from("ab.c");
        let result = prepare_filename(&path);
        assert_eq!(&result, b"AB      C  ");

        let path = PathBuf::from("file.tar.gz");
        let result = prepare_filename(&path);
        assert_eq!(&result, b"FILE    TAR");
        
        let path = PathBuf::from("file.c.gz");
        let result = prepare_filename(&path);
        assert_eq!(&result, b"FILE    C  ");

        let path = PathBuf::from("MyFile.TxT");
        let result = prepare_filename(&path);
        assert_eq!(&result, b"MYFILE  TXT");

        let path = PathBuf::from("a");
        let result = prepare_filename(&path);
        assert_eq!(&result, b"A          ");
    }

    #[test]
    fn test_sender_full_transfer() {
        let test_file = std::env::temp_dir().join("sender_test_small.txt");
        std::fs::write(&test_file, b"Test data").unwrap();

        let mut responses = vec![
            Some(RECEIVER_READY),
            Some(BS),
        ];

        for ch in b"SENDER_TTXT" {
            responses.push(Some(*ch));
        }

        responses.push(Some(TAB));
        responses.push(Some(PROCEED));
        responses.push(Some(GOOD));

        let mut expected_writes = vec![
            SENDER_READY,
            GOOD,
            EOT,
        ];

        expected_writes.extend_from_slice(b"SENDER_TTXT");

        expected_writes.push(ENQ);

        expected_writes.push(STX);
        let mut block = b"Test data".to_vec();
        block.resize(128, 0x1A);
        let checksum: u8 = block.iter().fold(0u8, |acc, &b| acc ^ b);
        expected_writes.extend_from_slice(&block);
        expected_writes.push(checksum);

        expected_writes.push(ETX);
        expected_writes.push(XOFF);

        let mock_serial = Box::new(MockSerialPort::new(responses, expected_writes));
        let files = vec![test_file.clone()];

        let fsm = SenderFsm::new(mock_serial, files, 0, true);

        match run_sender(fsm) {
            Ok(()) => {},
            Err(SenderError::TransferComplete) => {},
            Err(e) => panic!("Transfer failed: {:?}", e),
        }

        std::fs::remove_file(&test_file).ok();
    }

    #[test]
    fn test_sender_handshake_retry() {
        let responses = vec![None, Some(RECEIVER_READY)];

        let expected_writes = vec![
            SENDER_READY,
            SENDER_READY,
            GOOD,
        ];

        let mock_serial = Box::new(MockSerialPort::new(responses, expected_writes));
        let files = vec![PathBuf::from("dummy.txt")];

        let mut fsm = SenderFsm::new(mock_serial, files, 0, true);

        for _ in 0..3 {
            fsm = fsm.step().expect("Should succeed");
        }
    }

    #[test]
    fn test_sender_filename_mismatch() {
        let test_file = std::env::temp_dir().join("mismatch.txt");
        std::fs::write(&test_file, b"test").unwrap();

        let mut responses = vec![
            Some(RECEIVER_READY),
            Some(BS),
        ];

        for ch in b"MISK" {
            responses.push(Some(*ch));
        }

        responses.push(Some(BS));

        for ch in b"MISMATCHTXT" {
            responses.push(Some(*ch));
        }

        responses.push(Some(TAB));

        responses.push(Some(PROCEED));
        responses.push(Some(GOOD));

        let mut expected_writes = vec![
            SENDER_READY,
            GOOD,
            EOT,
        ];

        expected_writes.extend_from_slice(b"MISM");

        expected_writes.push(0x04);

        expected_writes.extend_from_slice(b"MISMATCHTXT");

        expected_writes.push(ENQ);

        expected_writes.push(STX);
        let mut block = b"test".to_vec();
        block.resize(128, 0x1A);
        let checksum: u8 = block.iter().fold(0u8, |acc, &b| acc ^ b);
        expected_writes.extend_from_slice(&block);
        expected_writes.push(checksum);

        expected_writes.push(ETX);
        expected_writes.push(XOFF);

        let mock_serial = Box::new(MockSerialPort::new(responses, expected_writes));
        let files = vec![test_file.clone()];

        let fsm = SenderFsm::new(mock_serial, files, 0, true);

        match run_sender(fsm) {
            Ok(()) => {},
            Err(SenderError::TransferComplete) => {},
            Err(e) => panic!("Transfer failed: {:?}", e),
        }

        std::fs::remove_file(&test_file).ok();
    }

    #[test]
    fn test_sender_bad_checksum_retry() {
        let test_file = std::env::temp_dir().join("badcheck.txt");
        std::fs::write(&test_file, b"retry").unwrap();

        let mut responses = vec![
            Some(RECEIVER_READY),
            Some(BS),
        ];

        for ch in b"BADCHECKTXT" {
            responses.push(Some(*ch));
        }

        responses.push(Some(TAB));

        responses.push(Some(PROCEED));
        responses.push(Some(BAD));

        responses.push(Some(PROCEED));
        responses.push(Some(GOOD));

        let mut expected_writes = vec![
            SENDER_READY,
            GOOD,
            EOT,
        ];

        expected_writes.extend_from_slice(b"BADCHECKTXT");

        expected_writes.push(ENQ);

        expected_writes.push(STX);
        let mut block = b"retry".to_vec();
        block.resize(128, 0x1A);
        let checksum: u8 = block.iter().fold(0u8, |acc, &b| acc ^ b);
        expected_writes.extend_from_slice(&block);
        expected_writes.push(checksum);

        expected_writes.push(STX);
        expected_writes.extend_from_slice(&block);
        expected_writes.push(checksum);

        expected_writes.push(ETX);
        expected_writes.push(XOFF);

        let mock_serial = Box::new(MockSerialPort::new(responses, expected_writes));
        let files = vec![test_file.clone()];

        let fsm = SenderFsm::new(mock_serial, files, 0, true);

        match run_sender(fsm) {
            Ok(()) => {},
            Err(SenderError::TransferComplete) => {},
            Err(e) => panic!("Transfer failed: {:?}", e),
        }

        std::fs::remove_file(&test_file).ok();
    }

    #[test]
    fn test_sender_multiple_blocks() {
        let test_file = std::env::temp_dir().join("multiblock.txt");

        let mut content = Vec::new();
        for i in 0..300 {
            content.push((i % 256) as u8);
        }
        std::fs::write(&test_file, &content).unwrap();

        let mut responses = vec![
            Some(RECEIVER_READY),
            Some(BS),
        ];

        for ch in b"MULTIBLOTXT" {
            responses.push(Some(*ch));
        }

        responses.push(Some(TAB));

        for _i in 0..3 {
            responses.push(Some(PROCEED));
            responses.push(Some(GOOD));
        }

        let mut expected_writes = vec![
            SENDER_READY,
            GOOD,
            EOT,
        ];

        expected_writes.extend_from_slice(b"MULTIBLOTXT");

        expected_writes.push(ENQ);

        for block_num in 0..3 {
            expected_writes.push(STX);

            let mut block = Vec::new();
            let start = block_num * 128;
            let end = std::cmp::min(start + 128, 300);
            for i in start..end {
                block.push((i % 256) as u8);
            }
            block.resize(128, 0x1A);

            let checksum: u8 = block.iter().fold(0u8, |acc, &b| acc ^ b);
            expected_writes.extend_from_slice(&block);
            expected_writes.push(checksum);
        }

        expected_writes.push(ETX);
        expected_writes.push(XOFF);

        let mock_serial = Box::new(MockSerialPort::new(responses, expected_writes));
        let files = vec![test_file.clone()];

        let fsm = SenderFsm::new(mock_serial, files, 0, true);

        match run_sender(fsm) {
            Ok(()) => {},
            Err(SenderError::TransferComplete) => {},
            Err(e) => panic!("Transfer failed: {:?}", e),
        }

        std::fs::remove_file(&test_file).ok();
    }

    #[test]
    fn test_sender_multiple_files() {
        let test_file1 = std::env::temp_dir().join("first.txt");
        let test_file2 = std::env::temp_dir().join("second.txt");
        std::fs::write(&test_file1, b"first").unwrap();
        std::fs::write(&test_file2, b"second").unwrap();

        let mut responses = vec![
            Some(RECEIVER_READY),
        ];

        responses.push(Some(BS));
        for ch in b"FIRST   TXT" {
            responses.push(Some(*ch));
        }
        responses.push(Some(TAB));
        responses.push(Some(PROCEED));
        responses.push(Some(GOOD));

        responses.push(Some(BS));
        for ch in b"SECOND  TXT" {
            responses.push(Some(*ch));
        }
        responses.push(Some(TAB));
        responses.push(Some(PROCEED));
        responses.push(Some(GOOD));

        let mut expected_writes = vec![
            SENDER_READY,
            GOOD,
            EOT,
        ];

        expected_writes.extend_from_slice(b"FIRST   TXT");
        expected_writes.push(ENQ);
        expected_writes.push(STX);

        let mut block1 = b"first".to_vec();
        block1.resize(128, 0x1A);
        let checksum1: u8 = block1.iter().fold(0u8, |acc, &b| acc ^ b);
        expected_writes.extend_from_slice(&block1);
        expected_writes.push(checksum1);
        expected_writes.push(ETX);

        expected_writes.push(0x04);
        expected_writes.extend_from_slice(b"SECOND  TXT");
        expected_writes.push(ENQ);
        expected_writes.push(STX);

        let mut block2 = b"second".to_vec();
        block2.resize(128, 0x1A);
        let checksum2: u8 = block2.iter().fold(0u8, |acc, &b| acc ^ b);
        expected_writes.extend_from_slice(&block2);
        expected_writes.push(checksum2);
        expected_writes.push(ETX);

        expected_writes.push(XOFF);

        let mock_serial = Box::new(MockSerialPort::new(responses, expected_writes));
        let files = vec![test_file1.clone(), test_file2.clone()];

        let fsm = SenderFsm::new(mock_serial, files, 0, true);

        match run_sender(fsm) {
            Ok(()) => {},
            Err(SenderError::TransferComplete) => {},
            Err(e) => panic!("Transfer failed: {:?}", e),
        }

        std::fs::remove_file(&test_file1).ok();
        std::fs::remove_file(&test_file2).ok();
    }
}
