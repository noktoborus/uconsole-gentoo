# The daemon of axp22x-soft-battery: it learns the cells from the ADC of
# the AXP22x (IIO axp22x-adc) and the status of the module, keeps the
# runs, writes the overlay of the curves (gen-dts), and powers the system
# off when the AXP is gone. The service is enabled by a preset.
#
# The source is daemon/ of the commit MY_COMMIT of
# github.com/noktoborus/axp22x-soft-battery; CRATES is its Cargo.lock.

EAPI=8

CRATES="
	lexopt@0.3.2
"

inherit cargo systemd

MY_COMMIT="7289c2c1c28609d9bb3433fb3d8a670523727ee4"

DESCRIPTION="Learns the cells of the uConsole from the ADC of the AXP22x"
HOMEPAGE="https://github.com/noktoborus/axp22x-soft-battery"
SRC_URI="
	https://github.com/noktoborus/axp22x-soft-battery/archive/${MY_COMMIT}.tar.gz -> axp22x-soft-battery-${MY_COMMIT}.tar.gz
	${CARGO_CRATE_URIS}
"
S=${WORKDIR}/axp22x-soft-battery-${MY_COMMIT}/daemon

LICENSE="GPL-2"
# the crates
LICENSE+=" MIT"
SLOT="0"
KEYWORDS="arm64"

RDEPEND="sys-power/axp22x-soft-battery"

src_install() {
	cargo_src_install
	systemd_dounit axp22x-soft-battery.service
	insinto "$(systemd_get_systempresetdir)"
	newins axp22x-soft-battery.preset 50-${PN}.preset
}
