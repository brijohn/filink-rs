# FILINK Protocol Documentation

## Overview

FILINK is a simple block-based file transfer protocol developed for Epson PX-8 (Geneva) portable computers. The protocol was built into the PX-8's UTY-ROM, providing a reliable method for transferring files between the PX-8 and other computers via RS-232C serial connections without requiring additional software on the PX-8.

### Key Characteristics

- **Block Size**: 128 bytes per block (matching CP/M sector size)
- **Checksum**: Simple XOR checksum of all bytes in block
- **Handshaking**: Character-based acknowledgments with timeout
- **Bidirectional**: Supports both send and receive operations
- **Filename Format**: 8.3 DOS-style naming (8 character name + 3 character extension)
- **Multiple Files**: Can transfer multiple files in a single session
- **Error Recovery**: Automatic block retransmission on checksum failure

## Serial Port Configuration

Typical RS-232C settings for FILINK:

- **Baud Rate**: 1200-9600 bps (commonly 4800 or 9600)
- **Data Bits**: 8
- **Parity**: None
- **Stop Bits**: 1-2 (depends on baud rate)
- **Flow Control**: None (protocol handles flow control)

## Control Characters

FILINK uses standard ASCII control characters and letter characters for protocol control:

| Character | ASCII Value | Hex  | Purpose |
|-----------|-------------|------|---------|
| STX       | 2           | 0x02 | Start of data block |
| ETX       | 3           | 0x03 | End of file |
| EOT       | 4           | 0x04 | Ready for filename |
| ENQ       | 5           | 0x05 | End of filename transmission |
| BS        | 8           | 0x08 | Acknowledge filename request |
| TAB       | 9           | 0x09 | Ready to receive file data |
| XOFF      | 19          | 0x13 | End of all transfers (session complete) |
| 'R'       | 82          | 0x52 | Sender ready |
| 'S'       | 83          | 0x53 | Receiver ready |
| 'G'       | 71          | 0x47 | Good / Proceed |
| 'B'       | 66          | 0x42 | Bad checksum - retransmit block |
| 'P'       | 80          | 0x50 | Proceed with block transmission |
| 'N'       | 78          | 0x4E | Negative acknowledgment |
| 'X'       | 88          | 0x58 | Error / Reject |

## Protocol State Machines

### Sender State Machine

The sender progresses through the following states when transmitting a file:

#### State 1: Initial Handshake

- **Send**: 'R' (Ready)
- **Wait for**: 'S' (Receiver ready)
- **Timeout**: 5.0 seconds
- **On timeout**: Display "Receiver not ready"
- **Next state**: State 2

#### State 2: Send Good Signal

- **Send**: 'G' (Good to proceed)
- **Next state**: State 3

#### State 3: Request Filename Send

- **Send**: EOT (4)
- **Wait for**: BS (8)
- **Timeout**: 2.0 seconds
- **On timeout**: Abort with "Receiver not responding"
- **Next state**: State 4

#### State 4: Transmit Filename

- **Send**: One character of filename (11 characters total)
- **Wait for**: Echo of same character
- **Format**: 8 characters name + 3 characters extension (uppercase, space-padded)
- **On mismatch**: Return to State 3
- **On timeout**: Abort with "Receiver not responding"
- **Next state**: State 5 (after all 11 characters sent)

#### State 5: End Filename

- **Send**: ENQ (5)
- **Wait for**: TAB (9)
- **Timeout**: 2.0 seconds
- **On timeout**: Abort with "Receiver not responding"
- **On wrong character**: Return to State 3
- **Next state**: State 6

#### State 6: Check for More Data in Current File

- **If EOF**: Send ETX (3), go to State 9
- **If more data**: Send STX (2), wait for 'P', go to State 7
- **Timeout**: 2.0 seconds
- **On timeout**: Abort with "Receiver not responding"

#### State 7: Transmit Block

- **Send**: 128 bytes of data
- **Action**: Calculate XOR checksum of all bytes
- **Padding**: Blocks shorter than 128 bytes are padded with EOF (0x1A)
- **Next state**: State 8 (after all 128 bytes sent)

#### State 8: Send Checksum and Wait

- **Send**: XOR checksum byte
- **Wait for**: 'G' (good) or 'B' (bad)
- **On 'B'**: Return to State 6 (retransmit same block)
- **On 'G'**: Return to State 6 (get next block)
- **Timeout**: 2.0 seconds
- **On timeout**: Abort with "Receiver not responding"

#### State 9: End of Current File

- **Send**: ETX (3) (already sent in State 6)
- **Action**: Close current file, decrement file counter
- **If more files remain**: Return to State 3 (send next file)
- **If no more files**: Send XOFF (19) and exit (session complete)

### Receiver State Machine

The receiver progresses through the following states when receiving a file:

#### State 1: Initial Handshake

- **Wait for**: 'R' (Sender ready)
- **Send**: 'S' (Ready to receive)
- **Timeout**: 5.0 seconds
- **On timeout**: Display "Sender not ready"
- **Next state**: State 2

#### State 2: Wait for Good Signal

- **Wait for**: 'G'
- **Timeout**: 2.0 seconds
- **On timeout**: Abort with "Sender not responding"
- **Next state**: State 3

#### State 3: Wait for File or Session End

- **Wait for**: EOT (4) or XOFF (19)
- **On XOFF**: Exit (all transfers complete)
- **On EOT**: Send BS (8), go to State 4
- **On other**: Send 'X', stay in State 3
- **Timeout**: 2.0 seconds
- **On timeout**: Abort with "Sender not responding"

#### State 4: Receive Filename

- **Receive**: 11 characters (8 name + 3 extension)
- **Send**: Echo each character back
- **Action**: Convert to lowercase (Unix implementation)
- **On invalid character**: Send 'X', return to State 3
- **Timeout**: 2.0 seconds per character
- **On timeout**: Abort with "Sender not responding"
- **Next state**: State 5 (after 11 characters received)

#### State 5: End Filename Reception

- **Wait for**: ENQ (5)
- **On ENQ**: Open file, send TAB (9), go to State 6
- **On file error**: Send 'X', return to State 3
- **On other**: Send 'X', return to State 3
- **Timeout**: 2.0 seconds
- **On timeout**: Abort with "Sender not responding"

#### State 6: Wait for Block or EOF

- **Wait for**: STX (2) or ETX (3)
- **On STX**: Send 'P', go to State 7
- **On ETX**: Close file, return to State 3
- **On other**: Send 'N', stay in State 6
- **Timeout**: 2.0 seconds
- **On timeout**: Abort with "Sender not responding"

#### State 7: Receive Data Block

- **Receive**: 128 bytes
- **Action**: Calculate XOR checksum as bytes arrive
- **Next state**: State 8 (after 128 bytes received)
- **Timeout**: 2.0 seconds per byte
- **On timeout**: Abort with "Sender not responding"

#### State 8: Verify Checksum

- **Receive**: Checksum byte
- **Action**: Compare with calculated checksum
- **On match**: Write block to disk, send 'G', return to State 6
- **On mismatch**: Send 'B', return to State 6 (sender will retransmit)
- **Timeout**: 2.0 seconds
- **On timeout**: Abort with "Sender not responding"

## Complete Transfer Sequence

Here's a complete example of transferring files. This shows the initial handshake, one complete file transfer, and how multiple files are handled:

```
SENDER                                  RECEIVER
------                                  --------
'R' ---------------------------------->
(Sender ready)
                                        'S'
                            <----------------------------------
                                        (Receiver ready)
'G' ---------------------------------->
(Good to proceed)

EOT ---------------------------------->
(First filename next)
                                        BS (8)
                            <----------------------------------
                                        (Ready for filename)
'E' ---------------------------------->
                                        'E'
                            <----------------------------------
'X' ---------------------------------->
                                        'X'
                            <----------------------------------
'A' ---------------------------------->
                                        'A'
                            <----------------------------------
'M' ---------------------------------->
                                        'M'
                            <----------------------------------
'P' ---------------------------------->
                                        'P'
                            <----------------------------------
'L' ---------------------------------->
                                        'L'
                            <----------------------------------
'E' ---------------------------------->
                                        'E'
                            <----------------------------------
' ' ---------------------------------->
(space padding for 8 chars)
                                        ' '
                            <----------------------------------
' ' ---------------------------------->
                                        ' '
                            <----------------------------------
'T' ---------------------------------->
                                        'T'
                            <----------------------------------
'X' ---------------------------------->
                                        'X'
                            <----------------------------------
'T' ---------------------------------->
                                        'T'
                            <----------------------------------
ENQ ---------------------------------->
(End of filename)
                                        TAB (9)
                            <----------------------------------
                                        (Ready for file data)

STX ---------------------------------->
(Start block)
                                        'P'
                            <----------------------------------
                                        (Proceed)
[128 bytes of data] ------------------>

[XOR checksum byte] ------------------>
                                        'G'
                            <----------------------------------
                                        (Good checksum)

[Repeat STX through 'G' for each 128-byte block]

ETX ---------------------------------->
(End of first file)

--- If more files to send, repeat from EOT ---

EOT ---------------------------------->
(Next filename)
                                        BS (8)
                            <----------------------------------
[... filename exchange ...]
[... file data blocks ...]
ETX ---------------------------------->
(End of second file)

--- When all files sent ---

XOFF --------------------------------->
(All transfers complete)
```

## Checksum Algorithm

The checksum is a simple XOR of all bytes in the 128-byte block:

```c
int checksum = 0;
for (int i = 0; i < 128; i++) {
    checksum ^= block[i];
}
```

This provides basic error detection but does not correct errors. On checksum failure, the entire block must be retransmitted.

## Filename Handling

### Protocol Specification

The protocol transmits filenames as exactly **11 characters** (8 for name + 3 for extension):

- Each character sent individually and echoed back
- Characters transmitted as uppercase (CP/M convention)
- Spaces used for padding
- No period/dot transmitted (implied between character 8 and 9)

## Error Handling

### Timeout Errors

- All operations have timeouts (typically 2-5 seconds)
- On timeout, display appropriate error message and abort transfer
- Messages include "Sender not responding" or "Receiver not responding"

### Checksum Errors

- Receiver sends 'B' (bad) to request retransmission
- Sender retransmits the same 128-byte block
- No limit on retries in basic implementation

### Protocol Errors

- Invalid characters during filename: Send 'X', restart filename exchange
- Unexpected control character: Send 'N', request retransmission

## Compatibility

### Known Implementations

- **FILINK.COM**: Built into PX-8 UTY-ROM
- **QXFILINK.COM**: QX-10/16 version
- **FILINK.EXE**: DOS version (from Epson FTP site)
- **filink.c**: Unix/Linux implementation by Frank Cringle (1994)
- **EPSLINK.ASM**: CP/M implementation by Jim Dorsey (1984)

## References

### Source Code

- `filink.c` - Unix implementation by Frank Cringle (August 1994)
- `EPSLINK.ASM` - CP/M 8080 assembly by Jim Dorsey (December 1984)

## Appendix: State Transition Table

### Sender States

| State | Send | Receive | Next State | On Error |
|-------|------|---------|------------|----------|
| 1 | 'R' | 'S' | 2 | Timeout: Retry |
| 2 | 'G' | - | 3 | - |
| 3 | EOT | BS | 4 | Timeout: Abort |
| 4 | Filename[n] | Echo | 4 or 5 | Mismatch: State 3 |
| 5 | ENQ | TAB | 6 | Timeout: Abort, Wrong char: State 3 |
| 6 | STX or ETX | 'P' or - | 7 or 9 | Timeout: Abort |
| 7 | Data[128] | - | 8 | - |
| 8 | Checksum | 'G' or 'B' | 6 | Timeout: Abort |
| 9 | ETX (sent in 6), then XOFF or - | - | 3 (more files) or Exit (done) | - |

### Receiver States

| State | Receive | Send | Next State | On Error |
|-------|---------|------|------------|----------|
| 1 | 'R' | 'S' | 2 | Timeout: Retry |
| 2 | 'G' | - | 3 | Timeout: Abort |
| 3 | EOT or XOFF | BS or - | 4 or Exit | Invalid: Send 'X' |
| 4 | Filename[n] | Echo | 4 or 5 | Invalid: State 3 |
| 5 | ENQ | TAB | 6 | Error: Send 'X', State 3 |
| 6 | STX or ETX | 'P' or - | 7 or 3 | Invalid: Send 'N' |
| 7 | Data[128] | - | 8 | Timeout: Abort |
| 8 | Checksum | 'G' or 'B' | 6 | Timeout: Abort |

---

*This documentation was reverse-engineered from the filink.c and EPSLINK.ASM source code implementations.*
