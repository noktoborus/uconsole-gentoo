#!/bin/sh
# The USB autosuspend of the profile drowse-cm5-powersave, run by TuneD:
# "start" when the profile is set, "stop" when it ends.
#
# The kernel lets a USB device suspend only with power/control=auto; it
# sets "on" for every device but a hub, and "auto" for a hub. "start"
# leaves the devices of KEEP as they are, sets "on" on the hubs they hang
# on, up to the root hub, so these never suspend, and "auto" on every
# other USB device; it keeps the values before;
# "stop" writes them back. A device then suspends after
# power/autosuspend_delay_ms with no use (2 s by default), and wakes on
# use.
#
# KEEP: the devices never touched, as vendor:product, split by spaces; a
# shell pattern, so 1eaf:* is every device of the vendor 1eaf.
# - 434b:5543 the keyboard of the uConsole, a USB device of the board.
# - 1eaf:* the bootloader of that keyboard (Leaflabs Maple DFU,
#   1eaf:0003): a fault of the keyboard leaves it there until a power
#   cycle.
# To see the devices:
#   grep -H . /sys/bus/usb/devices/*/idVendor /sys/bus/usb/devices/*/idProduct \
#       /sys/bus/usb/devices/*/product

KEEP="434b:5543 1eaf:*"
SAVED=/run/drowse-usb-control

# Whether a device "vendor:product" is one of KEEP. The patterns are not
# expanded as names of files (set -f).
kept() {
	set -f
	found=1
	for pattern in $KEEP; do
		# shellcheck disable=SC2254
		case "$1" in
			$pattern) found=0 ;;
		esac
	done
	set +f
	return "$found"
}

# Whether the value before of a device is kept already: a second "start"
# with no "stop" between (TuneD restarted with rollback=not_on_exit) must
# not take "auto" for the value before.
saved() {
	grep -q "^$1 " "$SAVED" 2>/dev/null
}

# The USB devices: the entries of /sys/bus/usb/devices with an idVendor
# (an interface has none).
devices() {
	for dev in /sys/bus/usb/devices/*; do
		[ -f "$dev/idVendor" ] && [ -f "$dev/power/control" ] && echo "$dev"
	done
}

# The real paths of the devices of KEEP, one a line. A hub is the
# directory of the devices on it, so a hub on the way to a kept device is
# a start of its path.
kept_paths() {
	devices | while read -r dev; do
		kept "$(cat "$dev/idVendor"):$(cat "$dev/idProduct")" && readlink -f "$dev"
	done
}

# What "start" does with a device: "keep" for one of KEEP, "on" for a hub
# on the way to one, "auto" for any other.
mode() {
	real=$(readlink -f "$1")
	echo "$KEPT_PATHS" | while read -r path; do
		case "$path" in
			"$real") echo keep ;;
			"$real"/*) echo on ;;
		esac
	done | sort -u | head -n 1 | grep . || echo auto
}

start() {
	touch "$SAVED" || return 1
	KEPT_PATHS=$(kept_paths)
	devices | while read -r dev; do
		control=$(mode "$dev")
		[ "$control" = keep ] && continue
		saved "$dev" || echo "$dev $(cat "$dev/power/control")" >> "$SAVED"
		echo "$control" > "$dev/power/control"
	done
}

stop() {
	[ -f "$SAVED" ] || return 0
	while read -r dev control; do
		[ -f "$dev/power/control" ] && echo "$control" > "$dev/power/control"
	done < "$SAVED"
	rm -f "$SAVED"
}

case "$1" in
	start) start ;;
	stop) stop ;;
esac
exit 0
