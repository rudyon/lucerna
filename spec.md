# lucerna computer specification

## components

- **lux8** processor
- Clock at 4 MHz to drive the lux8 processor
- ROM chip capable of storing 4 KiBs
- RAM chip capable of storing 60 KiBs

## memory bus

Single shared memory bus, with the ROM connected to the lower address range and the RAM connected to the higher range.

## memory layout

- `0x0000` to `0x0FFF` is assigned to the ROM.
- `0x1000` to `0xFFFF` is assigned to the RAM.

## devices

There is a dedicated bus for device I/O. Which can be accessed using `DVI` and `DVO` instructions.

### device layout

The device interface uses an 8-bit device address and an 8-bit port address. Each device address identifies a device, and each port address selects one of its 256 byte-wide ports. Device I/O is separate from memory addressing. The device addresses are as such:

| address       | device            |
| ------------- | ----------------- |
| `0x00`        | unused / reserved |
| `0x01`        | console           |
| `0x02` onward | unused / reserved |

#### console

| address | port   | description                                                    |
| ------- | ------ | -------------------------------------------------------------- |
| `0x00`  | input  | retrieves one incoming byte when read                          |
| `0x01`  | output | sends one outgoing byte when written                           |
| `0x02`  | status | bit 0 is `1` when an input byte is waiting; oterwise it is `0` |

- Reading the input port when no byte is available returns `0x00` and does not block. Programs should check the status port before reading if they need to distinguish an empty buffer from an actual zero byte.

### additional details

Reads from an unassigned device return `0x00`. Writes to an unassigned device are ignored.

## lux8 processor specification

- 8-bit data path.
- 16-bit addresses.
- Multi-byte values in memory use little-endian byte order.
- The stack starts at the upper end of memory and grows towards the lower addresses.

### registers

- **r0-r7**: 8-bit general purpose registers
- **rip**: 16-bit instruction pointer
- **rsp**: 16-bit stack pointer
- **rir**: 8-bit internal instruction register
- flags:
    - **carry flag**
    - **zero flag**

#### additional details

- **rip** points to the next instruction and increments after the current instruction is finished.
- **rsp** points to the top of the stack and decrements before `PSH` and increments after `POP`.
- **rip** initilizes to `0x0000`.
- **rsp** initilizes to `0xFFFF`.

### instructions

#### encoding

Instructions start with a 8-bit control byte, The upper 4 bits specify the opcode and the lower 4 bits specify the operand format. The decoder uses the operand format to determine how many additonal bytes to fetch and how to interpret them.

#### opcodes

| value | name | operands                   | description           |
| ----- | ---- | -------------------------- | --------------------- |
| `0x0` | NOP  | none                       | no operation          |
| `0x1` | HLT  | none                       | halt                  |
| `0x2` | JMP  | address                    | jump                  |
| `0x3` | JMZ  | address                    | jump if zero          |
| `0x4` | LDR  | register, address/register | load register         |
| `0x5` | STR  | address/register, register | store register        |
| `0x6` | WRT  | address/register, value    | write immediate       |
| `0x7` | POP  | register                   | pop value from stack  |
| `0x8` | PSH  | address/register           | push value onto stack |
| `0x9` | ADD  | register, address/register | add                   |
| `0xA` | MUL  | register, address/register | multiply              |
| `0xB` | SUB  | register, address/register | subtract              |
| `0xC` | DVI  | register, device, port     | device in             |
| `0xD` | DVO  | register, device, port     | device out            |

#### operand formats

| value | format                 | encoding after control byte                            | operand bytes |
| ----- | ---------------------- | ------------------------------------------------------ | ------------- |
| `0x0` | none                   | nothing                                                | 0             |
| `0x1` | register               | register index                                         | 1             |
| `0x2` | register, register     | register index, register index                         | 2             |
| `0x3` | register, address      | register index, address low byte, address high byte    | 3             |
| `0x4` | register, value        | register index, immediate value                        | 2             |
| `0x5` | address                | address low byte, address high byte                    | 2             |
| `0x6` | address, register      | address low byte, address high byte, register index    | 3             |
| `0x7` | address, address       | first address, then second address; each little-endian | 4             |
| `0x8` | address, value         | address low byte, address high byte, immediate value   | 3             |
| `0x9` | register, device, port | register index, device address byte, device port byte  | 3             |

#### additional details

For arithmetic operations:

- `ADD dst, src` -> `dst = dst + src`
- `MUL dst, src` -> `dst = dst * src`
- `SUB dst, src` -> `dst = dst - src`

The `src` can be a register or a byte read from memory. The result is stored in `dst`, which is always a register. The flags work as follows:

| flag  | `ADD`           | `MUL`               | `SUB`          |
| ----- | --------------- | ------------------- | -------------- |
| zero  | result is zero  | result is zero      | result is zero |
| carry | sum exceeds 255 | product exceeds 255 | borrow occurs  |
