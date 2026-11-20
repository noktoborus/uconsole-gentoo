# logind acts on the short press of the power key only: it powers off.
# Every other key, the lid and the idle state are ignored by logind.

EAPI=8

inherit systemd

DESCRIPTION="logind acts on the power key only: a press powers off"
S=${WORKDIR}

LICENSE="GPL-2"
SLOT="0"
KEYWORDS="amd64 arm64"

RDEPEND="
	sys-apps/systemd
"

src_install() {
	insinto "$(systemd_get_utildir)"/logind.conf.d
	newins "${FILESDIR}"/logind.conf 50-uconsole.conf
}
