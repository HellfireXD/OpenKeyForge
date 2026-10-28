#!/bin/sh
# Apply the keypad udev rule without a reboot. A keypad that was already
# plugged in picks it up on the trigger; failure here never fails the install.
if command -v udevadm >/dev/null 2>&1; then
    udevadm control --reload-rules 2>/dev/null || true
    udevadm trigger --subsystem-match=hidraw --subsystem-match=usb 2>/dev/null || true
fi
exit 0
