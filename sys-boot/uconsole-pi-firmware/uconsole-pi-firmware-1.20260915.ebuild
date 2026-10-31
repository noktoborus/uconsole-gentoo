# The files of the FAT partition of the ClockworkPi uConsole, /boot, as
# sys-boot/raspberrypi-firmware puts them, with the files of the uConsole:
# config.txt (the panel, the battery, the sound, USB host) and cmdline.txt
# (the root by its GPT name uconsole-root, the splash of the kernel).
# CONFIG_PROTECT keeps the changes of both on an update. The version is
# the one of the firmware of Raspberry Pi; the CM4 takes its start4*.elf
# and fixup4*.dat from it, the CM5 has them in its EEPROM.

EAPI=8

inherit mount-boot

DESCRIPTION="The boot files of the ClockworkPi uConsole: config.txt, cmdline.txt, the firmware of the CM4"
HOMEPAGE="https://github.com/raspberrypi/firmware"
SRC_URI="cm4? ( https://github.com/raspberrypi/firmware/archive/${PV}.tar.gz -> raspberrypi-firmware-${PV}.tar.gz )"
S=${WORKDIR}

LICENSE="GPL-2 cm4? ( raspberrypi-videocore-bin )"
SLOT="0"
KEYWORDS="arm64"
IUSE="cm5 cm4"
REQUIRED_USE="|| ( cm4 cm5 )"
RESTRICT="binchecks strip"
QA_PREBUILT="*"

# the same files in /boot
RDEPEND="!sys-boot/raspberrypi-firmware"

src_install() {
	insinto /boot
	if use cm4; then
		doins firmware-${PV}/boot/start4*.elf firmware-${PV}/boot/fixup4*.dat
	fi
	doins "${FILESDIR}"/config.txt "${FILESDIR}"/cmdline.txt
	newenvd "${FILESDIR}"/${PN}.envd 90${PN}
}

pkg_postinst() {
	elog "/boot/cmdline.txt finds the root by its GPT name:"
	elog "root=PARTLABEL=uconsole-root, the same on each card of the image."
	elog "With another disk of that name plugged in (a second such card in a"
	elog "reader of USB), the kernel may take the wrong root. To take the root"
	elog "of this card only, put its PARTUUID in /boot/cmdline.txt:"
	elog
	elog "  findmnt -no PARTUUID /"
	elog "  sed -i 's|root=PARTLABEL=uconsole-root|root=PARTUUID=<it>|' /boot/cmdline.txt"
	elog
	elog "CONFIG_PROTECT keeps the change on updates."
}
