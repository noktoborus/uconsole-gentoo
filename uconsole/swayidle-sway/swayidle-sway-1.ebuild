# The lock and the idle panel of sway: lock at 5 min, the panel off at 10.

EAPI=8

DESCRIPTION="The lock and the idle panel of sway: lock at 5 min, the panel off at 10"
S=${WORKDIR}

LICENSE="GPL-2"
SLOT="0"
KEYWORDS="amd64 arm64"

RDEPEND="
	gui-apps/swayidle
	gui-apps/swaylock
	gui-wm/sway
"

src_install() {
	insinto /etc/sway/config.d
	doins "${FILESDIR}"/60-swayidle.conf
}
