# Rofi as the menu of programs of sway (Super+d).

EAPI=8

DESCRIPTION="Rofi as the menu of programs of sway (Super+d)"
S=${WORKDIR}

LICENSE="GPL-2"
SLOT="0"
KEYWORDS="amd64 arm64"

RDEPEND="
	x11-misc/rofi[wayland]
	gui-wm/sway
"

src_install() {
	insinto /etc/sway/config.d
	doins "${FILESDIR}"/60-rofi.conf
}
