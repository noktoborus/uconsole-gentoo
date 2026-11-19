# The daemon of axp22x-soft-battery: it learns the cells from the ADC of
# the AXP22x (IIO axp22x-adc) and the status of the module, keeps the
# runs, writes the overlay of the curves (gen-dts), and powers the system
# off when the AXP is gone. The service is enabled by a preset.
#
# The source is a copy of daemon/ of axp22x-soft-battery in
# files/daemon, with the sum below;
# CRATES is Cargo.lock.

EAPI=8

CRATES="
	lexopt@0.3.2
"

inherit cargo systemd

DESCRIPTION="Learns the cells of the uConsole from the ADC of the AXP22x"
HOMEPAGE="https://www.clockworkpi.com/uconsole"
SRC_URI="${CARGO_CRATE_URIS}"
S=${WORKDIR}/daemon

LICENSE="GPL-2"
# the crates
LICENSE+=" MIT"
SLOT="0"
KEYWORDS="arm64"

RDEPEND="sys-power/axp22x-soft-battery"

# files/ sha256: 780f176476debd5e05d8d867333d9cee96adf63beedf9abcdbc85544deb03cfe

src_unpack() {
	cargo_src_unpack
	# the modes of files/ are not kept, as in sys-power/axp22x-soft-battery
	cp -r --no-preserve=mode "${FILESDIR}"/daemon/. "${S}" || die
}

src_install() {
	cargo_src_install
	systemd_dounit axp22x-soft-battery.service
	insinto "$(systemd_get_systempresetdir)"
	newins axp22x-soft-battery.preset 50-${PN}.preset
}
