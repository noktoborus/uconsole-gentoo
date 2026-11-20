# The login screen of the uConsole: greetd on VT 1 runs ReGreet in a sway
# of its own, with the panel, the backlight and the lid of the session
# (uconsole/sway-settings). Turn it on: systemctl enable greetd.service.

EAPI=8

inherit systemd

DESCRIPTION="The login screen of the uConsole: greetd, ReGreet in sway"
S=${WORKDIR}

LICENSE="GPL-2"
SLOT="0"
KEYWORDS="amd64 arm64"

RDEPEND="
	gui-libs/greetd
	gui-apps/ReGreet[systemd]
	gui-wm/sway
	gui-apps/swayidle
	sys-apps/dbus
	uconsole/sway-settings
"

src_install() {
	insinto /etc/greetd
	doins "${FILESDIR}"/uconsole.toml "${FILESDIR}"/sway-config
	systemd_install_serviced "${FILESDIR}"/greetd-service.conf greetd.service
}
