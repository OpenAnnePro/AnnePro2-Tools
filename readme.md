# Anne Pro 2 Tools

This is an alternative firmware update tool for the Anne Pro 2. It can update
the main, LED, and BLE MCUs through the keyboard's USB IAP firmware.

Please put the keyboard into IAP mode by holding down `esc` while
plugging it in to the computer before running this tool.

If no IAP device is present yet, the tool waits up to 20 seconds and refreshes
the countdown in place. Firmware writes are shown as a single updating progress
bar instead of one log line per interval. When output is redirected, concise
start/completion messages are emitted without terminal control characters.

## Safety behavior

The tool reads the IAP layout reported by the keyboard before erasing anything.
An explicit `--base` is accepted only when it matches that device-reported
address. It reads and validates the complete input image before erasing the
target, matches each reply to the target/command/key that produced it, aborts
on non-zero status or timeout, and returns a non-zero process exit status on
failure.

The protocol does not currently provide a verified readback path. A successful
transfer therefore proves that every erase/write request received a success
status, not that flash contents were independently read back and compared.

Read the layout and target modes without writing:

```bash
./target/release/annepro2_tools --probe
```

## Build

```bash
cargo build --release
```

Flash a main-MCU image and leave the keyboard in IAP:

```bash
./target/release/annepro2_tools a.bin
```

Supported target names are `main`/`key`, `led`, and `ble`. Do not supply
`--base` during normal use; the value is discovered from IAP. The option exists
for diagnostics and refuses mismatches.

## Flash BLE firmware

1. Disconnect the keyboard.
2. Hold `Esc` while reconnecting the USB cable to enter IAP mode.
3. Optionally inspect the detected layout and IAP modes without writing:

   ```bash
   ./target/release/annepro2_tools --probe
   ```

4. Flash the BLE image and restart the keyboard:

   ```bash
   ./target/release/annepro2_tools --target ble --boot ble.bin
   ```

Replace `ble.bin` with the path to the BLE update image. Do not pass `--base`
unless diagnosing an IAP layout: the tool reads the BLE transport base from
the keyboard and rejects a conflicting override.

The BLE transfer erases the BLE application region and sends the image in
32-byte chunks. It stops on the first timeout, malformed response, target or
command mismatch, or non-zero device status. `--boot` restarts the keyboard
only after every chunk has received a matching status-zero reply. Omit
`--boot` if the keyboard should remain in IAP mode for another operation.

Only use a BLE image intended for the keyboard hardware being updated. The
tool validates the IAP transaction but cannot identify whether an arbitrary
image is compatible with a particular Anne Pro 2 model. A successful transfer
also does not provide byte-for-byte flash readback; verify that the BLE
firmware boots, advertises, connects, and sends keyboard input afterward.

## Nix flake support

For development, run `nix-shell` or `nix develop`.

The flake can also run the tool directly:

```shell
nix run github:OpenAnnePro/AnnePro2-Tools -- --help
nix run github:OpenAnnePro/AnnePro2-Tools/master -- --help
nix run github:OpenAnnePro/AnnePro2-Tools -- --target ble --boot ble.bin
```
