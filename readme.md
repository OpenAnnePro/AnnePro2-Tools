# Anne Pro 2 Tools

This is an alternative firmware update tool for the Anne Pro 2.
It allows you to flash custom firmware onto the Anne Pro 2.
Currently only the main MCU has been tested to work.

Please put the keyboard into IAP mode by holding down `esc` while
plugging it in to the computer before running this tool.

To build
```bash
cargo build --release
```

To flash file called a.bin you can invoke

```bash
./target/release/annepro2_tools a.bin
```

By default, the flasher will look for 04d9:8008 (Default Anne Pro 2 IAP)
and flash binary starting at 0x4000. 