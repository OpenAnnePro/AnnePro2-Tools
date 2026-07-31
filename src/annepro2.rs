use hidapi::{HidApi, HidDevice};
use std::fmt;
use std::io::{self, IsTerminal, Read, Write};
use std::thread;
use std::time::{Duration, Instant};

const ANNEPRO2_VID: u16 = 0x04d9;
const PID_C15: u16 = 0x8008;
const PID_C18: u16 = 0x8009;

const HID_REPORT_ID: u8 = 0;
const HID_REPORT_SIZE: usize = 64;
const HID_WRITE_SIZE: usize = HID_REPORT_SIZE + 1;
const LIANA_SOH: u8 = 0x7b;
const LIANA_EOH: u8 = 0x7d;
const LIANA_VERSION: u8 = 0x10;
const LIANA_SEQUENCE: u8 = 0x10;
const COMMAND_HEADER_SIZE: usize = 2;
const OUTER_HEADER_SIZE: usize = 8;
const MAX_PAYLOAD_SIZE: usize = HID_REPORT_SIZE - OUTER_HEADER_SIZE;
const DEFAULT_REPLY_TIMEOUT: Duration = Duration::from_secs(5);
const ERASE_REPLY_TIMEOUT: Duration = Duration::from_secs(30);
const DEVICE_WAIT_TIMEOUT: Duration = Duration::from_secs(20);
const DEVICE_POLL_INTERVAL: Duration = Duration::from_secs(1);
const PROGRESS_INTERVAL: usize = 4096;
const PROGRESS_BAR_WIDTH: usize = 24;

struct RefreshLine {
    interactive: bool,
    active: bool,
}

impl RefreshLine {
    fn new(message: &str) -> Self {
        let mut line = Self {
            interactive: io::stdout().is_terminal(),
            active: false,
        };
        if line.interactive {
            line.refresh(message);
        } else {
            println!("{message}");
        }
        line
    }

    fn refresh(&mut self, message: &str) {
        if !self.interactive {
            return;
        }

        let mut stdout = io::stdout().lock();
        let _ = write!(stdout, "\r\x1b[2K{message}");
        let _ = stdout.flush();
        self.active = true;
    }

    fn finish(&mut self, message: &str) {
        if self.interactive {
            let mut stdout = io::stdout().lock();
            let _ = writeln!(stdout, "\r\x1b[2K{message}");
            let _ = stdout.flush();
            self.active = false;
        } else {
            println!("{message}");
        }
    }
}

impl Drop for RefreshLine {
    fn drop(&mut self) {
        if self.interactive && self.active {
            let mut stdout = io::stdout().lock();
            let _ = writeln!(stdout);
            let _ = stdout.flush();
        }
    }
}

#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum AP2Target {
    Reserved = 0,
    UsbHost = 1,
    BleHost = 2,
    McuMain = 3,
    McuLed = 4,
    McuBle = 5,
}

impl fmt::Display for AP2Target {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            AP2Target::Reserved => "reserved",
            AP2Target::UsbHost => "USB host",
            AP2Target::BleHost => "BLE host",
            AP2Target::McuMain => "main MCU",
            AP2Target::McuLed => "LED MCU",
            AP2Target::McuBle => "BLE MCU",
        };
        formatter.write_str(name)
    }
}

#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum L2Command {
    Global = 1,
    Firmware = 2,
    Keyboard = 16,
    Led = 32,
    Macro = 48,
    Ble = 64,
}

#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum KeyCommand {
    Reserved = 0,
    IapMode = 1,
    IapGetMode = 2,
    IapGetFwVersion = 3,
    IapWriteMemory = 0x31,
    IapWriteApFlag = 0x32,
    IapEraseMemory = 0x43,
}

#[derive(Debug)]
pub enum AP2FlashError {
    NoDeviceFound,
    MultipleDevicesFound(usize),
    Usb(String),
    Io(String),
    Protocol(String),
    Timeout {
        target: AP2Target,
        command: u8,
    },
    DeviceRejected {
        target: AP2Target,
        command: u8,
        status: u8,
    },
    BaseMismatch {
        target: AP2Target,
        requested: u32,
        detected: u32,
    },
    UnsupportedTarget(AP2Target),
}

impl fmt::Display for AP2FlashError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AP2FlashError::NoDeviceFound => formatter.write_str("no Anne Pro 2 IAP device found"),
            AP2FlashError::MultipleDevicesFound(count) => {
                write!(formatter, "found {count} Anne Pro 2 IAP devices; connect only one")
            }
            AP2FlashError::Usb(message) => write!(formatter, "USB error: {message}"),
            AP2FlashError::Io(message) => write!(formatter, "I/O error: {message}"),
            AP2FlashError::Protocol(message) => write!(formatter, "protocol error: {message}"),
            AP2FlashError::Timeout { target, command } => write!(
                formatter,
                "timed out waiting for {target} command 0x{command:02x}"
            ),
            AP2FlashError::DeviceRejected {
                target,
                command,
                status,
            } => write!(
                formatter,
                "{target} rejected command 0x{command:02x} with status 0x{status:02x}"
            ),
            AP2FlashError::BaseMismatch {
                target,
                requested,
                detected,
            } => write!(
                formatter,
                "{target} base mismatch: requested 0x{requested:08x}, device reports 0x{detected:08x}"
            ),
            AP2FlashError::UnsupportedTarget(target) => {
                write!(formatter, "no firmware partition is defined for {target}")
            }
        }
    }
}

impl std::error::Error for AP2FlashError {}

impl From<hidapi::HidError> for AP2FlashError {
    fn from(error: hidapi::HidError) -> Self {
        AP2FlashError::Usb(error.to_string())
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct FirmwareLayout {
    pub main_base: u32,
    pub led_base: u32,
    pub ble_base: u32,
}

impl FirmwareLayout {
    pub fn base_for(&self, target: AP2Target) -> Result<u32, AP2FlashError> {
        match target {
            AP2Target::McuMain => Ok(self.main_base),
            AP2Target::McuLed => Ok(self.led_base),
            AP2Target::McuBle => Ok(self.ble_base),
            _ => Err(AP2FlashError::UnsupportedTarget(target)),
        }
    }

    fn from_response(body: &[u8]) -> Result<Self, AP2FlashError> {
        // ObinsKit 1.2.11 selects the bases from response bytes 2..6,
        // 12..16 and 22..26 respectively.
        if body.len() < 26 {
            return Err(AP2FlashError::Protocol(format!(
                "firmware layout response is too short: {} bytes",
                body.len()
            )));
        }

        Ok(Self {
            main_base: read_u32_le(&body[2..6]),
            led_base: read_u32_le(&body[12..16]),
            ble_base: read_u32_le(&body[22..26]),
        })
    }
}

#[derive(Debug, Eq, PartialEq)]
struct ResponseFrame {
    source: u8,
    destination: u8,
    command: u8,
    key: u8,
    body: Vec<u8>,
}

pub fn probe() -> Result<FirmwareLayout, AP2FlashError> {
    let api = wait_for_api_device()?;
    let handle = open_iap_device(&api)?;
    let layout = read_firmware_layout(&handle)?;

    println!("IAP firmware layout:");
    println!("  main: 0x{:08x}", layout.main_base);
    println!("  led:  0x{:08x}", layout.led_base);
    println!("  ble:  0x{:08x}", layout.ble_base);

    for target in [AP2Target::McuMain, AP2Target::McuLed, AP2Target::McuBle] {
        match read_iap_mode(&handle, target) {
            Ok(mode) => println!("  {target} mode: {mode}"),
            Err(error) => println!("  {target} mode: unavailable ({error})"),
        }
    }

    Ok(layout)
}

pub fn flash_firmware<R: Read>(
    target: AP2Target,
    requested_base: Option<u32>,
    file: &mut R,
    boot: bool,
) -> Result<(), AP2FlashError> {
    // Read the complete image before opening or erasing the device. This keeps
    // an empty, truncated, or otherwise unreadable input from failing only
    // after the target application has already been erased.
    let image = read_firmware_image(file)?;
    let api = wait_for_api_device()?;
    let handle = open_iap_device(&api)?;
    let layout = read_firmware_layout(&handle)?;
    let detected_base = layout.base_for(target)?;
    let base = match requested_base {
        Some(requested) if requested != detected_base => {
            return Err(AP2FlashError::BaseMismatch {
                target,
                requested,
                detected: detected_base,
            })
        }
        Some(requested) => requested,
        None => detected_base,
    };
    validate_image_span(target, base, image.len())?;

    let mode = read_iap_mode(&handle, target)?;
    // ObinsKit treats only mode 2 as "not in IAP"; C18 reports mode 1 while
    // the target is ready for IAP writes.
    if mode == 2 {
        return Err(AP2FlashError::Protocol(format!(
            "{target} is not in IAP mode (reported mode {mode})"
        )));
    }

    println!("Flashing {target} at device-reported base 0x{base:08x}");
    erase_device(&handle, target, base)?;
    flash_image(&handle, target, base, &image)?;

    if boot {
        println!("Restarting keyboard");
        write_iap_mode_without_reply(&handle, AP2Target::McuMain, 2)?;
    }

    Ok(())
}

fn chunk_size(target: AP2Target) -> usize {
    if target == AP2Target::McuBle {
        32
    } else {
        48
    }
}

fn read_firmware_image<R: Read>(file: &mut R) -> Result<Vec<u8>, AP2FlashError> {
    let mut image = Vec::new();
    file.read_to_end(&mut image)
        .map_err(|error| AP2FlashError::Io(error.to_string()))?;
    if image.is_empty() {
        return Err(AP2FlashError::Protocol(
            "firmware image is empty; refusing to erase the target".to_owned(),
        ));
    }
    Ok(image)
}

fn validate_image_span(
    target: AP2Target,
    base: u32,
    image_size: usize,
) -> Result<(), AP2FlashError> {
    let write_size = chunk_size(target);
    let padded_size = image_size
        .checked_add(write_size - 1)
        .map(|size| size / write_size * write_size)
        .ok_or_else(|| AP2FlashError::Protocol("firmware image size overflow".to_owned()))?;
    if padded_size > u32::MAX as usize || base.checked_add(padded_size as u32).is_none() {
        return Err(AP2FlashError::Protocol(
            "firmware image exceeds the IAP address space".to_owned(),
        ));
    }
    Ok(())
}

fn wait_for_api_device() -> Result<HidApi, AP2FlashError> {
    let deadline = Instant::now() + DEVICE_WAIT_TIMEOUT;
    let wait_seconds = DEVICE_WAIT_TIMEOUT.as_secs();
    let mut wait_line: Option<RefreshLine> = None;

    loop {
        let api = HidApi::new()?;
        let count = iap_devices(&api).len();
        if count == 1 {
            if let Some(line) = wait_line.as_mut() {
                line.finish("IAP device detected");
            }
            return Ok(api);
        }
        if count > 1 {
            return Err(AP2FlashError::MultipleDevicesFound(count));
        }

        let now = Instant::now();
        if now >= deadline {
            if let Some(line) = wait_line.as_mut() {
                line.finish(&format!(
                    "IAP device not found after {wait_seconds} seconds"
                ));
            }
            return Err(AP2FlashError::NoDeviceFound);
        }

        if wait_line.is_none() {
            println!("Put the keyboard into IAP mode by reconnecting it while holding Esc.");
            wait_line = Some(RefreshLine::new(&format!(
                "Waiting for IAP device ({wait_seconds}s remaining)"
            )));
        }

        let remaining = deadline.saturating_duration_since(now);
        let remaining_seconds = remaining.as_secs() + u64::from(remaining.subsec_nanos() > 0);
        wait_line.as_mut().unwrap().refresh(&format!(
            "Waiting for IAP device ({remaining_seconds}s remaining)"
        ));
        thread::sleep(DEVICE_POLL_INTERVAL.min(remaining));
    }
}

fn iap_devices(api: &HidApi) -> Vec<&hidapi::DeviceInfo> {
    api.device_list()
        .filter(|device| {
            device.vendor_id() == ANNEPRO2_VID
                && ((device.product_id() == PID_C15 && device.interface_number() == 1)
                    || device.product_id() == PID_C18)
        })
        .collect()
}

fn open_iap_device(api: &HidApi) -> Result<HidDevice, AP2FlashError> {
    let devices = iap_devices(api);
    match devices.as_slice() {
        [] => Err(AP2FlashError::NoDeviceFound),
        [device] => {
            println!(
                "Using {:04x}:{:04x} {}",
                device.vendor_id(),
                device.product_id(),
                device.product_string().unwrap_or("Anne Pro 2 IAP")
            );
            Ok(api.open_path(device.path())?)
        }
        _ => Err(AP2FlashError::MultipleDevicesFound(devices.len())),
    }
}

fn read_firmware_layout(handle: &HidDevice) -> Result<FirmwareLayout, AP2FlashError> {
    // This mirrors ObinsKit's readIapFwVersion dispatch:
    // target=MCU_MAIN, command=IAP, key=GET_FW_VERSION. The response contains
    // all three partition descriptors.
    let body = request(
        handle,
        AP2Target::McuMain,
        L2Command::Firmware as u8,
        KeyCommand::IapGetFwVersion as u8,
        &[],
        DEFAULT_REPLY_TIMEOUT,
    )?;
    FirmwareLayout::from_response(&body)
}

fn read_iap_mode(handle: &HidDevice, target: AP2Target) -> Result<u8, AP2FlashError> {
    let body = request(
        handle,
        target,
        L2Command::Firmware as u8,
        KeyCommand::IapGetMode as u8,
        &[],
        DEFAULT_REPLY_TIMEOUT,
    )?;
    body.first().copied().ok_or_else(|| {
        AP2FlashError::Protocol(format!("{target} IAP mode response has no mode byte"))
    })
}

fn write_iap_mode_without_reply(
    handle: &HidDevice,
    target: AP2Target,
    mode: u8,
) -> Result<(), AP2FlashError> {
    send_request(
        handle,
        target,
        L2Command::Firmware as u8,
        KeyCommand::IapMode as u8,
        &[mode],
    )
}

fn flash_image(
    handle: &HidDevice,
    target: AP2Target,
    base: u32,
    image: &[u8],
) -> Result<(), AP2FlashError> {
    let chunk_size = chunk_size(target);
    let mut current_addr = base;
    let mut total_written = 0usize;
    let mut next_progress = PROGRESS_INTERVAL;
    let mut progress = RefreshLine::new(&format_transfer_progress(
        total_written,
        image.len(),
        current_addr,
    ));

    for image_chunk in image.chunks(chunk_size) {
        let mut chunk = vec![0u8; chunk_size];
        let size = image_chunk.len();
        chunk[..size].copy_from_slice(image_chunk);

        write_chunk(handle, target, current_addr, &chunk)?;
        current_addr = current_addr
            .checked_add(chunk_size as u32)
            .ok_or_else(|| AP2FlashError::Protocol("flash address overflow".to_owned()))?;
        total_written += size;

        if total_written >= next_progress && total_written < image.len() {
            progress.refresh(&format_transfer_progress(
                total_written,
                image.len(),
                current_addr,
            ));
            next_progress = total_written.saturating_add(PROGRESS_INTERVAL);
        }
    }

    progress.finish(&format_transfer_progress(
        total_written,
        image.len(),
        current_addr,
    ));
    Ok(())
}

fn format_transfer_progress(written: usize, total: usize, next_address: u32) -> String {
    let capped_written = written.min(total);
    let filled = if total == 0 {
        PROGRESS_BAR_WIDTH
    } else {
        ((capped_written as u128 * PROGRESS_BAR_WIDTH as u128) / total as u128) as usize
    };
    let percent = if total == 0 {
        100
    } else {
        ((capped_written as u128 * 100) / total as u128) as usize
    };
    let bar = format!(
        "{}{}",
        "=".repeat(filled),
        "-".repeat(PROGRESS_BAR_WIDTH - filled)
    );

    format!(
        "Writing [{bar}] {percent:3}% {}/{} (next 0x{next_address:08x})",
        format_bytes(capped_written),
        format_bytes(total),
    )
}

fn format_bytes(bytes: usize) -> String {
    const KIB: usize = 1024;
    const MIB: usize = 1024 * KIB;

    if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}

fn write_chunk(
    handle: &HidDevice,
    target: AP2Target,
    address: u32,
    chunk: &[u8],
) -> Result<(), AP2FlashError> {
    let mut args = Vec::with_capacity(4 + chunk.len());
    args.extend_from_slice(&address.to_le_bytes());
    args.extend_from_slice(chunk);
    let body = request(
        handle,
        target,
        L2Command::Firmware as u8,
        KeyCommand::IapWriteMemory as u8,
        &args,
        DEFAULT_REPLY_TIMEOUT,
    )?;
    expect_success(target, KeyCommand::IapWriteMemory as u8, &body)
}

fn erase_device(handle: &HidDevice, target: AP2Target, address: u32) -> Result<(), AP2FlashError> {
    println!("Erasing {target} application region");
    let body = request(
        handle,
        target,
        L2Command::Firmware as u8,
        KeyCommand::IapEraseMemory as u8,
        &address.to_le_bytes(),
        ERASE_REPLY_TIMEOUT,
    )?;
    expect_success(target, KeyCommand::IapEraseMemory as u8, &body)
}

fn expect_success(target: AP2Target, command: u8, body: &[u8]) -> Result<(), AP2FlashError> {
    let status = body.first().copied().ok_or_else(|| {
        AP2FlashError::Protocol(format!(
            "{target} command 0x{command:02x} response has no status byte"
        ))
    })?;
    if status == 0 {
        Ok(())
    } else {
        Err(AP2FlashError::DeviceRejected {
            target,
            command,
            status,
        })
    }
}

fn request(
    handle: &HidDevice,
    target: AP2Target,
    command: u8,
    key: u8,
    args: &[u8],
    timeout: Duration,
) -> Result<Vec<u8>, AP2FlashError> {
    send_request(handle, target, command, key, args)?;
    read_matching_response(handle, target, command, key, timeout)
}

fn send_request(
    handle: &HidDevice,
    target: AP2Target,
    command: u8,
    key: u8,
    args: &[u8],
) -> Result<(), AP2FlashError> {
    let report = build_report(target, command, key, args)?;
    let written = handle.write(&report)?;
    if written != HID_WRITE_SIZE {
        return Err(AP2FlashError::Usb(format!(
            "short HID write: expected {HID_WRITE_SIZE}, wrote {written}"
        )));
    }
    Ok(())
}

fn read_matching_response(
    handle: &HidDevice,
    target: AP2Target,
    command: u8,
    key: u8,
    timeout: Duration,
) -> Result<Vec<u8>, AP2FlashError> {
    let deadline = Instant::now() + timeout;
    let mut report = [0u8; HID_REPORT_SIZE];

    loop {
        let now = Instant::now();
        if now >= deadline {
            return Err(AP2FlashError::Timeout {
                target,
                command: key,
            });
        }

        let remaining_ms = deadline
            .saturating_duration_since(now)
            .as_millis()
            .clamp(1, i32::MAX as u128) as i32;
        let size = handle.read_timeout(&mut report, remaining_ms)?;
        if size == 0 {
            continue;
        }

        let frame = parse_response(&report[..size])?;
        if frame.destination != AP2Target::UsbHost as u8 {
            continue;
        }
        if target != AP2Target::Reserved && frame.source != target as u8 {
            continue;
        }
        if frame.command != command || frame.key != key {
            eprintln!(
                "Ignoring unrelated response from source {}: command 0x{:02x}/0x{:02x}",
                frame.source, frame.command, frame.key
            );
            continue;
        }
        return Ok(frame.body);
    }
}

fn build_report(
    target: AP2Target,
    command: u8,
    key: u8,
    args: &[u8],
) -> Result<[u8; HID_WRITE_SIZE], AP2FlashError> {
    let payload_len = COMMAND_HEADER_SIZE + args.len();
    if payload_len > MAX_PAYLOAD_SIZE {
        return Err(AP2FlashError::Protocol(format!(
            "payload is too large: {payload_len} bytes"
        )));
    }

    let mut report = [0u8; HID_WRITE_SIZE];
    report[0] = HID_REPORT_ID;
    report[1] = LIANA_SOH;
    report[2] = LIANA_VERSION;
    report[3] = ((target as u8) << 4) | AP2Target::UsbHost as u8;
    report[4] = LIANA_SEQUENCE;
    report[5] = payload_len as u8;
    report[6] = 0;
    report[7] = 0;
    report[8] = LIANA_EOH;
    report[9] = command;
    report[10] = key;
    report[11..11 + args.len()].copy_from_slice(args);
    Ok(report)
}

fn parse_response(report: &[u8]) -> Result<ResponseFrame, AP2FlashError> {
    if report.len() < OUTER_HEADER_SIZE + COMMAND_HEADER_SIZE {
        return Err(AP2FlashError::Protocol(format!(
            "short HID response: {} bytes",
            report.len()
        )));
    }
    if report[0] != LIANA_SOH
        || report[1] != LIANA_VERSION
        || report[3] != LIANA_SEQUENCE
        || report[7] != LIANA_EOH
    {
        return Err(AP2FlashError::Protocol(format!(
            "invalid Liana frame header: {:02x?}",
            &report[..OUTER_HEADER_SIZE]
        )));
    }

    let payload_len =
        report[4] as usize | ((report[5] as usize) << 8) | ((report[6] as usize) << 16);
    if payload_len < COMMAND_HEADER_SIZE {
        return Err(AP2FlashError::Protocol(format!(
            "response payload is too short: {payload_len} bytes"
        )));
    }
    if OUTER_HEADER_SIZE + payload_len > report.len() {
        return Err(AP2FlashError::Protocol(format!(
            "truncated response payload: header says {payload_len}, report has {}",
            report.len() - OUTER_HEADER_SIZE
        )));
    }

    let route = report[2];
    Ok(ResponseFrame {
        source: route & 0x0f,
        destination: route >> 4,
        command: report[8],
        key: report[9],
        body: report[10..OUTER_HEADER_SIZE + payload_len].to_vec(),
    })
}

fn read_u32_le(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_ble_write_report() {
        let mut args = Vec::from(0x25fe0u32.to_le_bytes());
        args.extend(0u8..32);
        let report = build_report(
            AP2Target::McuBle,
            L2Command::Firmware as u8,
            KeyCommand::IapWriteMemory as u8,
            &args,
        )
        .unwrap();

        assert_eq!(report.len(), 65);
        assert_eq!(
            &report[..15],
            &[0x00, 0x7b, 0x10, 0x51, 0x10, 0x26, 0, 0, 0x7d, 0x02, 0x31, 0xe0, 0x5f, 0x02, 0]
        );
    }

    #[test]
    fn builds_official_iap_layout_request() {
        let report = build_report(
            AP2Target::McuMain,
            L2Command::Firmware as u8,
            KeyCommand::IapGetFwVersion as u8,
            &[],
        )
        .unwrap();

        assert_eq!(
            &report[..11],
            &[0x00, 0x7b, 0x10, 0x31, 0x10, 0x02, 0, 0, 0x7d, 0x02, 0x03]
        );
    }

    #[test]
    fn parses_recorded_success_response() {
        let mut report = [0u8; HID_REPORT_SIZE];
        report[..11].copy_from_slice(&[0x7b, 0x10, 0x15, 0x10, 0x03, 0, 0, 0x7d, 0x02, 0x31, 0]);

        let parsed = parse_response(&report).unwrap();
        assert_eq!(
            parsed,
            ResponseFrame {
                source: AP2Target::McuBle as u8,
                destination: AP2Target::UsbHost as u8,
                command: L2Command::Firmware as u8,
                key: KeyCommand::IapWriteMemory as u8,
                body: vec![0],
            }
        );
    }

    #[test]
    fn parses_recorded_error_response() {
        let mut report = [0u8; HID_REPORT_SIZE];
        report[..11].copy_from_slice(&[0x7b, 0x10, 0x15, 0x10, 0x03, 0, 0, 0x7d, 0x02, 0x31, 1]);

        let parsed = parse_response(&report).unwrap();
        let error = expect_success(
            AP2Target::McuBle,
            KeyCommand::IapWriteMemory as u8,
            &parsed.body,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            AP2FlashError::DeviceRejected {
                target: AP2Target::McuBle,
                command: 0x31,
                status: 1
            }
        ));
    }

    #[test]
    fn extracts_official_partition_offsets() {
        let mut body = [0u8; 26];
        body[2..6].copy_from_slice(&0x4000u32.to_le_bytes());
        body[12..16].copy_from_slice(&0x2000u32.to_le_bytes());
        body[22..26].copy_from_slice(&0x8000u32.to_le_bytes());

        assert_eq!(
            FirmwareLayout::from_response(&body).unwrap(),
            FirmwareLayout {
                main_base: 0x4000,
                led_base: 0x2000,
                ble_base: 0x8000,
            }
        );
    }

    #[test]
    fn rejects_truncated_frames() {
        let error = parse_response(&[0x7b, 0x10]).unwrap_err();
        assert!(matches!(error, AP2FlashError::Protocol(_)));
    }

    #[test]
    fn rejects_explicit_base_mismatch() {
        let layout = FirmwareLayout {
            main_base: 0x4000,
            led_base: 0x2000,
            ble_base: 0x8000,
        };
        assert_eq!(layout.base_for(AP2Target::McuBle).unwrap(), 0x8000);
        assert!(matches!(
            layout.base_for(AP2Target::UsbHost),
            Err(AP2FlashError::UnsupportedTarget(AP2Target::UsbHost))
        ));
    }

    #[test]
    fn rejects_empty_firmware_before_erase() {
        let mut empty = &[][..];
        let error = read_firmware_image(&mut empty).unwrap_err();
        assert!(matches!(error, AP2FlashError::Protocol(_)));
        assert!(error.to_string().contains("refusing to erase"));
    }

    #[test]
    fn rejects_image_address_overflow() {
        let error = validate_image_span(AP2Target::McuBle, u32::MAX - 15, 16).unwrap_err();
        assert!(matches!(error, AP2FlashError::Protocol(_)));
    }

    #[test]
    fn formats_transfer_progress() {
        assert_eq!(
            format_transfer_progress(4096, 16384, 0x5000),
            "Writing [======------------------]  25% 4.0 KiB/16.0 KiB (next 0x00005000)"
        );
        assert_eq!(
            format_transfer_progress(16384, 16384, 0x8000),
            "Writing [========================] 100% 16.0 KiB/16.0 KiB (next 0x00008000)"
        );
    }
}
