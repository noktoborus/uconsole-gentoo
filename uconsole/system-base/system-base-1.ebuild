# What the uConsole needs to boot, reach the network and build: the
# kernel and the firmware of the board (USE of the profile), the battery,
# NetworkManager, DNS, the power key, zram, the UCM, the caches of emerge.

EAPI=8

DESCRIPTION="The system of the uConsole with no desktop: the kernel, the firmware, the battery, the network, the sound card"
HOMEPAGE="https://www.clockworkpi.com/uconsole"

LICENSE="metapackage"
SLOT="0"
KEYWORDS="arm64"

RDEPEND="
	sys-kernel/uconsole-pi-kernel
	sys-boot/uconsole-pi-firmware
	sys-kernel/linux-firmware
	sys-firmware/bluez-firmware
	sys-power/axp22x-soft-battery
	sys-power/axp22x-soft-battery-daemon
	sys-apps/dtc
	net-misc/networkmanager
	uconsole/networkmanager-settings
	net-dns/avahi
	uconsole/resolved-settings
	uconsole/logind-settings
	uconsole/zram-generator-settings
	media-libs/uconsole-pi-ucm
	app-portage/portage-ccache
	app-portage/portage-sccache
"
