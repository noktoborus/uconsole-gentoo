# The daemon of axp22x-soft-battery: it learns the cells from the battery
# of the module, keeps the runs, writes the overlay of the curves
# (gen-dts), and powers the system off at the silence of the AXP. The
# service is enabled by a preset.
#
# The source is a copy of daemon/ of axp22x-soft-battery in
# files/daemon, with the sum below;
# CRATES is Cargo.lock.

EAPI=8

CRATES="
	lexopt@0.3.2
"

inherit cargo systemd

DESCRIPTION="Learns the cells of the uConsole from the battery of axp22x-soft-battery"
HOMEPAGE="https://www.clockworkpi.com/uconsole"
SRC_URI="${CARGO_CRATE_URIS}"
S=${WORKDIR}/daemon

LICENSE="GPL-2"
# the crates
LICENSE+=" MIT"
SLOT="0"
KEYWORDS="arm64"

RDEPEND="sys-power/axp22x-soft-battery"

# files/ sha256: 4bd3f09af815999237d549ffa46b8987882df9ec5068c65369d259c813c45568

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
