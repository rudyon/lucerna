use clap::Parser;
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::path::PathBuf;

#[cfg(unix)]
mod terminal {
    use std::io;
    use std::os::fd::RawFd;

    unsafe extern "C" {
        fn fcntl(fd: RawFd, command: i32, ...) -> i32;
    }

    const F_GETFL: i32 = 3;
    const F_SETFL: i32 = 4;
    const O_NONBLOCK: i32 = 0x800;

    pub fn make_stdin_nonblocking() -> io::Result<i32> {
        // stdin is kept in its normal terminal mode; input becomes available
        // after the user presses enter, without blocking the emulated CPU.
        let flags = unsafe { fcntl(0, F_GETFL) };
        if flags < 0 {
            return Err(io::Error::last_os_error());
        }
        if unsafe { fcntl(0, F_SETFL, flags | O_NONBLOCK) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(flags)
    }

    pub fn restore_stdin(flags: i32) {
        unsafe {
            fcntl(0, F_SETFL, flags);
        }
    }
}

struct Console {
    input: VecDeque<u8>,
    output: Vec<u8>,
    host: bool,
    #[cfg(unix)]
    old_stdin_flags: Option<i32>,
}

impl Console {
    fn test() -> Self {
        Self {
            input: VecDeque::new(),
            output: Vec::new(),
            host: false,
            #[cfg(unix)]
            old_stdin_flags: None,
        }
    }

    fn host() -> io::Result<Self> {
        #[cfg(unix)]
        let old_stdin_flags = Some(terminal::make_stdin_nonblocking()?);
        Ok(Self {
            input: VecDeque::new(),
            output: Vec::new(),
            host: true,
            #[cfg(unix)]
            old_stdin_flags,
        })
    }

    fn poll_input(&mut self) {
        if !self.host {
            return;
        }
        #[cfg(unix)]
        {
            let mut bytes = [0u8; 256];
            loop {
                match io::stdin().read(&mut bytes) {
                    Ok(0) => break,
                    Ok(count) => self.input.extend(&bytes[..count]),
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    Err(_) => break,
                }
            }
        }
    }

    fn read(&mut self, device: u8, port: u8) -> u8 {
        if device != 0x01 {
            return 0;
        }
        self.poll_input();
        match port {
            0x00 => self.input.pop_front().unwrap_or(0),
            0x02 => (!self.input.is_empty()) as u8,
            _ => 0,
        }
    }

    fn write(&mut self, device: u8, port: u8, value: u8) {
        if device != 0x01 || port != 0x01 {
            return;
        }
        if self.host {
            let _ = io::stdout()
                .write_all(&[value])
                .and_then(|_| io::stdout().flush());
        } else {
            self.output.push(value);
        }
    }
}

impl Drop for Console {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(flags) = self.old_stdin_flags {
            terminal::restore_stdin(flags);
        }
    }
}

const MEMORY_SIZE: usize = 65_536;
const ROM_END: usize = 0x1000;

struct Lux8 {
    memory: Vec<u8>,
    registers: [u8; 8],
    rip: u16,
    rsp: u16,
    rir: u8,
    carry: bool,
    zero: bool,
    halted: bool,
    console: Console,
}

/// An emulator for the lucerna computer
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Binary file to load into ROM
    rom: PathBuf,
}

impl Lux8 {
    fn new() -> Self {
        Self {
            memory: vec![0; MEMORY_SIZE],
            registers: [0; 8],
            rip: 0,
            rsp: 0xFFFF,
            rir: 0,
            carry: false,
            zero: false,
            halted: false,
            console: Console::test(),
        }
    }

    fn new_host() -> io::Result<Self> {
        let mut cpu = Self::new();
        cpu.console = Console::host()?;
        Ok(cpu)
    }

    fn load_rom(&mut self, program: &[u8]) -> Result<(), String> {
        if program.len() > ROM_END {
            return Err(format!(
                "ROM image is too large: {} bytes (maximum {} bytes)",
                program.len(),
                ROM_END
            ));
        }

        self.memory[..program.len()].copy_from_slice(program);
        Ok(())
    }

    fn fetch_byte(&mut self) -> u8 {
        let byte = self.memory[self.rip as usize];
        self.rip = self.rip.wrapping_add(1);
        byte
    }

    fn fetch_word(&mut self) -> u16 {
        let low = self.fetch_byte();
        let high = self.fetch_byte();
        u16::from_le_bytes([low, high])
    }

    fn register(&self, id: u8) -> Result<usize, String> {
        if id < 8 {
            Ok(id as usize)
        } else {
            Err(format!("Invalid register: {id}"))
        }
    }

    fn write_memory(&mut self, address: u16, value: u8) -> Result<(), String> {
        if (address as usize) < ROM_END {
            return Err(format!("Attempted write to ROM at {address:#06X}"));
        }

        self.memory[address as usize] = value;
        Ok(())
    }

    fn step(&mut self) -> Result<(), String> {
        if self.halted {
            return Ok(());
        }

        self.rir = self.fetch_byte();

        let opcode = self.rir >> 4;
        let format = self.rir & 0x0F;

        match opcode {
            // NOP
            0x0 if format == 0x0 => {}

            // HLT
            0x1 if format == 0x0 => {
                self.halted = true;
            }

            // JMP address
            0x2 if format == 0x5 => {
                self.rip = self.fetch_word();
            }

            // JMZ address
            0x3 if format == 0x5 => {
                let address = self.fetch_word();

                if self.zero {
                    self.rip = address;
                }
            }

            // LDR destination, source
            0x4 => match format {
                // LDR register, register
                0x2 => {
                    let destination_id = self.fetch_byte();
                    let destination = self.register(destination_id)?;
                    let source_id = self.fetch_byte();
                    let source = self.register(source_id)?;

                    self.registers[destination] = self.registers[source];
                }

                // LDR register, address
                0x3 => {
                    let destination_id = self.fetch_byte();
                    let destination = self.register(destination_id)?;
                    let address = self.fetch_word();

                    self.registers[destination] = self.memory[address as usize];
                }

                _ => return Err(format!("Invalid LDR format: {format:#X}")),
            },

            // STR destination, source
            0x5 => match format {
                // STR destination-register, source-register
                0x2 => {
                    let destination_id = self.fetch_byte();
                    let destination = self.register(destination_id)?;
                    let source_id = self.fetch_byte();
                    let source = self.register(source_id)?;

                    self.registers[destination] = self.registers[source];
                }

                // STR address, source-register
                0x6 => {
                    let address = self.fetch_word();
                    let source_id = self.fetch_byte();
                    let source = self.register(source_id)?;
                    let value = self.registers[source];

                    self.write_memory(address, value)?;
                }

                _ => return Err(format!("Invalid STR format: {format:#X}")),
            },

            // WRT destination, immediate-value
            0x6 => match format {
                // WRT register, value
                0x4 => {
                    let destination_id = self.fetch_byte();
                    let destination = self.register(destination_id)?;
                    let value = self.fetch_byte();

                    self.registers[destination] = value;
                }

                // WRT address, value
                0x8 => {
                    let address = self.fetch_word();
                    let value = self.fetch_byte();

                    self.write_memory(address, value)?;
                }

                _ => return Err(format!("Invalid WRT format: {format:#X}")),
            },

            // POP register
            0x7 if format == 0x1 => {
                let id = self.fetch_byte();
                let destination = self.register(id)?;

                let value = self.memory[self.rsp as usize];
                self.rsp = self.rsp.wrapping_add(1);

                self.registers[destination] = value;
            }

            // PSH register or memory address
            0x8 => {
                let value = match format {
                    0x1 => {
                        let id = self.fetch_byte();
                        let source = self.register(id)?;
                        self.registers[source]
                    }

                    0x5 => {
                        let address = self.fetch_word();
                        self.memory[address as usize]
                    }

                    _ => return Err(format!("Invalid PSH format: {format:#X}")),
                };

                self.rsp = self.rsp.wrapping_sub(1);
                self.write_memory(self.rsp, value)?;
            }

            // DVI/DVO register, device, port
            0xC | 0xD if format == 0x9 => {
                let id = self.fetch_byte();
                let register = self.register(id)?;
                let device = self.fetch_byte();
                let port = self.fetch_byte();
                if opcode == 0xC {
                    self.registers[register] = self.console.read(device, port);
                } else {
                    let value = self.registers[register];
                    self.console.write(device, port, value);
                }
            }

            // ADD, MUL, SUB: destination register, source register/address
            0x9 | 0xA | 0xB => {
                let destination_id = self.fetch_byte();
                let destination = self.register(destination_id)?;

                let source = match format {
                    // Register source
                    0x2 => {
                        let source_id = self.fetch_byte();
                        let source = self.register(source_id)?;
                        self.registers[source]
                    }

                    // Memory source
                    0x3 => {
                        let address = self.fetch_word();
                        self.memory[address as usize]
                    }

                    _ => {
                        return Err(format!("Invalid arithmetic format: {format:#X}"));
                    }
                };

                let left = self.registers[destination];

                let result = match opcode {
                    // ADD
                    0x9 => {
                        let sum = left as u16 + source as u16;
                        self.carry = sum > 0xFF;
                        sum as u8
                    }

                    // MUL
                    0xA => {
                        let product = left as u16 * source as u16;
                        self.carry = product > 0xFF;
                        product as u8
                    }

                    // SUB
                    0xB => {
                        self.carry = left < source;
                        left.wrapping_sub(source)
                    }

                    _ => unreachable!(),
                };

                self.registers[destination] = result;
                self.zero = result == 0;
            }

            _ => {
                return Err(format!(
                    "Invalid instruction: byte {:#04X} at {:#06X}",
                    self.rir,
                    self.rip.wrapping_sub(1)
                ));
            }
        }

        Ok(())
    }

    fn run(&mut self) -> Result<usize, String> {
        let mut steps = 0;

        while !self.halted {
            self.step()?;
            steps += 1;
        }

        Ok(steps)
    }
}

fn main() -> Result<(), String> {
    let args = Args::parse();
    let path = args.rom;

    let rom = std::fs::read(&path)
        .map_err(|error| format!("Could not read {}: {error}", path.display()))?;

    let mut cpu =
        Lux8::new_host().map_err(|error| format!("Could not open host console: {error}"))?;
    cpu.load_rom(&rom)?;

    println!("Loaded {} bytes from {}", rom.len(), path.display());

    let steps = cpu.run()?;

    println!("lux8 halted after {steps} instructions");

    for (index, value) in cpu.registers.iter().enumerate() {
        println!("r{index} = {value}");
    }

    println!("RIP = {:#06X}", cpu.rip);
    println!("RSP = {:#06X}", cpu.rsp);
    println!("Carry = {}", cpu.carry);
    println!("Zero = {}", cpu.zero);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executes_program_and_adds_registers() {
        let mut cpu = Lux8::new();

        cpu.load_rom(&[
            0x64, 0x00, 42, // WRT r0, 42
            0x64, 0x01, 8, // WRT r1, 8
            0x92, 0x00, 0x01, // ADD r0, r1
            0x10, // HLT
        ])
        .unwrap();

        assert_eq!(cpu.run().unwrap(), 4);
        assert_eq!(cpu.registers[0], 50);
        assert_eq!(cpu.registers[1], 8);
        assert!(cpu.halted);
    }

    #[test]
    fn loads_and_stores_registers_and_memory() {
        let mut cpu = Lux8::new();
        cpu.memory[0x2000] = 42;

        cpu.load_rom(&[
            0x64, 0x00, 42, // WRT r0, 42
            0x52, 0x01, 0x00, // STR r1, r0
            0x43, 0x02, 0x00, 0x20, // LDR r2, 0x2000
            0x56, 0x00, 0x20, 0x02, // STR 0x2000, r2
            0x10, // HLT
        ])
        .unwrap();

        cpu.run().unwrap();

        assert_eq!(cpu.registers[1], 42);
        assert_eq!(cpu.registers[2], 42);
        assert_eq!(cpu.memory[0x2000], 42);
    }

    #[test]
    fn writes_immediate_values_to_ram() {
        let mut cpu = Lux8::new();

        cpu.load_rom(&[
            0x68, 0x00, 0x20, 42,   // WRT 0x2000, 42
            0x10, // HLT
        ])
        .unwrap();

        cpu.run().unwrap();
        assert_eq!(cpu.memory[0x2000], 42);
    }

    #[test]
    fn rejects_writes_to_rom() {
        let mut cpu = Lux8::new();

        assert!(cpu.write_memory(0x0000, 42).is_err());
        assert_eq!(cpu.memory[0x0000], 0);
    }

    #[test]
    fn addition_sets_carry_and_zero() {
        let mut cpu = Lux8::new();
        cpu.registers[0] = 255;
        cpu.registers[1] = 1;

        cpu.load_rom(&[0x92, 0x00, 0x01]).unwrap();
        cpu.step().unwrap();

        assert_eq!(cpu.registers[0], 0);
        assert!(cpu.carry);
        assert!(cpu.zero);
    }

    #[test]
    fn multiplication_sets_carry() {
        let mut cpu = Lux8::new();
        cpu.registers[0] = 20;
        cpu.registers[1] = 20;

        cpu.load_rom(&[0xA2, 0x00, 0x01]).unwrap();
        cpu.step().unwrap();

        assert_eq!(cpu.registers[0], 144);
        assert!(cpu.carry);
        assert!(!cpu.zero);
    }

    #[test]
    fn subtraction_sets_borrow_and_zero() {
        let mut cpu = Lux8::new();
        cpu.registers[0] = 2;
        cpu.registers[1] = 3;

        cpu.load_rom(&[0xB2, 0x00, 0x01]).unwrap();
        cpu.step().unwrap();

        assert_eq!(cpu.registers[0], 255);
        assert!(cpu.carry);
        assert!(!cpu.zero);
    }

    #[test]
    fn conditional_jump_uses_zero_flag() {
        let mut cpu = Lux8::new();
        cpu.zero = true;

        cpu.load_rom(&[
            0x35, 0x05, 0x00, // JMZ 0x0005
            0x10, // HLT (skipped)
            0x00, // NOP
            0x10, // HLT at 0x0005
        ])
        .unwrap();

        cpu.step().unwrap();
        assert_eq!(cpu.rip, 0x0005);

        cpu.step().unwrap();
        assert!(cpu.halted);
    }

    #[test]
    fn pushes_and_pops_register_values() {
        let mut cpu = Lux8::new();
        cpu.registers[0] = 42;

        cpu.load_rom(&[
            0x81, 0x00, // PSH r0
            0x71, 0x01, // POP r1
            0x10, // HLT
        ])
        .unwrap();

        cpu.run().unwrap();

        assert_eq!(cpu.registers[1], 42);
        assert_eq!(cpu.rsp, 0xFFFF);
    }

    #[test]
    fn device_io_uses_console_ports() {
        let mut cpu = Lux8::new();
        cpu.console.input.push_back(b'X');
        cpu.load_rom(&[
            0xC9, 0x00, 0x01, 0x02, // DVI r0, console, status
            0xC9, 0x01, 0x01, 0x00, // DVI r1, console, input
            0xD9, 0x01, 0x01, 0x01, // DVO r1, console, output
            0x10,
        ])
        .unwrap();
        cpu.run().unwrap();
        assert_eq!(cpu.registers[0], 1);
        assert_eq!(cpu.registers[1], b'X');
        assert_eq!(cpu.console.output, vec![b'X']);

        let mut cpu = Lux8::new();
        cpu.load_rom(&[0xC9, 0x00, 0x02, 0x00]).unwrap(); // reserved device
        cpu.step().unwrap();
        assert_eq!(cpu.registers[0], 0);
    }

    #[test]
    fn rejects_invalid_registers_and_formats() {
        let mut cpu = Lux8::new();

        cpu.load_rom(&[0x42, 0x08, 0x00]).unwrap(); // LDR r8, r0
        assert!(cpu.step().is_err());

        let mut cpu = Lux8::new();
        cpu.load_rom(&[0x94, 0x00, 42]).unwrap(); // Invalid ADD format
        assert!(cpu.step().is_err());
    }
}
