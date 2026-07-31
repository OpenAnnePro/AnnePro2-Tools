use crate::annepro2::AP2Target;
use std::fs::File;
use std::num::ParseIntError;
use std::path::PathBuf;
use std::process;
use structopt::StructOpt;

pub mod annepro2;

fn parse_hex(src: &str) -> Result<u32, ParseIntError> {
    if let Some(number) = src.strip_prefix("0x") {
        u32::from_str_radix(number, 16)
    } else {
        u32::from_str_radix(src, 16)
    }
}

#[derive(StructOpt, Debug)]
#[structopt(name = "annepro2_tools")]
struct ArgOpts {
    /// Override the device-reported partition base. A mismatch is rejected.
    #[structopt(long, parse(try_from_str = parse_hex))]
    base: Option<u32>,

    /// Restart the keyboard after a successful transfer.
    #[structopt(long)]
    boot: bool,

    /// Read and print IAP layout/mode information without writing flash.
    #[structopt(long)]
    probe: bool,

    #[structopt(short = "t", long, default_value = "main")]
    target: String,

    /// Firmware image to flash. Omit only with --probe.
    #[structopt(name = "file", parse(from_os_str))]
    file: Option<PathBuf>,
}

fn parse_target(value: &str) -> Result<AP2Target, String> {
    if value.eq_ignore_ascii_case("ble") {
        Ok(AP2Target::McuBle)
    } else if value.eq_ignore_ascii_case("main") || value.eq_ignore_ascii_case("key") {
        Ok(AP2Target::McuMain)
    } else if value.eq_ignore_ascii_case("led") {
        Ok(AP2Target::McuLed)
    } else {
        Err(format!(
            "invalid target {value:?}; choose main/key, led, or ble"
        ))
    }
}

fn run(args: ArgOpts) -> Result<(), Box<dyn std::error::Error>> {
    if args.probe {
        if args.file.is_some() {
            return Err("--probe does not accept a firmware image".into());
        }
        annepro2::probe()?;
        return Ok(());
    }

    let path = args
        .file
        .ok_or("a firmware image is required unless --probe is used")?;
    let target = parse_target(&args.target)?;
    let mut file = File::open(&path)?;

    println!("Image: {}", path.display());
    annepro2::flash_firmware(target, args.base, &mut file, args.boot)?;
    println!("Flash complete");
    Ok(())
}

fn main() {
    let args = ArgOpts::from_args();
    if let Err(error) = run(args) {
        eprintln!("Flash error: {error}");
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_targets() {
        assert_eq!(parse_target("ble").unwrap(), AP2Target::McuBle);
        assert_eq!(parse_target("KEY").unwrap(), AP2Target::McuMain);
        assert_eq!(parse_target("led").unwrap(), AP2Target::McuLed);
        assert!(parse_target("unknown").is_err());
    }
}
