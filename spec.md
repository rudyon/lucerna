# stolas computer specification

## components

- **lux8** processor
- Clock at 4 MHz to drive the **lux8** processor
- ROM chip capable of storing 4 KiBs
- RAM chip capable of storing 60 KiBs

## memory bus

Single shared memory bus, with the ROM connected to the lower address range and the RAM connected to the higher range.

### memory layout

- `0x0000` to `0x0FFF` is assigned to the ROM.
- `0x1000` to `0xFFFF` is assigned to the RAM.

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

Instructions start with a 8-bit control byte, The upper 4 bits specify the opcode and the lower 4 bits specify the operand format. The decoder uss the operand format to determine how many additonal bytes to fetch and how to interpret them.

#### opcodes

| value | name | operands                   | description           |
| ----- | ---- | -------------------------- | --------------------- |
| `0x0` | NOP  | none                       | no operation          |
| `0x1` | ADD  | register, address/register | add                   |
| `0x2` | JMP  | address                    | jump                  |
| `0x3` | HLT  | none                       | halt                  |
| `0x4` | LDR  | register, address/register | load register         |
| `0x5` | STR  | register, address/register | store register        |
| `0x6` | WRT  | value, address/register    | write immediate       |
| `0x7` | JMZ  | address                    | jump if zero          |
| `0x8` | POP  | register                   | pop value from stack  |
| `0x9` | PSH  | address/register           | push value onto stack |

#### operand formats

| value | format             |
| ----- | ------------------ |
| `0x0` | none               |
| `0x1` | register, register |
| `0x2` | register, address  |
| `0x3` | value, address     |
| `0x4` | value, register    |
| `0x5` | address            |
| `0x6` | register           |

## deferred

- A way to connect to external devices: such as a keyboard and display.
- Persistent storage.
