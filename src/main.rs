use crate::annepro2::AP2Target;
use std::fs::File;
use std::num::ParseIntError;
use std::path::PathBuf;
use structopt::StructOpt;

pub mod annepro2;

fn parse_hex(src: &str) -> std::result::Result<u32, ParseIntError> {
    if let Some(num) = src.strip_prefix("0x") {
        u32::from_str_radix(num, 16)
    } else {
        u32::from_str_radix(src, 16)
    }
}

#[derive(StructOpt, Debug)]
#[structopt(name = "annepro2_tools")]
struct ArgOpts {
    #[structopt(long, parse(try_from_str = parse_hex), default_value = "0x4000")]
    base: u32,
    #[structopt(long = "boot")]
    boot: bool,
    #[structopt(short = "t", long, default_value = "main")]
    target: String,
    /// File to be flashed onto device
    #[structopt(name = "file", parse(from_os_str))]
    file: PathBuf,
}

fn main() {
    let args: ArgOpts = ArgOpts::from_args();
    println!("args: {:#x?}", args);
    let mut file = File::open(args.file).expect("invalid file");
    let target;
    if args.target.eq_ignore_ascii_case("ble") {
        target = AP2Target::McuBle;
    } else if args.target.eq_ignore_ascii_case("main") {
        target = AP2Target::McuMain;
    } else if args.target.eq_ignore_ascii_case("led") {
        target = AP2Target::McuLed;
    } else {
        panic!("Invalid target, choose from main, led, and ble");
    }
    let result = annepro2::flash_firmware(target, args.base, &mut file, args.boot);
    if result.is_ok() {
        println!("Flash complete");
        if args.boot {
            println!("Booting Keyboard");
        }
    } else {
        println!("Flash error: {:?}", result.unwrap_err());
    }
}
