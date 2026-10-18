use clap::Parser;
use std::path::PathBuf;

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
}

/// An emulator for the stolas computer
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
        }
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

            // ADD destination, source
            0x1 => {
                let destination_id = self.fetch_byte();
                let destination = self.register(destination_id)?;

                let source = match format {
                    0x1 => {
                        let id = self.fetch_byte();
                        self.register(id)?;
                        self.registers[id as usize]
                    }
                    0x2 => {
                        let address = self.fetch_word();
                        self.memory[address as usize]
                    }
                    _ => return Err(format!("Invalid ADD format: {format:#X}")),
                };

                let result = self.registers[destination] as u16 + source as u16;

                self.registers[destination] = result as u8;
                self.carry = result > 0xFF;
                self.zero = self.registers[destination] == 0;
            }

            // JMP address
            0x2 if format == 0x5 => {
                self.rip = self.fetch_word();
            }

            // HLT
            0x3 if format == 0x0 => {
                self.halted = true;
            }

            // LDR destination, source
            0x4 => {
                let destination_id = self.fetch_byte();
                let destination = self.register(destination_id)?;

                let value = match format {
                    0x1 => {
                        let id = self.fetch_byte();
                        self.register(id)?;
                        self.registers[id as usize]
                    }
                    0x2 => {
                        let address = self.fetch_word();
                        self.memory[address as usize]
                    }
                    _ => return Err(format!("Invalid LDR format: {format:#X}")),
                };

                self.registers[destination] = value;
            }

            // STR source, destination
            0x5 => {
                let source_id = self.fetch_byte();
                let source = self.register(source_id)?;
                let value = self.registers[source];

                match format {
                    // Copy source register into destination register.
                    0x1 => {
                        let destination = self.fetch_byte();
                        let destination = self.register(destination)?;
                        self.registers[destination] = value;
                    }
                    // Write source register to memory.
                    0x2 => {
                        let address = self.fetch_word();
                        self.write_memory(address, value)?;
                    }
                    _ => return Err(format!("Invalid STR format: {format:#X}")),
                }
            }

            // WRT immediate, destination
            0x6 => {
                let value = self.fetch_byte();

                match format {
                    // WRT value, address
                    0x3 => {
                        let address = self.fetch_word();
                        self.write_memory(address, value)?;
                    }
                    // WRT value, register
                    0x4 => {
                        let id = self.fetch_byte();
                        let destination = self.register(id)?;
                        self.registers[destination] = value;
                    }
                    _ => return Err(format!("Invalid WRT format: {format:#X}")),
                }
            }

            // JMZ address
            0x7 if format == 0x5 => {
                let address = self.fetch_word();

                if self.zero {
                    self.rip = address;
                }
            }

            // POP register
            0x8 if format == 0x6 => {
                let id = self.fetch_byte();
                let destination = self.register(id)?;

                let value = self.memory[self.rsp as usize];
                self.rsp = self.rsp.wrapping_add(1);

                self.registers[destination] = value;
            }

            // PSH register or memory address
            0x9 => {
                let value = match format {
                    0x5 => {
                        let address = self.fetch_word();
                        self.memory[address as usize]
                    }
                    0x6 => {
                        let id = self.fetch_byte();
                        let source = self.register(id)?;
                        self.registers[source]
                    }
                    _ => return Err(format!("Invalid PSH format: {format:#X}")),
                };

                self.rsp = self.rsp.wrapping_sub(1);
                self.write_memory(self.rsp, value)?;
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

    fn run(&mut self, max_steps: usize) -> Result<usize, String> {
        let mut steps = 0;

        while !self.halted {
            if steps >= max_steps {
                return Err(format!("Execution limit reached: {max_steps} steps"));
            }

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

    let mut cpu = Lux8::new();
    cpu.load_rom(&rom)?;

    println!("Loaded {} bytes from {}", rom.len(), path.display());

    let steps = cpu.run(1_000_000)?;

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
            0x64, 42, 0x00, // WRT 42, r0
            0x64, 8, 0x01, // WRT 8, r1
            0x11, 0x00, 0x01, // ADD r0, r1
            0x30, // HLT
        ])
        .unwrap();

        assert_eq!(cpu.run(100).unwrap(), 4);
        assert_eq!(cpu.registers[0], 50);
        assert_eq!(cpu.registers[1], 8);
        assert!(cpu.halted);
    }

    #[test]
    fn addition_sets_carry_and_zero_flags() {
        let mut cpu = Lux8::new();
        cpu.registers[0] = 255;
        cpu.registers[1] = 1;
        cpu.load_rom(&[0x11, 0x00, 0x01, 0x30]).unwrap();

        cpu.step().unwrap();

        assert_eq!(cpu.registers[0], 0);
        assert!(cpu.carry);
        assert!(cpu.zero);
    }

    #[test]
    fn writes_to_ram() {
        let mut cpu = Lux8::new();
        cpu.load_rom(&[0x63, 42, 0x00, 0x20, 0x30]).unwrap();

        cpu.run(10).unwrap();

        assert_eq!(cpu.memory[0x2000], 42);
    }

    #[test]
    fn rejects_cpu_writes_to_rom() {
        let mut cpu = Lux8::new();
        assert!(cpu.write_memory(0x0000, 42).is_err());
        assert_eq!(cpu.memory[0x0000], 0);
    }

    #[test]
    fn pushes_and_pops_register_values() {
        let mut cpu = Lux8::new();
        cpu.registers[0] = 42;
        cpu.load_rom(&[0x96, 0x00, 0x86, 0x01, 0x30]).unwrap();

        cpu.run(10).unwrap();

        assert_eq!(cpu.registers[1], 42);
        assert_eq!(cpu.rsp, 0xFFFF);
    }

    #[test]
    fn stops_infinite_loops_at_step_limit() {
        let mut cpu = Lux8::new();
        cpu.load_rom(&[0x25, 0x00, 0x00]).unwrap();

        assert!(cpu.run(10).is_err());
    }
}
