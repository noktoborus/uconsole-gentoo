# Plymouth quits with --retain-splash: the login screen draws over its
# last picture, with no black screen between.

EAPI=8

inherit systemd

DESCRIPTION="Plymouth keeps its last picture until the login screen"
S=${WORKDIR}

LICENSE="GPL-2"
SLOT="0"
KEYWORDS="amd64 arm64"

RDEPEND="
	sys-boot/plymouth
"

src_install() {
	systemd_install_serviced "${FILESDIR}"/plymouth-quit-service.conf plymouth-quit.service
}
