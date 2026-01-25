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

//! FILINK protocol constants

/// Start of text - begins transmission of a 128-byte data block
pub const STX: u8 = 0x02;

/// End of text - signals end of current file
pub const ETX: u8 = 0x03;

/// End of transmission - sender requests to send filename
pub const EOT: u8 = 0x04;

/// Enquiry - sender confirms filename transmission complete
pub const ENQ: u8 = 0x05;

/// Backspace - receiver ready to receive filename
pub const BS: u8 = 0x08;

/// Tab - receiver acknowledges filename and ready for data
pub const TAB: u8 = 0x09;

/// Transmit off - sender signals no more files, end session
pub const XOFF: u8 = 0x13;

/// Good - sender confirms handshake complete, or receiver confirms block checksum is valid
pub const GOOD: u8 = b'G';

/// Bad - receiver reports checksum failure, retransmit block
pub const BAD: u8 = b'B';

/// NAK - receiver received unexpected character
pub const NAK: u8 = b'N';

/// Proceed - receiver ready to receive block data
pub const PROCEED: u8 = b'P';

/// Sender ready - sender initiates handshake
pub const SENDER_READY: u8 = b'R';

/// Receiver ready - receiver confirms ready to receive
pub const RECEIVER_READY: u8 = b'S';

/// Error - abort due to protocol violation or unexpected character
pub const ERROR: u8 = b'X';
