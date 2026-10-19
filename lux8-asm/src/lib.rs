use std::collections::HashMap;

const ROM_SIZE: usize = 4096;

#[derive(Debug, Clone)]
struct Statement {
    line: usize,
    instruction: String,
    operands: Vec<String>,
}

/// Assemble a lux8 assembly source file into a ROM image
///
/// Instructions and registers are case-insensitive. Labels are case-sensitive.
pub fn assemble(source: &str) -> Result<Vec<u8>, String> {
    let mut labels = HashMap::<String, usize>::new();
    let mut statements = Vec::new();
    let mut address = 0usize;

    for (line_number, source_line) in source.lines().enumerate() {
        let line_number = line_number + 1;
        let line = source_line.split(';').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }

        let (label, rest) = if let Some(colon) = line.find(':') {
            let name = line[..colon].trim();
            if !valid_label(name) {
                return Err(error(line_number, "invalid label name"));
            }
            (Some(name.to_string()), line[colon + 1..].trim())
        } else {
            (None, line)
        };

        if let Some(name) = label
            && labels.insert(name.clone(), address).is_some()
        {
            return Err(error(line_number, format!("duplicate label '{name}'")));
        }
        if rest.is_empty() {
            continue;
        }

        let mut fields = rest.splitn(2, char::is_whitespace);
        let instruction = fields.next().unwrap().to_ascii_uppercase();
        let operand_text = fields.next().unwrap_or_default().trim();
        let operands = if operand_text.is_empty() {
            Vec::new()
        } else {
            operand_text
                .split(',')
                .map(str::trim)
                .map(str::to_string)
                .collect()
        };
        if operands.iter().any(String::is_empty) {
            return Err(error(line_number, "malformed operand list"));
        }

        let statement = Statement {
            line: line_number,
            instruction,
            operands,
        };
        let size = instruction_size(&statement, &labels, true)?;
        address = address
            .checked_add(size)
            .ok_or_else(|| error(line_number, "program is too large"))?;
        if address > ROM_SIZE {
            return Err(error(
                line_number,
                format!("program exceeds the {ROM_SIZE}-byte ROM capacity"),
            ));
        }
        statements.push(statement);
    }

    let mut image = Vec::with_capacity(address);
    for statement in &statements {
        encode(statement, &labels, &mut image)?;
    }
    Ok(image)
}

fn error(line: usize, message: impl std::fmt::Display) -> String {
    format!("line {line}: {message}")
}

fn valid_label(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(c) if c == '_' || c.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

fn expect_count(statement: &Statement, count: usize) -> Result<(), String> {
    if statement.operands.len() != count {
        Err(error(
            statement.line,
            format!(
                "{} expects {count} operand(s), got {}",
                statement.instruction,
                statement.operands.len()
            ),
        ))
    } else {
        Ok(())
    }
}

fn parse_register(value: &str, line: usize) -> Result<u8, String> {
    let lower = value.to_ascii_lowercase();
    let digits = lower
        .strip_prefix('r')
        .ok_or_else(|| error(line, format!("invalid register '{value}'")))?;
    let id = digits
        .parse::<u8>()
        .map_err(|_| error(line, format!("invalid register '{value}'")))?;
    if id >= 8 {
        return Err(error(
            line,
            format!("invalid register '{value}' (expected r0 through r7)"),
        ));
    }
    Ok(id)
}

fn parse_number(value: &str, line: usize) -> Result<u16, String> {
    let result = if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        if hex.is_empty() {
            return Err(error(line, format!("invalid number '{value}'")));
        }
        u16::from_str_radix(hex, 16)
    } else if !value.is_empty() && value.chars().all(|c| c.is_ascii_digit()) {
        value.parse::<u16>()
    } else {
        return Err(error(line, format!("invalid number '{value}'")));
    };
    result.map_err(|_| error(line, format!("number out of range: '{value}'")))
}

fn parse_address(
    value: &str,
    labels: &HashMap<String, usize>,
    line: usize,
    unresolved_ok: bool,
) -> Result<u16, String> {
    if valid_label(value) {
        if let Some(&address) = labels.get(value) {
            return u16::try_from(address).map_err(|_| error(line, "address is out of range"));
        }
        if unresolved_ok {
            return Ok(0);
        }
        return Err(error(line, format!("undefined label '{value}'")));
    }
    parse_number(value, line)
}

fn check_byte(value: u16, line: usize, description: &str) -> Result<(), String> {
    if value > u8::MAX as u16 {
        Err(error(
            line,
            format!("{description} is out of range (expected 0..255)"),
        ))
    } else {
        Ok(())
    }
}

fn instruction_size(
    statement: &Statement,
    labels: &HashMap<String, usize>,
    unresolved_ok: bool,
) -> Result<usize, String> {
    let op = statement.instruction.as_str();
    let args = &statement.operands;
    match op {
        "NOP" | "HLT" => {
            expect_count(statement, 0)?;
            Ok(1)
        }
        "JMP" | "JMZ" => {
            expect_count(statement, 1)?;
            parse_address(&args[0], labels, statement.line, unresolved_ok)?;
            Ok(3)
        }
        "LDR" => {
            expect_count(statement, 2)?;
            parse_register(&args[0], statement.line)?;
            if is_register(&args[1]) {
                parse_register(&args[1], statement.line)?;
                Ok(3)
            } else {
                parse_address(&args[1], labels, statement.line, unresolved_ok)?;
                Ok(4)
            }
        }
        "STR" => {
            expect_count(statement, 2)?;
            if is_register(&args[0]) {
                parse_register(&args[0], statement.line)?;
                parse_register(&args[1], statement.line)?;
                Ok(3)
            } else {
                parse_address(&args[0], labels, statement.line, unresolved_ok)?;
                parse_register(&args[1], statement.line)?;
                Ok(4)
            }
        }
        "WRT" => {
            expect_count(statement, 2)?;
            let first_is_register = is_register(&args[0]);
            if first_is_register {
                parse_register(&args[0], statement.line)?;
            } else {
                parse_address(&args[0], labels, statement.line, unresolved_ok)?;
            }
            let value = parse_number(&args[1], statement.line)?;
            check_byte(value, statement.line, "immediate value")?;
            Ok(if first_is_register { 3 } else { 4 })
        }
        "POP" => {
            expect_count(statement, 1)?;
            parse_register(&args[0], statement.line)?;
            Ok(2)
        }
        "PSH" => {
            expect_count(statement, 1)?;
            if is_register(&args[0]) {
                parse_register(&args[0], statement.line)?;
                Ok(2)
            } else {
                parse_address(&args[0], labels, statement.line, unresolved_ok)?;
                Ok(3)
            }
        }
        "ADD" | "MUL" | "SUB" => {
            expect_count(statement, 2)?;
            parse_register(&args[0], statement.line)?;
            if is_register(&args[1]) {
                parse_register(&args[1], statement.line)?;
                Ok(3)
            } else {
                parse_address(&args[1], labels, statement.line, unresolved_ok)?;
                Ok(4)
            }
        }
        "DVI" | "DVO" => {
            expect_count(statement, 3)?;
            parse_register(&args[0], statement.line)?;
            let device = parse_number(&args[1], statement.line)?;
            let port = parse_number(&args[2], statement.line)?;
            check_byte(device, statement.line, "device address")?;
            check_byte(port, statement.line, "port address")?;
            Ok(4)
        }
        _ => Err(error(
            statement.line,
            format!("unknown instruction '{}'", statement.instruction),
        )),
    }
}

fn is_register(value: &str) -> bool {
    value.len() >= 2 && value.as_bytes()[0].eq_ignore_ascii_case(&b'r')
}

fn word(value: u16, output: &mut Vec<u8>) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn encode(
    statement: &Statement,
    labels: &HashMap<String, usize>,
    output: &mut Vec<u8>,
) -> Result<(), String> {
    instruction_size(statement, labels, false)?;
    let a = &statement.operands;
    let line = statement.line;
    let address = |s: &str| parse_address(s, labels, line, false);
    match statement.instruction.as_str() {
        "NOP" => output.push(0x00),
        "HLT" => output.push(0x10),
        "JMP" => {
            output.push(0x25);
            word(address(&a[0])?, output);
        }
        "JMZ" => {
            output.push(0x35);
            word(address(&a[0])?, output);
        }
        "LDR" => {
            let r = parse_register(&a[0], line)?;
            if is_register(&a[1]) {
                output.extend([0x42, r, parse_register(&a[1], line)?]);
            } else {
                output.push(0x43);
                output.push(r);
                word(address(&a[1])?, output);
            }
        }
        "STR" => {
            if is_register(&a[0]) {
                output.extend([
                    0x52,
                    parse_register(&a[0], line)?,
                    parse_register(&a[1], line)?,
                ]);
            } else {
                output.push(0x56);
                word(address(&a[0])?, output);
                output.push(parse_register(&a[1], line)?);
            }
        }
        "WRT" => {
            let value = parse_number(&a[1], line)? as u8;
            if is_register(&a[0]) {
                output.extend([0x64, parse_register(&a[0], line)?, value]);
            } else {
                output.push(0x68);
                word(address(&a[0])?, output);
                output.push(value);
            }
        }
        "POP" => output.extend([0x71, parse_register(&a[0], line)?]),
        "PSH" => {
            if is_register(&a[0]) {
                output.extend([0x81, parse_register(&a[0], line)?]);
            } else {
                output.push(0x85);
                word(address(&a[0])?, output);
            }
        }
        "ADD" | "MUL" | "SUB" => {
            let opcode = match statement.instruction.as_str() {
                "ADD" => 0x90,
                "MUL" => 0xA0,
                _ => 0xB0,
            };
            output.push(opcode | if is_register(&a[1]) { 2 } else { 3 });
            output.push(parse_register(&a[0], line)?);
            if is_register(&a[1]) {
                output.push(parse_register(&a[1], line)?);
            } else {
                word(address(&a[1])?, output);
            }
        }
        "DVI" | "DVO" => {
            let opcode = if statement.instruction == "DVI" {
                0xC0
            } else {
                0xD0
            };
            output.extend([
                opcode | 0x09,
                parse_register(&a[0], line)?,
                parse_number(&a[1], line)? as u8,
                parse_number(&a[2], line)? as u8,
            ]);
        }
        _ => unreachable!(),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::assemble;

    #[test]
    fn encodes_every_instruction_form() {
        let source = "NOP\nHLT\nJMP 0x1234\nJMZ 7\nLDR r1, r2\nLDR r1, 0x2000\nSTR r1, r2\nSTR 0x2000, r2\nWRT r1, 255\nWRT 0x2000, 42\nPOP r3\nPSH r3\nPSH 0x2000\nADD r0, r1\nADD r0, 0x2000\nMUL r0, r1\nSUB r0, 0x2000\nDVI r0, 1, 0\nDVO r0, 1, 1\n";
        let image = assemble(source).unwrap();
        assert_eq!(&image[0..8], &[0x00, 0x10, 0x25, 0x34, 0x12, 0x35, 7, 0]);
        assert!(image.windows(3).any(|x| x == [0x42, 1, 2]));
        assert!(image.windows(4).any(|x| x == [0x43, 1, 0, 0x20]));
        assert!(image.windows(4).any(|x| x == [0x56, 0, 0x20, 2]));
        assert!(image.windows(4).any(|x| x == [0x68, 0, 0x20, 42]));
        assert!(image.windows(3).any(|x| x == [0x92, 0, 1]));
        assert!(image.windows(4).any(|x| x == [0x93, 0, 0, 0x20]));
        assert!(image.windows(4).any(|x| x == [0xC9, 0, 1, 0]));
        assert!(image.windows(4).any(|x| x == [0xD9, 0, 1, 1]));
    }

    #[test]
    fn resolves_forward_and_backward_labels_and_comments() {
        let image = assemble("start: JMP end ; forward\nNOP\nend: JMZ start\n; blank\n").unwrap();
        assert_eq!(image, vec![0x25, 4, 0, 0x00, 0x35, 0, 0]);
    }

    #[test]
    fn rejects_bad_input() {
        for source in [
            "WHAT",
            "LDR r8, r0",
            "WRT r0, 256",
            "DVI r0, 1, 256",
            "JMP missing",
            "start: NOP\nstart: HLT",
        ] {
            assert!(assemble(source).is_err(), "accepted {source}");
        }
    }

    #[test]
    fn rejects_rom_overflow() {
        let source = "NOP\n".repeat(4097);
        assert!(assemble(&source).is_err());
    }
}
