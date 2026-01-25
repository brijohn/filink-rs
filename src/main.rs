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

// Filink protocol implementation
mod protocol;
mod sender;
mod receiver;
mod serial;

use clap::{Parser, Subcommand};
use serialport::{DataBits, Parity, StopBits};
use std::path::PathBuf;
use serial::RealSerialPort;

#[derive(Parser)]
#[command(name = "filink")]
#[command(about = "Filink protocol implementation for RS-232 file transfer", long_about = None)]
#[command(disable_help_subcommand = true)]
struct Cli {
    /// Serial port to use (e.g., /dev/ttyUSB0 or COM1)
    #[arg(short, long)]
    port: String,

    /// Baud rate
    #[arg(short, long, default_value = "9600")]
    baud: u32,

    /// Data bits (5, 6, 7, or 8)
    #[arg(long, default_value = "8", value_name="BITS")]
    data_bits: u8,

    /// Parity (none, odd, or even)
    #[arg(long, default_value = "none")]
    parity: String,

    /// Stop bits (1 or 2)
    #[arg(long, default_value = "1", value_name="BITS")]
    stop_bits: u8,

    /// Delay in milliseconds between sending each byte of a data block
    #[arg(long, default_value = "0", value_name = "MS")]
    byte_delay: u8,

    /// Enable debug output
    #[arg(long)]
    debug: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Send a file using the filink protocol
    Send {
        /// File to send
        file: PathBuf,
    },
    /// Receive files using the filink protocol
    Receive {
        /// Directory to save received files
        #[arg(short, long, default_value = ".")]
        output_dir: PathBuf,
    },
}

fn parse_data_bits(bits: u8) -> Result<DataBits, String> {
    match bits {
        5 => Ok(DataBits::Five),
        6 => Ok(DataBits::Six),
        7 => Ok(DataBits::Seven),
        8 => Ok(DataBits::Eight),
        _ => Err(format!("Invalid data bits: {}. Must be 5, 6, 7, or 8", bits)),
    }
}

fn parse_parity(parity: &str) -> Result<Parity, String> {
    match parity.to_lowercase().as_str() {
        "none" => Ok(Parity::None),
        "odd" => Ok(Parity::Odd),
        "even" => Ok(Parity::Even),
        _ => Err(format!("Invalid parity: {}. Must be 'none', 'odd', or 'even'", parity)),
    }
}

fn parse_stop_bits(bits: u8) -> Result<StopBits, String> {
    match bits {
        1 => Ok(StopBits::One),
        2 => Ok(StopBits::Two),
        _ => Err(format!("Invalid stop bits: {}. Must be 1 or 2", bits)),
    }
}

fn main() {
    let cli = Cli::parse();

    let data_bits = match parse_data_bits(cli.data_bits) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    let parity = match parse_parity(&cli.parity) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    let stop_bits = match parse_stop_bits(cli.stop_bits) {
        Ok(sb) => sb,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    println!("Opening serial port: {}", cli.port);
    println!("Settings: {} baud, {:?}, {:?}, {:?}", cli.baud, data_bits, parity, stop_bits);

    let serial_port = match RealSerialPort::open(&cli.port, cli.baud, data_bits, parity, stop_bits) {
        Ok(port) => port,
        Err(e) => {
            eprintln!("Failed to open serial port: {}", e);
            std::process::exit(1);
        }
    };

    match cli.command {
        Commands::Send { file } => {
            println!("\nSending file: {}", file.display());
            if let Err(e) = send_file(serial_port, file, cli.byte_delay, cli.debug) {
                eprintln!("Send failed: {}", e);
                std::process::exit(1);
            }
            println!("\nFile sent successfully!");
        }
        Commands::Receive { output_dir } => {
            println!("\nReceiving files to: {}", output_dir.display());
            if let Err(e) = receive_files(serial_port, output_dir, cli.debug) {
                eprintln!("Receive failed: {}", e);
                std::process::exit(1);
            }
            println!("\nFiles received successfully!");
        }
    }
}

fn send_file(serial_port: RealSerialPort, file: PathBuf, byte_delay: u8, debug: bool) -> Result<(), sender::SenderError> {
    use sender::{SenderFsm, InitialHandshake};

    if !file.exists() {
        return Err(sender::SenderError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("File not found: {}", file.display()),
        )));
    }

    let mut state = SenderFsm::<InitialHandshake>::new(Box::new(serial_port), vec![file], byte_delay, debug);

    loop {
        match state.step() {
            Ok(next_state) => {
                state = next_state;
            }
            Err(sender::SenderError::TransferComplete) => {
                return Ok(());
            }
            Err(e) => {
                return Err(e);
            }
        }
    }
}

fn receive_files(serial_port: RealSerialPort, output_dir: PathBuf, debug: bool) -> Result<(), receiver::ReceiverError> {
    use receiver::{ReceiverFsm, InitialHandshake};

    if !output_dir.exists() {
        return Err(receiver::ReceiverError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Output directory not found: {}", output_dir.display()),
        )));
    }

    let mut state = ReceiverFsm::<InitialHandshake>::new(Box::new(serial_port), output_dir, debug);

    loop {
        match state.step() {
            Ok(next_state) => {
                state = next_state;
            }
            Err(receiver::ReceiverError::TransferComplete) => {
                return Ok(());
            }
            Err(e) => {
                return Err(e);
            }
        }
    }
}
