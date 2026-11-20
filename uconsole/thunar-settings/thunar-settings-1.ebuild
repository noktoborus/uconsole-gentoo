# Thunar runs in the background from /etc/xdg/autostart, so its windows
# open at once and thunar-volman mounts what is plugged in, by the defaults
# of /etc/xdg/xfce4/xfconf.

EAPI=8

DESCRIPTION="Thunar in the background; thunar-volman mounts what is plugged in"
S=${WORKDIR}

LICENSE="GPL-2"
SLOT="0"
KEYWORDS="amd64 arm64"

RDEPEND="
	xfce-base/thunar
	xfce-base/thunar-volman
"

src_install() {
	insinto /etc/xdg/xfce4/xfconf/xfce-perchannel-xml
	doins "${FILESDIR}"/thunar-volman.xml
	insinto /etc/xdg/autostart
	doins "${FILESDIR}"/thunar-daemon.desktop
}
