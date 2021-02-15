use std::intrinsics::transmute;
use hidapi::{HidApi, HidDevice, HidResult};

const ANNEPRO2_VID: u16 = 0x04d9;

const PID_C15: u16 = 0x8008;
const PID_C18: u16 = 0x8009;

#[repr(u8)]
#[derive(Debug, Copy, Clone)]
pub enum AP2Target {
    UsbHost = 1,
    BleHost = 2,
    McuMain = 3,
    McuLed = 4,
    McuBle = 5,
}

#[repr(u8)]
#[derive(Debug, Copy, Clone)]
pub enum L2Command {
    GLOBAL = 1,
    FW = 2,
    KEYBOARD = 16,
    LED = 32,
    MACRO = 48,
    BLE = 64,
}

#[repr(u8)]
#[derive(Debug, Copy, Clone)]
pub enum KeyCommand {
    Reserved = 0,
    IapMode = 1,
    IapGetMode = 2,
    IapGetFwVersion = 3,
    IapWirteMemory = 49,
    // 0x31
    IapWriteApFlag = 50,
    // 0x32
    IapEraseMemory = 67, // 0x43
}

#[derive(Debug, Copy, Clone)]
pub enum AP2FlashError {
    NoDeviceFound,
    MultipleDeviceFound,
    USBError,
    EraseError,
    FlashError,
    OtherError,
}

pub fn flash_firmware<R: std::io::Read>(target: AP2Target, base: u32, file: &mut R, boot: bool) -> std::result::Result<(), AP2FlashError> {
    match HidApi::new() {
        Ok(api) => {
            for dev in api.device_list() {
                println!("HID Dev: {:04x}:{:04x} usage #: {:02x} usage_page #: {:04x} {:?}", dev.vendor_id(), dev.product_id(),
                dev.usage(), dev.usage_page(), dev.product_string());
            }
            let dev = api.device_list().find(|dev| {
                dev.vendor_id() == ANNEPRO2_VID &&
                    ((dev.product_id() == PID_C15 && dev.interface_number() == 1) ||
                     (dev.product_id() == PID_C18))
            }).expect("No device found. Please remind to put the device into IAP Mode");

            let handle = dev.open_device(&api).expect("unable to open device");
            handle.set_blocking_mode(true).expect("non-blocking");
            println!("device is {:?}", handle.get_product_string().expect("string"));

            // Flashing Code
            erase_device(&handle, target, base).map_err(|err| {
                println!("Error while erasing: {}", err);
                AP2FlashError::USBError
            })?;
            flash_file(&handle, target, base, file);
            write_ap_flag(&handle, 2).map_err(|e| {
                println!("Error while writing AP flag: {:?}", e);
                AP2FlashError::USBError
            })?;
            if boot {
                boot_device(&handle).map_err(|e| {
                    println!("Error while booting device: {:?}", e);
                    AP2FlashError::USBError
                })?;
            }
            Ok(())
        }
        Err(e) => {
            println!("Error: {:?}", e);
            Err(AP2FlashError::USBError)
        }
    }
}

pub fn write_ap_flag(handle: &HidDevice, flag: u8) -> HidResult<()> {
    let mut buffer: Vec<u8> = Vec::new();
    buffer.push(L2Command::FW as u8);
    buffer.push(KeyCommand::IapWriteApFlag as u8);
    buffer.push(flag);
    write_to_target(handle, AP2Target::McuMain, &buffer)?;
    Ok(())
}

pub fn flash_file<F: std::io::Read>(handle: &HidDevice, target: AP2Target, base: u32, file: &mut F)
{
    let chunk_size = match &target {
        AP2Target::McuBle => 32usize,
        _ => 48usize,
    };
    let mut current_addr = base;
    loop {
        let mut buffer = vec![0u8; chunk_size];
        let size = file.read(&mut buffer).expect("read file failure");

        if size > 0 {
            let result = write_chunk(handle, target, current_addr, &buffer);
            if result.is_err() {
                println!("[WARNING] Error {:?} occurred during write at {:#08x}, continuing...",
                         result.unwrap_err(), current_addr);
            } else {
                println!("[INFO] Wrote {} bytes, at {:#08x}, total: {} bytes written",
                         size, current_addr, (current_addr + size as u32) - base);
            }
            current_addr += size as u32;
        }

        if size < chunk_size {
            break;
        }
    }
}

pub fn write_chunk(handle: &HidDevice, target: AP2Target, addr: u32, chunk: &[u8]) -> HidResult<()> {
    let mut buffer: Vec<u8> = Vec::new();
    buffer.push(L2Command::FW as u8);
    buffer.push(KeyCommand::IapWirteMemory as u8);
    let addr_slice: [u8; 4] = unsafe { transmute(addr.to_le()) };
    buffer.extend_from_slice(&addr_slice);
    buffer.extend_from_slice(chunk);
    write_to_target(handle, target, &buffer).map(|_| { () })
}

pub fn erase_device(handle: &HidDevice, target: AP2Target, addr: u32) -> HidResult<()> {
    let mut buffer: Vec<u8> = Vec::new();
    buffer.push(L2Command::FW as u8);
    buffer.push(KeyCommand::IapEraseMemory as u8);
    let addr_slice: [u8; 4] = unsafe { transmute(addr.to_le()) };
    buffer.extend_from_slice(&addr_slice);

    write_to_target(handle, target, &buffer)?;
    Ok(())
}

pub fn boot_device(handle: &HidDevice) -> HidResult<()> {
    let buffer: Vec<u8> = vec![0x00, 0x7b, 0x10, 0x31, 0x10, 0x03, 0x00, 0x00, 0x7d, 0x02, 0x01, 0x02];

    // directly use write because we shouldn't pad this command to 64 bytes
    let lol = handle.write(&buffer);

    if lol.is_err() {
        let err = handle.check_error();
        println!("err: {:?}", err);
    }

    Ok(())
}

pub fn write_to_target(handle: &HidDevice, target: AP2Target, payload: &[u8]) -> HidResult<usize> {
    let mut buffer: Vec<u8> = Vec::with_capacity(64);
    buffer.push(0x7b);
    buffer.push(0x10);
    buffer.push((((target as u8) & 0xF) << 4) | AP2Target::UsbHost as u8);
    buffer.push(0x10);
    buffer.push(payload.len() as u8);
    buffer.push(0);
    buffer.push(0);
    buffer.push(0x7d);
    buffer.extend_from_slice(payload);
    if buffer.len() > 64 {
        panic!("Wut?");
    }
    // Pad to 64 bytes
    while buffer.len() < 64 {
        buffer.push(0);
    }

    buffer.insert(0, 0); // First word is report id.

    let lol = handle.write(&buffer);

    if lol.is_err() {
        let err = handle.check_error();
        println!("err: {:?}", err);
    }

    let mut buf: Vec<u8> = vec![0u8; 64];
    handle.read(&mut buf);
    use pretty_hex::*;
    println!("read back: {:#?}", buf[0..].as_ref().hex_dump());

    lol
}
