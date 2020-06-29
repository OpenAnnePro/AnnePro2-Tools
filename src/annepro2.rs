use rusb::{ConfigDescriptor, DeviceDescriptor, DeviceHandle, DeviceList, EndpointDescriptor, InterfaceDescriptor, Language, Result, Speed, UsbContext, Device, GlobalContext, Interface};
use std::time::Duration;
use std::intrinsics::transmute;
use std::panic::resume_unwind;
use pretty_hex::PrettyHex;

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

const USB_TIMEOUT: Duration = Duration::from_secs(1);

pub fn flash_firmware<R: std::io::Read>(target: AP2Target, base: u32, file: &mut R, vid: u16, pid: u16) -> std::result::Result<(), AP2FlashError> {
    let devices = DeviceList::new().map_err(|_| { AP2FlashError::OtherError })?;

    let mut filtered_devices: Vec<Device<GlobalContext>> = devices.iter()
        .filter(|dev| {
            if let Ok(desc) = dev.device_descriptor() {
                return desc.vendor_id() == vid && desc.product_id() == pid;
            }
            return false;
        }).collect();

    println!("{} devices found.", filtered_devices.len());
    if filtered_devices.is_empty() {
        return Err(AP2FlashError::NoDeviceFound);
    }
    if filtered_devices.len() > 1 {
        return Err(AP2FlashError::MultipleDeviceFound);
    }
    let dev = filtered_devices.pop().expect("has dev");
    let dev_desc = dev.device_descriptor().expect("has desc");

    println!("Found Anne Pro 2 In IAP Mode on Bus {:03} Port {:03} vid:pid {:04x}:{:04x}",
             dev.bus_number(), dev.port_number(), dev_desc.vendor_id(), dev_desc.product_id());

    println!("Device has {} configuration(s), using the first one", dev_desc.num_configurations());

    let config_desc = dev.config_descriptor(0).expect("no config");

    let mut dev_handle = dev.open().expect("can't open handle");
    let lang = dev_handle.read_languages(USB_TIMEOUT).map_err(|_| { AP2FlashError::USBError })?[0];

    let dev_name = dev_handle.read_product_string(lang, &dev_desc, USB_TIMEOUT)
        .map_err(|_| { AP2FlashError::USBError })?;
    let dev_serial = dev_handle.read_serial_number_string(lang, &dev_desc, USB_TIMEOUT)
        .map_err(|_| { AP2FlashError::USBError })?;
    println!("Device has name: \"{}\" Serial: {}", dev_name, dev_serial);
    println!("has {} interfaces. We use second interface and EP 3/4", config_desc.num_interfaces());
    if config_desc.num_interfaces() < 2 {
        return Err(AP2FlashError::USBError);
    }

    let mut interfaces: Vec<Interface> = config_desc.interfaces().collect();
    let interface = &mut interfaces[1];

    let ifdesc = interface.descriptors().next().unwrap();
    println!("Found {} eps on if#1", ifdesc.num_endpoints());
    let mut epdesc: Vec<EndpointDescriptor> = ifdesc.endpoint_descriptors().collect();
    println!("{:#?}", epdesc);

    dev_handle.detach_kernel_driver(1);
    dev_handle.claim_interface(1).expect("claim");

    // Flashing Code
    erase_device(&dev_handle, epdesc[0].address(), target, base).map_err(|_| { AP2FlashError::EraseError })?;
    flash_file(&dev_handle, epdesc[0].address(), target, base, file);


    dev_handle.release_interface(1).expect("claim");
    dev_handle.attach_kernel_driver(1);

    Ok(())
}

pub fn flash_file<T: UsbContext, F: std::io::Read>(handle: &DeviceHandle<T>, ep: u8,
                                                   target: AP2Target, base: u32, file: &mut F) {
    let chunk_size = match &target {
        AP2Target::McuBle => 32usize,
        _ => 48usize,
    };
    let mut current_addr = base;
    loop {
        let mut buffer = vec![0u8; chunk_size];
        let size = file.read(&mut buffer).expect("read file failure");

        if size > 0 {
            let result = write_chunk(handle, ep, target, current_addr, &buffer);
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

pub fn write_chunk<T: UsbContext>(handle: &DeviceHandle<T>, ep: u8, target: AP2Target, addr: u32, chunk: &[u8]) -> Result<()> {
    let mut buffer: Vec<u8> = Vec::new();
    buffer.push(L2Command::FW as u8);
    buffer.push(KeyCommand::IapWirteMemory as u8);
    let addr_slice: [u8; 4] = unsafe { transmute(addr.to_le()) };
    buffer.extend_from_slice(&addr_slice);
    buffer.extend_from_slice(chunk);
    write_to_target(handle, ep, target, &buffer).map(|_| { () })
}

pub fn erase_device<T: UsbContext>(handle: &DeviceHandle<T>, ep: u8, target: AP2Target, addr: u32) -> Result<()> {
    let mut buffer: Vec<u8> = Vec::new();
    buffer.push(L2Command::FW as u8);
    buffer.push(KeyCommand::IapEraseMemory as u8);
    let addr_slice: [u8; 4] = unsafe { transmute(addr.to_le()) };
    buffer.extend_from_slice(&addr_slice);

    write_to_target(handle, ep, target, &buffer)?;
    Ok(())
}

pub fn write_to_target<T: UsbContext>(handle: &DeviceHandle<T>, ep: u8, target: AP2Target, payload: &[u8]) -> Result<Vec<u8>> {
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

    handle.write_interrupt(ep, &buffer, USB_TIMEOUT)?;
    let mut buf = vec![0u8; 64];
    let bytes = handle.read_interrupt((ep + 1) | 0x80, &mut buf, USB_TIMEOUT);
    println!("read back {:?} bytes:\n{:#?}", bytes, buf[0..].as_ref().hex_dump());
    Ok(buf)
}