# The center of the notifications of swaync on Super+n.

EAPI=8

DESCRIPTION="The center of the notifications of swaync on Super+n"
S=${WORKDIR}

LICENSE="GPL-2"
SLOT="0"
KEYWORDS="amd64 arm64"

RDEPEND="
	gui-apps/swaync
	gui-wm/sway
"

src_install() {
	insinto /etc/sway/config.d
	doins "${FILESDIR}"/60-swaync.conf
}
