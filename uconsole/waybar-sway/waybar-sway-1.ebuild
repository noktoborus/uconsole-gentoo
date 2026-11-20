# Waybar in place of swaybar.

EAPI=8

DESCRIPTION="Waybar in place of swaybar"
S=${WORKDIR}

LICENSE="GPL-2"
SLOT="0"
KEYWORDS="amd64 arm64"

RDEPEND="
	gui-apps/waybar
	gui-wm/sway
"

src_install() {
	insinto /etc/sway/config.d
	doins "${FILESDIR}"/60-waybar.conf
}
