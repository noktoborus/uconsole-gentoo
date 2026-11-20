# Sway on the uConsole for each user: the panel turned (DSI-1 of the CM4,
# DSI-2 of the CM5), the lid (a short press of the power key) turns the
# panel off and on, the keys of the backlight by /usr/bin/backlight
# (logind), and sway-session.target, which runs /etc/xdg/autostart.

EAPI=8

inherit systemd

DESCRIPTION="Sway on the uConsole: the panel, the lid, the backlight, the session of systemd"
S=${WORKDIR}

LICENSE="GPL-2"
SLOT="0"
KEYWORDS="amd64 arm64"

RDEPEND="
	gui-wm/sway
	sys-apps/systemd
"

src_install() {
	insinto /etc/sway/config.d
	doins "${FILESDIR}"/40-uconsole-output.conf "${FILESDIR}"/50-uconsole.conf \
		"${FILESDIR}"/50-uconsole-backlight.conf "${FILESDIR}"/50-uconsole-bindings.conf
	systemd_douserunit "${FILESDIR}"/sway-session.target
	dobin "${FILESDIR}"/backlight
}
