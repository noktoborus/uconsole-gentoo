# zyterm as the terminal of sway: Super+Enter, and $term.

EAPI=8

DESCRIPTION="zyterm as the terminal of sway"
S=${WORKDIR}

LICENSE="GPL-2"
SLOT="0"
KEYWORDS="amd64 arm64"

RDEPEND="
	x11-terms/zyterm
	gui-wm/sway
"

src_install() {
	insinto /etc/sway/config.d
	doins "${FILESDIR}"/60-zyterm.conf
}
