use structopt::StructOpt;
use std::num::ParseIntError;
use crate::annepro2::AP2Target;
use std::path::PathBuf;
use std::fs::File;

pub mod annepro2;

fn parse_hex_16(src: &str) -> std::result::Result<u16, ParseIntError> {
    if src.starts_with("0x") {
        u16::from_str_radix(&src[2..], 16)
    } else {
        u16::from_str_radix(src, 16)
    }
}

fn parse_hex(src: &str) -> std::result::Result<u32, ParseIntError> {
    if src.starts_with("0x") {
        u32::from_str_radix(&src[2..], 16)
    } else {
        u32::from_str_radix(src, 16)
    }
}

#[derive(StructOpt, Debug)]
#[structopt(name = "annepro2_tools")]
struct ArgOpts {
    #[structopt(short="v", long="vid", parse(try_from_str = parse_hex_16), default_value = "04d9")]
    vid: u16,
    #[structopt(short="p", long="pid", parse(try_from_str = parse_hex_16), default_value = "8008")]
    pid: u16,
    #[structopt(long, parse(try_from_str = parse_hex), default_value = "0x4000")]
    base: u32,
    /// File to be flashed onto device
    #[structopt(name = "file", parse(from_os_str))]
    file: PathBuf,
}

fn main() {
    let args: ArgOpts = ArgOpts::from_args();
    println!("args: {:#x?}", args);
    let mut file = File::open(args.file).expect("invalid file");
    let result = annepro2::flash_firmware(AP2Target::McuMain, args.base, &mut file, args.vid, args.pid);
    if result.is_ok() {
        println!("Flash complete");
    } else {
        println!("Flash error: {:?}", result.unwrap_err());
    }
}
