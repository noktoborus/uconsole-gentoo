# The battery of the uConsole by a model of the device tree, in place of
# axp20x_battery: the module axp22x_soft_battery, the blacklist of
# axp20x_battery, and the overlay axp22x-soft-battery.dtbo with every
# setting of the module (it has no default of its own).
#
# The source is a copy of module/ of axp22x-soft-battery in
# files/module, with the sum below.

EAPI=8

inherit linux-mod-r1 mount-boot

DESCRIPTION="Battery of the uConsole by a model of the device tree, in place of axp20x_battery"
HOMEPAGE="https://www.clockworkpi.com/uconsole"
S=${WORKDIR}/module

LICENSE="GPL-2"
SLOT="0"
KEYWORDS="arm64"

BDEPEND="sys-apps/dtc"

# files/ sha256: 39c6146cfe088500055540dfdb66b2a1d5ef9df30131d307e615e0009f04faa7

CONFIG_CHECK="MFD_AXP20X AXP20X_ADC AXP20X_POWER IIO POWER_SUPPLY"
ERROR_AXP20X_ADC="the module reads the battery by the IIO channels of axp20x_adc"
ERROR_AXP20X_POWER="the charger in or out comes from axp20x-ac"

pkg_setup() {
	linux-mod-r1_pkg_setup
	# built in, axp20x_battery binds first and the blacklist cannot stop it
	linux_chkconfig_builtin BATTERY_AXP20X &&
		die "CONFIG_BATTERY_AXP20X=y: axp20x_battery must be a module (m) or off (n)"
}

src_unpack() {
	# the modes of files/ are not kept: a copy with 0700 from the host
	# would close ${S} to the user portage
	mkdir "${S}" || die
	cp -r --no-preserve=mode "${FILESDIR}"/module/. "${S}" || die
}

src_compile() {
	local modlist=( axp22x_soft_battery=kernel/drivers/power/supply )
	local modargs=( KDIR="${KV_OUT_DIR}" )
	linux-mod-r1_src_compile

	dtc -@ -I dts -O dtb -o axp22x-soft-battery.dtbo axp22x-soft-battery.dts || die
}

src_install() {
	linux-mod-r1_src_install

	insinto /etc/modprobe.d
	doins axp22x-soft-battery.conf
	insinto /boot/overlays
	doins axp22x-soft-battery.dtbo
}

pkg_postinst() {
	linux-mod-r1_pkg_postinst
	mount-boot_pkg_postinst

	elog "No package changes config.txt. Add by hand, in [pi5], after the"
	elog "overlay of the device:"
	elog "    dtoverlay=clockworkpi-uconsole-cm5"
	elog "    dtoverlay=axp22x-soft-battery"
	elog "The overlay of the curves of 'axp22x-soft-battery gen-dts' goes after it."
}
