#!/bin/sh
# Every radio off, for the profile cm5-powersave-no-radio, run by TuneD:
# "start" when the profile is set, "stop" when it ends.
#
# "start" sets the soft block of every device of rfkill
# (/sys/class/rfkill/rfkill*/soft = 1) and keeps the values before;
# "stop" writes them back. So a radio the user had off stays off. A
# device gone in between (a module unloaded) is passed over. NetworkManager
# and bluetoothd follow the soft block by themselves.

SAVED=/run/cm5-powersave-no-radio-rfkill

# Whether the value before of a device is kept already: a second "start"
# with no "stop" between (TuneD restarted with rollback=not_on_exit) must
# not take "1" for the value before.
saved() {
	grep -q "^$1 " "$SAVED" 2>/dev/null
}

start() {
	touch "$SAVED" || return 1
	for dev in /sys/class/rfkill/rfkill*; do
		[ -f "$dev/soft" ] || continue
		real=$(readlink -f "$dev")
		saved "$real" || echo "$real $(cat "$dev/soft")" >> "$SAVED"
		echo 1 > "$dev/soft"
	done
}

stop() {
	[ -f "$SAVED" ] || return 0
	while read -r dev soft; do
		[ -f "$dev/soft" ] && echo "$soft" > "$dev/soft"
	done < "$SAVED"
	rm -f "$SAVED"
}

case "$1" in
	start) start ;;
	stop) stop ;;
esac
exit 0
